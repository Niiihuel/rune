//! Push-to-talk keys: the recorded key's name, whether the window holds it,
//! and the desktop's global shortcut session that holds it elsewhere.
//!
//! The window's key is read from the keys limn's tree holds, which it lets
//! go of when the window loses focus, so a release the window never saw
//! cannot leave the microphone open. The global key ends held whenever its
//! session ends.
//!
//! The same session carries the voice shortcuts of the desktop work (P6):
//! mute, deafen and push to mute, as Serein binds them; one portal session
//! asks the desktop for all of them at once.

use std::collections::HashSet;
use std::sync::Arc;

use eframe::egui;
use limn::{Key, Modifiers};
use rune_i18n::{es, t, tr};
use rune_model::UserId;
use rune_platform::shortcuts::{GlobalShortcuts, Shortcut, ShortcutEvent};
use rune_ui::desktop::Shortcut as VoiceKey;
use rune_ui::{GlobalKeys, Ui};

const ID: &str = "push-to-talk";

/// A key as the settings keep it: its modifiers, then its name, the one
/// egui gave it before limn knew every key, so older settings still read.
pub fn binding(key: Key, modifiers: Modifiers) -> String {
    let mut name = String::new();
    for (held, modifier) in [
        (modifiers.ctrl, "Ctrl+"),
        (modifiers.alt, "Alt+"),
        (modifiers.shift, "Shift+"),
    ] {
        if held {
            name.push_str(modifier);
        }
    }
    name.push_str(key.name());
    name
}

/// The key and the modifiers `binding` names; `None` for nonsense.
fn parse(binding: &str) -> Option<(Key, Modifiers)> {
    let (modifiers, key) = binding.rsplit_once('+').unwrap_or(("", binding));
    let mut held = Modifiers::default();
    for modifier in modifiers.split('+').filter(|part| !part.is_empty()) {
        match modifier {
            "Ctrl" => held.ctrl = true,
            "Alt" => held.alt = true,
            "Shift" => held.shift = true,
            _ => return None,
        }
    }
    Some((Key::from_name(key)?, held))
}

/// Whether `held` has at least the modifiers `wanted`: an extra one held
/// does not stop a key.
fn covers(held: Modifiers, wanted: Modifiers) -> bool {
    (!wanted.ctrl || held.ctrl) && (!wanted.alt || held.alt) && (!wanted.shift || held.shift)
}

/// Whether the window holds `binding` now: its key and its modifiers, as
/// the tree keeps them (none once the window loses the focus).
pub fn window_held(tree: &limn::UiTree, binding: &str) -> bool {
    parse(binding)
        .is_some_and(|(key, wanted)| tree.key_held(key) && covers(tree.modifiers(), wanted))
}

/// Whether the presses the window saw in this frame include `binding`,
/// with its modifiers (P6): a toggle acts once per press, never on a key
/// repeating, which the host leaves out of `pressed`.
pub fn window_pressed(pressed: &[(Key, Modifiers)], binding: &str) -> bool {
    parse(binding).is_some_and(|(key, wanted)| {
        pressed
            .iter()
            .any(|(press, held)| *press == key && covers(*held, wanted))
    })
}

/// The freedesktop shortcuts format of `binding`, for keys whose keysym name
/// is plain; the desktop lets the person choose when there is none.
fn trigger(binding: &str) -> Option<String> {
    let (key, modifiers) = parse(binding)?;
    let name = key.name();
    let keysym = if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() {
        name.to_ascii_lowercase()
    } else if function_key(key) {
        name.to_owned()
    } else if key == Key::Space {
        "space".to_owned()
    } else {
        return None;
    };
    let mut trigger = String::new();
    for (held, modifier) in [
        (modifiers.ctrl, "CTRL+"),
        (modifiers.alt, "ALT+"),
        (modifiers.shift, "SHIFT+"),
    ] {
        if held {
            trigger.push_str(modifier);
        }
    }
    trigger.push_str(&keysym);
    Some(trigger)
}

fn function_key(key: Key) -> bool {
    let name = key.name();
    name.len() > 1 && name.starts_with('F') && name[1..].bytes().all(|b| b.is_ascii_digit())
}

/// A key held everywhere would also type everywhere: only a combination
/// or a function key is asked of the desktop, as Serein does.
pub fn global_allowed(binding: &str) -> bool {
    parse(binding)
        .is_some_and(|(key, modifiers)| modifiers != Modifiers::default() || function_key(key))
}

/// The desktop's push-to-talk session: one per account and set of keys,
/// while push-to-talk or a voice shortcut is on and the window is not
/// closing.
#[derive(Default)]
pub struct GlobalPushToTalk {
    session: Option<GlobalShortcuts>,
    /// What the session was asked for, each shortcut's id and key; another
    /// value starts a new one.
    wanted: Option<(UserId, Vec<(String, String)>)>,
    /// The shortcuts held now, by id.
    held: HashSet<String>,
    /// The shortcuts the desktop bound, which the window then leaves to it.
    bound: HashSet<String>,
    /// Mute and deafen pressed since last taken (P6).
    toggles: Vec<VoiceKey>,
}

impl GlobalPushToTalk {
    pub fn held(&self) -> bool {
        self.held.contains(ID)
    }

    /// The push-to-mute key is held outside the window (P6).
    pub fn muting(&self) -> bool {
        self.held.contains(VoiceKey::PushToMute.key())
    }

    /// Whether the desktop bound `shortcut`: then it, not the window, says
    /// when it is pressed.
    pub fn bound(&self, shortcut: VoiceKey) -> bool {
        self.bound.contains(shortcut.key())
    }

    /// Mute and deafen pressed outside the window since the last call.
    pub fn take_toggles(&mut self) -> Vec<VoiceKey> {
        std::mem::take(&mut self.toggles)
    }

    /// Start, replace or end the session for what `ui` asks now, then take
    /// what it reported. `start` is the portal, or a test's stand-in.
    pub fn sync(
        &mut self,
        ui: &mut Ui,
        account: Option<UserId>,
        open: bool,
        start: impl FnOnce(Vec<Shortcut>) -> Option<GlobalShortcuts>,
    ) {
        let key = (ui.prefs.push_to_talk && open)
            .then_some(ui.prefs.push_to_talk_key.as_deref())
            .flatten()
            .filter(|_| account.is_some());
        let mut keys: Vec<(String, String)> = key
            .filter(|key| global_allowed(key))
            .map(|key| (ID.to_owned(), key.to_owned()))
            .into_iter()
            .collect();
        // Desktop (P6): the voice shortcuts, each with a key fit to go global.
        if open && account.is_some() {
            keys.extend(VoiceKey::GLOBAL.into_iter().filter_map(|shortcut| {
                shortcut
                    .binding(&ui.prefs)
                    .filter(|key| global_allowed(key))
                    .map(|key| (shortcut.key().to_owned(), key.to_owned()))
            }));
        }
        let wanted = account
            .filter(|_| !keys.is_empty())
            .map(|account| (account, keys));
        if wanted != self.wanted {
            // Dropping the old session closes it and releases its keys.
            self.session = None;
            self.held.clear();
            self.bound.clear();
            self.wanted = wanted;
            if let Some((_, keys)) = &self.wanted {
                self.session = start(
                    keys.iter()
                        .map(|(id, key)| Shortcut {
                            id: id.clone(),
                            description: match id.as_str() {
                                ID => t("Pulsar para hablar en RUNE").into(),
                                id => tr!(
                                    "{shortcut} en RUNE",
                                    shortcut = VoiceKey::GLOBAL
                                        .into_iter()
                                        .find(|shortcut| shortcut.key() == id)
                                        .map_or(t("Atajo"), |shortcut| t(shortcut.label()))
                                ),
                            },
                            preferred_trigger: trigger(key),
                        })
                        .collect(),
                );
                ui.audio.global_keys = GlobalKeys::Waiting;
            }
        }
        if self.wanted.is_none() {
            match key {
                None => ui.audio.global_keys = GlobalKeys::Off,
                Some(_) if !matches!(ui.audio.global_keys, GlobalKeys::Unavailable(_)) => {
                    ui.audio.global_keys = GlobalKeys::Unavailable(
                        es("el atajo necesita Ctrl, Alt o Shift, o ser una tecla F, para no escribirse en otras ventanas.")
                            .into(),
                    );
                }
                Some(_) => {}
            }
        }
        while let Some(event) = self.session.as_ref().and_then(GlobalShortcuts::poll) {
            self.apply(ui, event);
        }
    }

    fn apply(&mut self, ui: &mut Ui, event: ShortcutEvent) {
        match event {
            ShortcutEvent::Bound(bound) => {
                self.bound = bound.iter().map(|(id, _)| id.clone()).collect();
                // The first one tells how the desktop describes them.
                let trigger = bound
                    .iter()
                    .find(|(id, _)| id == ID)
                    .or(bound.first())
                    .map(|(_, trigger)| trigger.clone());
                if let Some(trigger) = trigger {
                    ui.audio.global_keys = GlobalKeys::Bound(trigger);
                }
            }
            ShortcutEvent::Pressed(id) => {
                // Desktop (P6): mute and deafen act once per press.
                if let Some(toggle) = [VoiceKey::Mute, VoiceKey::Deafen]
                    .into_iter()
                    .find(|shortcut| shortcut.key() == id)
                {
                    self.toggles.push(toggle);
                }
                if self
                    .wanted
                    .as_ref()
                    .is_some_and(|(_, keys)| keys.iter().any(|(known, _)| *known == id))
                {
                    self.held.insert(id);
                }
            }
            ShortcutEvent::Released(id) => {
                self.held.remove(&id);
            }
            ShortcutEvent::Ended(reason) => {
                // Keep `wanted`: the same keys are not asked again until they change.
                self.session = None;
                self.held.clear();
                self.bound.clear();
                ui.audio.global_keys = GlobalKeys::Unavailable(reason.to_string());
            }
        }
    }
}

/// The real portal session, waking the window on each event.
pub fn portal(ctx: &egui::Context) -> impl FnOnce(Vec<Shortcut>) -> Option<GlobalShortcuts> {
    let ctx = ctx.clone();
    move |shortcuts| {
        Some(GlobalShortcuts::start(
            shortcuts,
            Arc::new(move || ctx.request_repaint()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_platform::shortcuts::ShortcutError;

    #[test]
    fn a_binding_names_its_modifiers_and_key_and_reads_back() {
        let ctrl_shift = Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        let named = binding(Key::T, ctrl_shift);
        assert_eq!(named, "Ctrl+Shift+T");
        assert_eq!(parse(&named), Some((Key::T, ctrl_shift)));
        assert_eq!(trigger(&named).as_deref(), Some("CTRL+SHIFT+t"));
        assert_eq!(trigger("F13").as_deref(), Some("F13"));
        assert_eq!(trigger("Alt+Backtick"), None, "the desktop chooses");
        assert_eq!(parse("Plus").map(|(key, _)| key), Some(Key::Plus));
        // Every key limn knows, digits and punctuation too, keeps egui's
        // name, so a key recorded before reads the same.
        assert_eq!(binding(Key::Num1, Modifiers::default()), "1");
        assert_eq!(binding(Key::F13, Modifiers::default()), "F13");
        assert_eq!(
            parse("Alt+Backtick").map(|(key, _)| key),
            Some(Key::Backtick)
        );
        assert_eq!(trigger("Ctrl+1").as_deref(), Some("CTRL+1"));
        assert_eq!(parse("Super+T"), None);
        assert_eq!(parse("Ctrl+Nothing"), None);
    }

    #[test]
    fn only_combinations_and_function_keys_go_global() {
        assert!(global_allowed("Ctrl+V"));
        assert!(global_allowed("F9"));
        assert!(!global_allowed("V"), "it would type everywhere");
        assert!(!global_allowed("Backtick"));
    }

    #[test]
    fn the_window_holds_its_key_only_while_focused_and_with_its_modifiers() {
        let mut tree = limn::UiTree::new(limn::Element::spacer());
        let key = |key, ctrl| limn::UiEvent::Key {
            key,
            shift: false,
            ctrl,
            alt: false,
        };
        tree.handle_event(key(Key::T, false));
        assert!(!window_held(&tree, "Ctrl+T"), "Ctrl is not held");
        tree.handle_event(limn::UiEvent::ModifiersChanged(Modifiers {
            ctrl: true,
            ..Default::default()
        }));
        assert!(window_held(&tree, "Ctrl+T"));
        assert!(window_held(&tree, "T"), "extra modifiers do not stop it");
        tree.handle_event(limn::UiEvent::KeyReleased { key: Key::T });
        assert!(!window_held(&tree, "Ctrl+T"), "released");
        tree.handle_event(key(Key::Num1, true));
        assert!(window_held(&tree, "Ctrl+1"), "a digit holds too");
        tree.set_window_focused(false);
        assert!(!window_held(&tree, "Ctrl+1"), "a lost focus releases it");
    }

    #[test]
    fn a_toggle_matches_the_presses_of_the_frame_with_their_modifiers() {
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        let pressed = [(Key::M, ctrl), (Key::F9, Modifiers::default())];
        assert!(window_pressed(&pressed, "Ctrl+M"));
        assert!(window_pressed(&pressed, "F9"));
        assert!(
            !window_pressed(&pressed, "Ctrl+Shift+M"),
            "Shift was not held"
        );
        assert!(!window_pressed(&[], "Ctrl+M"));
    }

    fn ptt(key: &str) -> Ui {
        let mut ui = Ui::new();
        ui.prefs.push_to_talk = true;
        ui.prefs.push_to_talk_key = Some(key.into());
        ui
    }

    #[test]
    fn the_session_follows_the_setting_the_account_and_closing_and_its_end_releases() {
        let me = Some(UserId(1));
        let mut global = GlobalPushToTalk::default();
        let mut ui = ptt("Ctrl+T");
        let mut asked = Vec::new();
        global.sync(&mut ui, me, true, |shortcuts| {
            asked = shortcuts;
            None
        });
        assert_eq!(asked[0].preferred_trigger.as_deref(), Some("CTRL+t"));
        assert_eq!(ui.audio.global_keys, GlobalKeys::Waiting);
        global.sync(&mut ui, me, true, |_| {
            panic!("the same key is not asked twice")
        });
        global.apply(
            &mut ui,
            ShortcutEvent::Bound(vec![(ID.into(), "Ctrl+T".into())]),
        );
        assert_eq!(ui.audio.global_keys, GlobalKeys::Bound("Ctrl+T".into()));
        global.apply(&mut ui, ShortcutEvent::Pressed("other".into()));
        assert!(!global.held());
        global.apply(&mut ui, ShortcutEvent::Pressed(ID.into()));
        assert!(global.held());
        // The desktop ending the session releases a key it never released.
        global.apply(&mut ui, ShortcutEvent::Ended(ShortcutError::CLOSED));
        assert!(!global.held());
        assert!(matches!(ui.audio.global_keys, GlobalKeys::Unavailable(_)));
        global.sync(&mut ui, me, true, |_| {
            panic!("an ended session is not retried by itself")
        });
        // Another account asks again; closing, or turning it off, ends it.
        global.apply(&mut ui, ShortcutEvent::Pressed(ID.into()));
        let mut started = 0;
        global.sync(&mut ui, Some(UserId(2)), true, |_| {
            started += 1;
            None
        });
        assert_eq!(started, 1);
        assert!(!global.held(), "a new session starts released");
        global.sync(&mut ui, Some(UserId(2)), false, |_| {
            panic!("closing asks nothing")
        });
        assert_eq!(ui.audio.global_keys, GlobalKeys::Off);
        ui.prefs.push_to_talk = false;
        global.sync(&mut ui, me, true, |_| panic!("off asks nothing"));
        assert_eq!(ui.audio.global_keys, GlobalKeys::Off);
    }

    // Desktop (P6).
    #[test]
    fn voice_shortcuts_share_the_session_and_mute_and_deafen_act_once_per_press() {
        let me = Some(UserId(1));
        let mut global = GlobalPushToTalk::default();
        let mut ui = Ui::new();
        ui.prefs.mute_key = Some("Ctrl+Shift+M".into());
        ui.prefs.deafen_key = Some("D".into());
        ui.prefs.push_to_mute_key = Some("F8".into());
        let mut asked = Vec::new();
        global.sync(&mut ui, me, true, |shortcuts| {
            asked = shortcuts;
            None
        });
        let ids: Vec<&str> = asked.iter().map(|shortcut| shortcut.id.as_str()).collect();
        assert_eq!(
            ids,
            ["mute", "ptm"],
            "a plain key is not asked of the desktop"
        );
        assert_eq!(asked[0].preferred_trigger.as_deref(), Some("CTRL+SHIFT+m"));
        assert_eq!(ui.audio.global_keys, GlobalKeys::Waiting);
        global.apply(
            &mut ui,
            ShortcutEvent::Bound(vec![
                ("mute".into(), "Ctrl+Shift+M".into()),
                ("ptm".into(), "F8".into()),
            ]),
        );
        assert!(global.bound(VoiceKey::Mute) && !global.bound(VoiceKey::Deafen));
        global.apply(&mut ui, ShortcutEvent::Pressed("mute".into()));
        global.apply(&mut ui, ShortcutEvent::Released("mute".into()));
        global.apply(&mut ui, ShortcutEvent::Pressed("ptm".into()));
        assert_eq!(global.take_toggles(), [VoiceKey::Mute]);
        assert!(global.take_toggles().is_empty());
        assert!(global.muting() && !global.held());
        // A key changed asks again, and starts with nothing held.
        ui.prefs.push_to_mute_key = Some("F9".into());
        let mut again = 0;
        global.sync(&mut ui, me, true, |_| {
            again += 1;
            None
        });
        assert_eq!(again, 1);
        assert!(!global.muting());
    }

    #[test]
    fn a_plain_key_stays_in_the_window_and_says_why() {
        let mut global = GlobalPushToTalk::default();
        let mut ui = ptt("V");
        global.sync(&mut ui, Some(UserId(1)), true, |_| {
            panic!("a plain key is never global")
        });
        assert!(matches!(ui.audio.global_keys, GlobalKeys::Unavailable(_)));
    }
}
