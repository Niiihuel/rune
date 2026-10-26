//! Global shortcuts on Windows (windows-base): low-level keyboard and mouse
//! hooks on a thread of their own, as Discord has them. The hooks never
//! consume input: every event goes on with `CallNextHookEx`, and is only
//! compared with the bound shortcuts. Nothing is stored, buffered, sent or
//! logged: the state is one flag per shortcut and the modifiers held, both
//! from the same events. Injected input counts too. Windows may not send the
//! input of an elevated program to these hooks, so RUNE runs elevated too
//! when that matters (see docs/SECURITY.md). The matching is pure and is
//! tested on every platform.

use super::Shortcut;

/// Windows' modifier flags (`MOD_ALT`, `MOD_CONTROL`, `MOD_SHIFT`).
const ALT: u32 = 1;
const CONTROL: u32 = 2;
const SHIFT: u32 = 4;

/// The mouse buttons a shortcut can hold: the wheel's and the two side ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Button {
    Middle,
    X1,
    X2,
}

/// What a shortcut waits for: a key by its virtual-key code, or a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Main {
    Key(u32),
    Button(Button),
}

/// One hook event, reduced to what matching needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Input {
    Key { vk: u32, down: bool },
    Button { button: Button, down: bool },
}

/// A shortcut as Windows names it, with how a person reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Hotkey {
    pub main: Main,
    pub modifiers: u32,
    pub shown: String,
}

/// The virtual-key code of a key's name in the freedesktop format. Keys whose
/// code depends on the keyboard layout (the punctuation) are the US layout's.
fn key_code(key: &str) -> Option<u32> {
    Some(match key.as_bytes() {
        [letter] if letter.is_ascii_alphabetic() => u32::from(letter.to_ascii_uppercase()),
        [digit] if digit.is_ascii_digit() => u32::from(*digit),
        [b'F', number @ ..] => {
            let number: u32 = std::str::from_utf8(number).ok()?.parse().ok()?;
            // VK_F1 is 0x70; Windows names keys up to F24.
            (1..=24).contains(&number).then_some(0x6f + number)?
        }
        _ => match key {
            "space" => 0x20,
            "grave" => 0xc0,
            "minus" => 0xbd,
            "equal" => 0xbb,
            "comma" => 0xbc,
            "period" => 0xbe,
            "slash" => 0xbf,
            "semicolon" => 0xba,
            "apostrophe" => 0xde,
            "bracketleft" => 0xdb,
            "bracketright" => 0xdd,
            "backslash" => 0xdc,
            "Left" => 0x25,
            "Up" => 0x26,
            "Right" => 0x27,
            "Down" => 0x28,
            "Home" => 0x24,
            "End" => 0x23,
            "Page_Up" => 0x21,
            "Page_Down" => 0x22,
            "Insert" => 0x2d,
            "Delete" => 0x2e,
            _ => return None,
        },
    })
}

/// A trigger in the freedesktop format (`CTRL+SHIFT+t`, `ALT+F5`,
/// `CTRL+grave`, `MOUSE4`) as a hotkey.
fn hotkey(trigger: &str) -> Option<Hotkey> {
    let mut parts: Vec<&str> = trigger.split('+').collect();
    let key = parts.pop()?;
    let (mut modifiers, mut shown) = (0, String::new());
    for part in parts {
        let (flag, name) = match part {
            "CTRL" => (CONTROL, "Ctrl"),
            "ALT" => (ALT, "Alt"),
            "SHIFT" => (SHIFT, "Shift"),
            _ => return None,
        };
        if modifiers & flag != 0 {
            return None;
        }
        modifiers |= flag;
        shown.push_str(name);
        shown.push('+');
    }
    let main = match key {
        "MOUSE3" => Main::Button(Button::Middle),
        "MOUSE4" => Main::Button(Button::X1),
        "MOUSE5" => Main::Button(Button::X2),
        _ => Main::Key(key_code(key)?),
    };
    shown.push_str(&match key {
        "space" => "Space".to_owned(),
        // A letter reads in capitals; a key's name stays as the trigger writes it.
        other if other.len() == 1 => other.to_ascii_uppercase(),
        other => other.to_owned(),
    });
    Some(Hotkey {
        main,
        modifiers,
        shown,
    })
}

/// The shortcuts that can be asked of Windows, each with its hotkey.
fn wanted(shortcuts: Vec<Shortcut>) -> Vec<(String, Hotkey)> {
    shortcuts
        .into_iter()
        .filter_map(|shortcut| Some((shortcut.id, hotkey(shortcut.preferred_trigger.as_deref()?)?)))
        .collect()
}

/// The side bit of a modifier's virtual key; the generic codes count as the
/// left side. Each side is kept, so releasing one of two held does not end it.
fn side(vk: u32) -> Option<u32> {
    Some(match vk {
        0x10 | 0xa0 => 1 << 0,
        0xa1 => 1 << 1,
        0x11 | 0xa2 => 1 << 2,
        0xa3 => 1 << 3,
        0x12 | 0xa4 => 1 << 4,
        0xa5 => 1 << 5,
        _ => return None,
    })
}

/// The modifier flags held, from the sides held.
fn held_flags(sides: u32) -> u32 {
    let mut flags = 0;
    for (flag, mask) in [(SHIFT, 0b11), (CONTROL, 0b1100), (ALT, 0b11_0000)] {
        if sides & mask != 0 {
            flags |= flag;
        }
    }
    flags
}

/// A shortcut that started or stopped, by its place in the list it was built from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Edge {
    Pressed(usize),
    Released(usize),
}

/// Starts a shortcut when its main input goes down with exactly its modifiers
/// held, and stops it when that main input or one of its modifiers goes up.
pub(super) struct Matcher {
    hotkeys: Vec<Hotkey>,
    active: Vec<bool>,
    sides: u32,
}

impl Matcher {
    pub(super) fn new(hotkeys: Vec<Hotkey>) -> Self {
        Self {
            active: vec![false; hotkeys.len()],
            hotkeys,
            sides: 0,
        }
    }

    /// The edges one input makes. A key that repeats while held makes none.
    pub(super) fn step(&mut self, input: Input) -> Vec<Edge> {
        let mut edges = Vec::new();
        let (main, down) = match input {
            Input::Key { vk, down } => match side(vk) {
                Some(bit) => {
                    if down {
                        self.sides |= bit;
                    } else {
                        self.sides &= !bit;
                    }
                    let flags = held_flags(self.sides);
                    for (place, hotkey) in self.hotkeys.iter().enumerate() {
                        if self.active[place] && hotkey.modifiers & !flags != 0 {
                            self.active[place] = false;
                            edges.push(Edge::Released(place));
                        }
                    }
                    return edges;
                }
                None => (Main::Key(vk), down),
            },
            Input::Button { button, down } => (Main::Button(button), down),
        };
        let flags = held_flags(self.sides);
        for (place, hotkey) in self.hotkeys.iter().enumerate() {
            if hotkey.main != main {
                continue;
            }
            if down && !self.active[place] && hotkey.modifiers == flags {
                self.active[place] = true;
                edges.push(Edge::Pressed(place));
            } else if !down && self.active[place] {
                self.active[place] = false;
                edges.push(Edge::Released(place));
            }
        }
        edges
    }
}

#[cfg(windows)]
pub(super) use hooks::spawn;

#[cfg(windows)]
mod hooks {
    use std::cell::RefCell;
    use std::time::Duration;

    use tokio::sync::oneshot::{self, error::TryRecvError};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, MsgWaitForMultipleObjects, PeekMessageW, SetWindowsHookExW,
        UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, PM_REMOVE, QS_ALLINPUT,
        WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
        WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1, XBUTTON2,
    };

    use super::super::{Shortcut, ShortcutError, ShortcutEvent};
    use super::{wanted, Button, Edge, Hotkey, Input, Matcher};

    /// How often the thread looks whether its handle is gone. The hooks run
    /// as messages arrive, so this only bounds how late a stop is noticed.
    const IDLE: Duration = Duration::from_millis(250);

    /// The hooks' state, on the thread that installed them: the hook
    /// procedures run there, and nothing else reads it.
    struct State {
        matcher: Matcher,
        ids: Vec<String>,
        report: Box<dyn Fn(ShortcutEvent)>,
    }

    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    fn say(event: ShortcutEvent) {
        STATE.with(|state| {
            if let Some(state) = state.borrow().as_ref() {
                (state.report)(event);
            }
        });
    }

    fn feed(input: Input) {
        STATE.with(|state| {
            if let Some(state) = state.borrow_mut().as_mut() {
                for edge in state.matcher.step(input) {
                    let event = match edge {
                        Edge::Pressed(place) => ShortcutEvent::Pressed(state.ids[place].clone()),
                        Edge::Released(place) => ShortcutEvent::Released(state.ids[place].clone()),
                    };
                    (state.report)(event);
                }
            }
        });
    }

    /// `WH_KEYBOARD_LL`: the key and its direction; the event goes on.
    unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let message = wparam.0 as u32;
            if matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP) {
                // SAFETY: for a code of at least zero, lParam is this event's
                // KBDLLHOOKSTRUCT, valid for the call.
                let vk = unsafe { (*(lparam.0 as *const KBDLLHOOKSTRUCT)).vkCode };
                let down = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
                feed(Input::Key { vk, down });
            }
        }
        // SAFETY: passes the event on unchanged; the hook handle is not needed.
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// `WH_MOUSE_LL`: the middle and side buttons only; the event goes on.
    unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let message = wparam.0 as u32;
            let button = match message {
                WM_MBUTTONDOWN | WM_MBUTTONUP => Some(Button::Middle),
                WM_XBUTTONDOWN | WM_XBUTTONUP => {
                    // SAFETY: for a code of at least zero, lParam is this
                    // event's MSLLHOOKSTRUCT, valid for the call.
                    let data = unsafe { (*(lparam.0 as *const MSLLHOOKSTRUCT)).mouseData };
                    match (data >> 16) as u16 {
                        XBUTTON1 => Some(Button::X1),
                        XBUTTON2 => Some(Button::X2),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(button) = button {
                let down = matches!(message, WM_MBUTTONDOWN | WM_XBUTTONDOWN);
                feed(Input::Button { button, down });
            }
        }
        // SAFETY: passes the event on unchanged; the hook handle is not needed.
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// Both hooks on this thread; if the second fails, the first is removed.
    fn install() -> windows::core::Result<(HHOOK, HHOOK)> {
        // SAFETY: the module of this executable, which the hooks keep.
        let module = unsafe { GetModuleHandleW(PCWSTR::null()) }?;
        // SAFETY: the procedures above take the events the system passes them.
        let keyboard = unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), Some(module.into()), 0)
        }?;
        // SAFETY: as above.
        match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(module.into()), 0) } {
            Ok(mouse) => Ok((keyboard, mouse)),
            Err(error) => {
                uninstall(keyboard);
                Err(error)
            }
        }
    }

    fn uninstall(hook: HHOOK) {
        // SAFETY: a hook this thread installed and has not removed.
        let _ = unsafe { UnhookWindowsHookEx(hook) };
    }

    /// Pumps this thread's queue, where the hooks are called, until the
    /// handle is dropped.
    fn pump(stopped: &mut oneshot::Receiver<()>) {
        let mut message = MSG::default();
        while matches!(stopped.try_recv(), Err(TryRecvError::Empty)) {
            // SAFETY: waits on this thread's own queue and drains it.
            unsafe {
                MsgWaitForMultipleObjects(None, false, IDLE.as_millis() as u32, QS_ALLINPUT);
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {}
            }
        }
    }

    pub(in crate::shortcuts) fn spawn(
        shortcuts: Vec<Shortcut>,
        report: impl Fn(ShortcutEvent) + Send + 'static,
        mut stopped: oneshot::Receiver<()>,
    ) {
        let started = std::thread::Builder::new()
            .name("rune-shortcuts".into())
            .spawn(move || {
                let hotkeys = wanted(shortcuts);
                if hotkeys.is_empty() {
                    report(ShortcutEvent::Ended(ShortcutError::WINDOWS_KEY));
                    return;
                }
                let bound = hotkeys
                    .iter()
                    .map(|(id, hotkey)| (id.clone(), hotkey.shown.clone()))
                    .collect();
                let (ids, keys): (Vec<String>, Vec<Hotkey>) = hotkeys.into_iter().unzip();
                STATE.with(|state| {
                    *state.borrow_mut() = Some(State {
                        matcher: Matcher::new(keys),
                        ids,
                        report: Box::new(report),
                    });
                });
                match install() {
                    Ok((keyboard, mouse)) => {
                        say(ShortcutEvent::Bound(bound));
                        pump(&mut stopped);
                        uninstall(keyboard);
                        uninstall(mouse);
                    }
                    Err(error) => {
                        // The code tells the support, never the keys.
                        eprintln!(
                            "rune: global shortcuts: the Windows hooks did not install ({error})"
                        );
                        say(ShortcutEvent::Ended(ShortcutError::WINDOWS_HOOK));
                    }
                }
                STATE.with(|state| *state.borrow_mut() = None);
            });
        if started.is_err() {
            // The report went with the thread that never started.
            eprintln!("rune: could not start the global shortcuts");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(vk: u32, down: bool) -> Input {
        Input::Key { vk, down }
    }

    const CTRL: u32 = 0xa2;
    const SHIFT_KEY: u32 = 0xa0;
    const T: u32 = b'T' as u32;
    const F9: u32 = 0x78;

    #[test]
    fn freedesktop_triggers_become_windows_hotkeys() {
        let plain = |main, modifiers, shown: &str| {
            Some(Hotkey {
                main,
                modifiers,
                shown: shown.into(),
            })
        };
        assert_eq!(
            hotkey("CTRL+SHIFT+t"),
            plain(Main::Key(T), CONTROL | SHIFT, "Ctrl+Shift+T")
        );
        assert_eq!(hotkey("ALT+F5"), plain(Main::Key(0x74), ALT, "Alt+F5"));
        assert_eq!(hotkey("F24"), plain(Main::Key(0x87), 0, "F24"));
        assert_eq!(
            hotkey("CTRL+space"),
            plain(Main::Key(0x20), CONTROL, "Ctrl+Space")
        );
        assert_eq!(hotkey("CTRL+7"), plain(Main::Key(0x37), CONTROL, "Ctrl+7"));
        // The names the freedesktop format gives keys a plain key cannot be
        // bound by, and the other keys the recorder can record.
        assert_eq!(
            hotkey("CTRL+grave"),
            plain(Main::Key(0xc0), CONTROL, "Ctrl+grave")
        );
        assert_eq!(hotkey("Page_Up"), plain(Main::Key(0x21), 0, "Page_Up"));
        assert_eq!(hotkey("ALT+Left"), plain(Main::Key(0x25), ALT, "Alt+Left"));
        assert_eq!(
            hotkey("MOUSE4"),
            plain(Main::Button(Button::X1), 0, "MOUSE4")
        );
        assert_eq!(
            hotkey("CTRL+MOUSE3"),
            plain(Main::Button(Button::Middle), CONTROL, "Ctrl+MOUSE3")
        );
        for refused in [
            "",
            "F25",
            "F0",
            "SUPER+a",
            "CTRL+CTRL+a",
            "CTRL+",
            "CTRL+ñ",
            "Tab",
            "MOUSE6",
            "KP_1",
            "CapsLock",
        ] {
            assert_eq!(hotkey(refused), None, "{refused}");
        }
    }

    #[test]
    fn only_shortcuts_with_a_trigger_windows_names_are_asked_for() {
        let shortcut = |id: &str, trigger: Option<&str>| Shortcut {
            id: id.into(),
            description: String::new(),
            preferred_trigger: trigger.map(Into::into),
        };
        let asked = wanted(vec![
            shortcut("ptt", Some("CTRL+t")),
            shortcut("mute", None),
            shortcut("deafen", Some("META+d")),
        ]);
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].0, "ptt");
    }

    fn matcher(triggers: &[&str]) -> Matcher {
        Matcher::new(triggers.iter().filter_map(|t| hotkey(t)).collect())
    }

    #[test]
    fn a_press_starts_and_its_release_stops_and_a_repeat_is_nothing() {
        let mut matcher = matcher(&["CTRL+t"]);
        assert!(matcher.step(key(CTRL, true)).is_empty());
        assert_eq!(matcher.step(key(T, true)), [Edge::Pressed(0)]);
        assert!(matcher.step(key(T, true)).is_empty(), "auto-repeat");
        assert_eq!(matcher.step(key(T, false)), [Edge::Released(0)]);
        assert!(matcher.step(key(T, false)).is_empty());
    }

    #[test]
    fn extra_modifiers_do_not_start_it_and_the_modifiers_must_be_held_first() {
        let mut matcher = matcher(&["CTRL+t"]);
        matcher.step(key(CTRL, true));
        matcher.step(key(SHIFT_KEY, true));
        assert!(matcher.step(key(T, true)).is_empty(), "Shift is extra");
        matcher.step(key(SHIFT_KEY, false));
        assert!(matcher.step(key(T, false)).is_empty());
        assert_eq!(matcher.step(key(T, true)), [Edge::Pressed(0)]);
        let mut late = self::matcher(&["CTRL+t"]);
        assert!(late.step(key(T, true)).is_empty(), "Ctrl pressed after T");
    }

    #[test]
    fn releasing_a_required_modifier_ends_it_and_either_side_counts() {
        let mut matcher = matcher(&["CTRL+t"]);
        // Right Ctrl counts as well as the left one.
        matcher.step(key(0xa3, true));
        assert_eq!(matcher.step(key(T, true)), [Edge::Pressed(0)]);
        matcher.step(key(CTRL, true));
        assert!(
            matcher.step(key(CTRL, false)).is_empty(),
            "the right Ctrl still holds it"
        );
        assert_eq!(matcher.step(key(0xa3, false)), [Edge::Released(0)]);
        assert!(matcher.step(key(T, false)).is_empty());
    }

    #[test]
    fn the_generic_modifier_code_counts_as_the_left_side() {
        let mut matcher = matcher(&["CTRL+t"]);
        matcher.step(key(0x11, true));
        assert_eq!(matcher.step(key(T, true)), [Edge::Pressed(0)]);
        assert_eq!(matcher.step(key(0x11, false)), [Edge::Released(0)]);
    }

    #[test]
    fn a_plain_key_is_not_the_same_with_a_modifier_held() {
        let mut matcher = matcher(&["F9"]);
        matcher.step(key(CTRL, true));
        assert!(matcher.step(key(F9, true)).is_empty(), "Ctrl is extra");
        matcher.step(key(CTRL, false));
        assert_eq!(matcher.step(key(F9, true)), [Edge::Pressed(0)]);
    }

    #[test]
    fn mouse_buttons_start_and_stop_like_keys() {
        let mut matcher = matcher(&["MOUSE4", "MOUSE5", "MOUSE3"]);
        let button = |button, down| Input::Button { button, down };
        assert_eq!(matcher.step(button(Button::X1, true)), [Edge::Pressed(0)]);
        assert_eq!(matcher.step(button(Button::X2, true)), [Edge::Pressed(1)]);
        assert_eq!(
            matcher.step(button(Button::Middle, true)),
            [Edge::Pressed(2)]
        );
        assert_eq!(matcher.step(button(Button::X1, false)), [Edge::Released(0)]);
        assert_eq!(
            matcher.step(button(Button::X1, true)),
            [Edge::Pressed(0)],
            "pressed again after its release"
        );
    }

    #[test]
    fn two_shortcuts_are_tracked_apart() {
        let mut matcher = matcher(&["CTRL+t", "F9"]);
        matcher.step(key(CTRL, true));
        assert_eq!(matcher.step(key(T, true)), [Edge::Pressed(0)]);
        assert_eq!(matcher.step(key(T, false)), [Edge::Released(0)]);
        // F9 with Ctrl held is not F9 alone, so it waits for Ctrl to go up.
        assert!(matcher.step(key(F9, true)).is_empty());
        assert!(matcher.step(key(F9, false)).is_empty());
        matcher.step(key(CTRL, false));
        assert_eq!(matcher.step(key(F9, true)), [Edge::Pressed(1)]);
        assert_eq!(matcher.step(key(F9, false)), [Edge::Released(1)]);
    }
}
