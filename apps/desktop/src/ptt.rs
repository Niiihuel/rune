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
use limn::{Key, Modifiers, OtherButton};
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

/// The name a mouse button keeps: the middle and the side buttons, as the
/// Windows hook names them.
pub fn mouse_binding(button: OtherButton) -> &'static str {
    match button {
        OtherButton::Middle => "MOUSE3",
        OtherButton::Back => "MOUSE4",
        OtherButton::Forward => "MOUSE5",
    }
}

/// The mouse button `binding` names, if it names one.
fn mouse(binding: &str) -> Option<OtherButton> {
    [OtherButton::Middle, OtherButton::Back, OtherButton::Forward]
        .into_iter()
        .find(|button| mouse_binding(*button) == binding)
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
/// the tree keeps them (none once the window loses the focus), or its mouse
/// button, as `buttons` keeps them.
pub fn window_held(tree: &limn::UiTree, buttons: &[OtherButton], binding: &str) -> bool {
    if let Some(button) = mouse(binding) {
        return buttons.contains(&button);
    }
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

/// The freedesktop shortcuts format of `binding`, with the keysym names of
/// the keys that have one; the desktop lets the person choose when there is none.
fn trigger(binding: &str) -> Option<String> {
    if mouse(binding).is_some() {
        return Some(binding.to_owned());
    }
    let (key, modifiers) = parse(binding)?;
    let name = key.name();
    let keysym = match key {
        Key::Space => "space".to_owned(),
        Key::Backtick => "grave".to_owned(),
        Key::Minus => "minus".to_owned(),
        Key::Equals => "equal".to_owned(),
        Key::Comma => "comma".to_owned(),
        Key::Period => "period".to_owned(),
        Key::Slash => "slash".to_owned(),
        Key::Semicolon => "semicolon".to_owned(),
        Key::Quote => "apostrophe".to_owned(),
        Key::OpenBracket => "bracketleft".to_owned(),
        Key::CloseBracket => "bracketright".to_owned(),
        Key::Backslash => "backslash".to_owned(),
        Key::ArrowLeft => "Left".to_owned(),
        Key::ArrowRight => "Right".to_owned(),
        Key::ArrowUp => "Up".to_owned(),
        Key::ArrowDown => "Down".to_owned(),
        Key::Home => "Home".to_owned(),
        Key::End => "End".to_owned(),
        Key::PageUp => "Page_Up".to_owned(),
        Key::PageDown => "Page_Down".to_owned(),
        Key::Insert => "Insert".to_owned(),
        Key::Delete => "Delete".to_owned(),
        _ if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() => {
            name.to_ascii_lowercase()
        }
        _ if function_key(key) => name.to_owned(),
        _ => return None,
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

/// On Windows a key held everywhere would also type everywhere, so only a
/// combination or a function key is asked, as Serein does. The Linux desktop
/// takes any key, as the owner asked on 2026-10-26.
pub fn global_allowed(binding: &str) -> bool {
    // The Linux portal takes no mouse button; Windows' hook takes these three.
    if mouse(binding).is_some() {
        return cfg!(windows);
    }
    cfg!(target_os = "linux")
        || parse(binding)
            .is_some_and(|(key, modifiers)| modifiers != Modifiers::default() || function_key(key))
}

/// Hyprland assigns no key by itself: this is the line to add to its config
/// for `binding`, in Lua (`lua`) or in the classic form. `None` without a trigger.
pub fn hyprland_line(binding: &str, lua: bool) -> Option<String> {
    let trigger = match mouse(binding) {
        // The Linux input codes: BTN_MIDDLE, BTN_SIDE and BTN_EXTRA.
        Some(button) => format!(
            "mouse:{}",
            match button {
                OtherButton::Middle => 274,
                OtherButton::Back => 275,
                OtherButton::Forward => 276,
            }
        ),
        None => trigger(binding)?,
    };
    let (mods, key) = trigger.rsplit_once('+').unwrap_or(("", trigger.as_str()));
    Some(if lua {
        let combo = if mods.is_empty() {
            key.to_owned()
        } else {
            format!("{} + {key}", mods.replace('+', " + "))
        };
        format!(r#"hl.bind("{combo}", hl.dsp.global("rune:{ID}"))"#)
    } else {
        format!(
            "bind = {}, {key}, global, rune:{ID}",
            mods.replace('+', " ")
        )
    })
}

/// Whether the session runs under Hyprland, by its environment.
fn hyprland() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
        || std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| {
            desktop
                .split(':')
                .any(|part| part.eq_ignore_ascii_case("Hyprland"))
        })
}

/// Whether the person's Hyprland config is Lua (read only).
fn hyprland_lua() -> bool {
    std::env::var_os("HOME").is_some_and(|home| {
        std::path::Path::new(&home)
            .join(".config/hypr/hyprland.lua")
            .is_file()
    })
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
        ui.audio.hyprland_bind = key
            .filter(|_| hyprland())
            .and_then(|key| hyprland_line(key, hyprland_lua()));
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
                // Asked for but not bound: outside the window it does not work.
                let asked = self
                    .wanted
                    .as_ref()
                    .is_some_and(|(_, keys)| keys.iter().any(|(known, _)| known == ID));
                if asked && !self.bound.contains(ID) {
                    ui.audio.global_keys =
                        GlobalKeys::Unavailable(es("el sistema no puede dar esa tecla.").into());
                    return;
                }
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
        assert_eq!(trigger("Alt+Backtick").as_deref(), Some("ALT+grave"));
        assert_eq!(trigger("Colon"), None, "the desktop chooses");
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
    #[cfg(not(target_os = "linux"))]
    fn only_combinations_and_function_keys_go_global() {
        assert!(global_allowed("Ctrl+V"));
        assert!(global_allowed("F9"));
        assert!(!global_allowed("V"), "it would type everywhere");
        assert!(!global_allowed("Backtick"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn the_linux_desktop_takes_any_key() {
        assert!(global_allowed("V"));
        assert!(global_allowed("Backtick"));
    }

    #[test]
    fn the_desktop_gets_the_names_of_more_keys_and_hyprland_its_bind_line() {
        assert_eq!(trigger("Shift+ArrowLeft").as_deref(), Some("SHIFT+Left"));
        assert_eq!(trigger("Ctrl+PageUp").as_deref(), Some("CTRL+Page_Up"));
        assert_eq!(trigger("Ctrl+Slash").as_deref(), Some("CTRL+slash"));
        assert_eq!(trigger("Quote").as_deref(), Some("apostrophe"));
        assert_eq!(
            hyprland_line("Ctrl+Shift+T", false).as_deref(),
            Some("bind = CTRL SHIFT, t, global, rune:push-to-talk")
        );
        assert_eq!(
            hyprland_line("Ctrl+Shift+T", true).as_deref(),
            Some(r#"hl.bind("CTRL + SHIFT + t", hl.dsp.global("rune:push-to-talk"))"#)
        );
        assert_eq!(
            hyprland_line("F9", false).as_deref(),
            Some("bind = , F9, global, rune:push-to-talk")
        );
    }

    #[test]
    fn a_mouse_button_binds_as_the_windows_hook_names_it_and_as_hyprland_codes_it() {
        assert_eq!(mouse_binding(OtherButton::Back), "MOUSE4");
        assert_eq!(
            hyprland_line("MOUSE4", false).as_deref(),
            Some("bind = , mouse:275, global, rune:push-to-talk")
        );
        assert_eq!(
            hyprland_line("MOUSE3", true).as_deref(),
            Some(r#"hl.bind("mouse:274", hl.dsp.global("rune:push-to-talk"))"#)
        );
        assert_eq!(
            global_allowed("MOUSE5"),
            cfg!(windows),
            "the portal takes no mouse"
        );
        let tree = limn::UiTree::new(limn::Element::spacer());
        assert!(window_held(&tree, &[OtherButton::Back], "MOUSE4"));
        assert!(!window_held(&tree, &[OtherButton::Middle], "MOUSE4"));
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
        assert!(!window_held(&tree, &[], "Ctrl+T"), "Ctrl is not held");
        tree.handle_event(limn::UiEvent::ModifiersChanged(Modifiers {
            ctrl: true,
            ..Default::default()
        }));
        assert!(window_held(&tree, &[], "Ctrl+T"));
        assert!(
            window_held(&tree, &[], "T"),
            "extra modifiers do not stop it"
        );
        tree.handle_event(limn::UiEvent::KeyReleased { key: Key::T });
        assert!(!window_held(&tree, &[], "Ctrl+T"), "released");
        tree.handle_event(key(Key::Num1, true));
        assert!(window_held(&tree, &[], "Ctrl+1"), "a digit holds too");
        tree.set_window_focused(false);
        assert!(
            !window_held(&tree, &[], "Ctrl+1"),
            "a lost focus releases it"
        );
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

    #[test]
    fn a_push_to_talk_the_desktop_did_not_bind_is_not_shown_as_another_one() {
        let mut global = GlobalPushToTalk::default();
        let mut ui = ptt("Ctrl+T");
        global.sync(&mut ui, Some(UserId(1)), true, |_| None);
        global.apply(
            &mut ui,
            ShortcutEvent::Bound(vec![("mute".into(), "Ctrl+M".into())]),
        );
        assert!(matches!(ui.audio.global_keys, GlobalKeys::Unavailable(_)));
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
        // A plain key is asked of the desktop only on Linux.
        let plain: &[&str] = if cfg!(target_os = "linux") {
            &["mute", "deafen", "ptm"]
        } else {
            &["mute", "ptm"]
        };
        assert_eq!(ids, plain);
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
    #[cfg(not(target_os = "linux"))]
    fn a_plain_key_stays_in_the_window_and_says_why() {
        let mut global = GlobalPushToTalk::default();
        let mut ui = ptt("V");
        global.sync(&mut ui, Some(UserId(1)), true, |_| {
            panic!("a plain key is never global")
        });
        assert!(matches!(ui.audio.global_keys, GlobalKeys::Unavailable(_)));
    }
}
