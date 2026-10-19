//! Global shortcuts on Windows (windows-base): each one the system's own
//! hotkey (`RegisterHotKey`), on a thread of its own, without hooks on the
//! keyboard. Windows says when a hotkey is pressed, never when it is let
//! go: while one is held the thread asks the key's state every 15 ms
//! (`GetAsyncKeyState`), and says it was released when it is up. A hotkey
//! another program holds is not bound; the rest are. While RUNE holds a
//! hotkey, other programs do not get that combination.

use super::Shortcut;

/// Windows' modifier flags (`MOD_ALT`, `MOD_CONTROL`, `MOD_SHIFT`).
const ALT: u32 = 1;
const CONTROL: u32 = 2;
const SHIFT: u32 = 4;

/// A trigger in the freedesktop format (`CTRL+SHIFT+t`, `ALT+F5`, `space`)
/// as Windows' modifiers and virtual key, with how a person reads it.
fn hotkey(trigger: &str) -> Option<(u32, u32, String)> {
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
    let virtual_key = match key.as_bytes() {
        [letter] if letter.is_ascii_alphabetic() => u32::from(letter.to_ascii_uppercase()),
        [digit] if digit.is_ascii_digit() => u32::from(*digit),
        _ if key == "space" => 0x20,
        [b'F', number @ ..] => {
            let number: u32 = std::str::from_utf8(number).ok()?.parse().ok()?;
            // VK_F1 is 0x70; Windows names keys up to F24.
            (1..=24).contains(&number).then_some(0x6f + number)?
        }
        _ => return None,
    };
    shown.push_str(&match key {
        "space" => "Space".to_owned(),
        other => other.to_ascii_uppercase(),
    });
    Some((modifiers, virtual_key, shown))
}

/// The shortcuts that can be asked of Windows, each with its hotkey.
fn wanted(shortcuts: Vec<Shortcut>) -> Vec<(String, (u32, u32, String))> {
    shortcuts
        .into_iter()
        .filter_map(|shortcut| Some((shortcut.id, hotkey(shortcut.preferred_trigger.as_deref()?)?)))
        .collect()
}

#[cfg(windows)]
pub(super) use hotkeys::spawn;

#[cfg(windows)]
mod hotkeys {
    use std::time::Duration;

    use tokio::sync::oneshot::{self, error::TryRecvError};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        MsgWaitForMultipleObjects, PeekMessageW, MSG, PM_NOREMOVE, PM_REMOVE, QS_ALLINPUT,
        WM_HOTKEY,
    };

    use super::super::{Shortcut, ShortcutError, ShortcutEvent};
    use super::wanted;

    /// How often a held key is looked at, and how often the thread looks
    /// whether its handle is gone.
    const HELD: Duration = Duration::from_millis(15);
    const IDLE: Duration = Duration::from_millis(250);

    pub(in crate::shortcuts) fn spawn(
        shortcuts: Vec<Shortcut>,
        report: impl Fn(ShortcutEvent) + Send + 'static,
        mut stopped: oneshot::Receiver<()>,
    ) {
        let started = std::thread::Builder::new()
            .name("rune-shortcuts".into())
            .spawn(move || {
                let mut message = MSG::default();
                // SAFETY: this thread's own queue, made before any hotkey
                // posts to it, and its hotkeys, released before it ends.
                unsafe {
                    let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                }
                let mut bound = Vec::new();
                // Each hotkey's number is its place in `bound`.
                for (id, (modifiers, key, shown)) in wanted(shortcuts) {
                    let number = bound.len() as i32 + 1;
                    let flags = HOT_KEY_MODIFIERS(modifiers) | MOD_NOREPEAT;
                    if unsafe { RegisterHotKey(None, number, flags, key) }.is_ok() {
                        bound.push((id, key, shown));
                    }
                }
                if bound.is_empty() {
                    report(ShortcutEvent::Ended(ShortcutError::DENIED));
                    return;
                }
                report(ShortcutEvent::Bound(
                    bound
                        .iter()
                        .map(|(id, _, shown)| (id.clone(), shown.clone()))
                        .collect(),
                ));
                let mut held: Vec<usize> = Vec::new();
                while matches!(stopped.try_recv(), Err(TryRecvError::Empty)) {
                    let wait = if held.is_empty() { IDLE } else { HELD };
                    // SAFETY: waits on this thread's queue only.
                    unsafe {
                        MsgWaitForMultipleObjects(
                            None,
                            false,
                            wait.as_millis() as u32,
                            QS_ALLINPUT,
                        );
                        while PeekMessageW(&mut message, None, WM_HOTKEY, WM_HOTKEY, PM_REMOVE)
                            .as_bool()
                        {
                            let index = message.wParam.0.wrapping_sub(1);
                            if bound.get(index).is_some() && !held.contains(&index) {
                                held.push(index);
                                report(ShortcutEvent::Pressed(bound[index].0.clone()));
                            }
                        }
                    }
                    held.retain(|&index| {
                        // The high bit is set while the key is down.
                        let down = unsafe { GetAsyncKeyState(bound[index].1 as i32) } < 0;
                        if !down {
                            report(ShortcutEvent::Released(bound[index].0.clone()));
                        }
                        down
                    });
                }
                for number in 1..=bound.len() as i32 {
                    // SAFETY: the hotkeys this thread registered.
                    let _ = unsafe { UnregisterHotKey(None, number) };
                }
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

    #[test]
    fn freedesktop_triggers_become_windows_hotkeys() {
        assert_eq!(
            hotkey("CTRL+SHIFT+t"),
            Some((CONTROL | SHIFT, u32::from(b'T'), "Ctrl+Shift+T".into()))
        );
        assert_eq!(hotkey("ALT+F5"), Some((ALT, 0x74, "Alt+F5".into())));
        assert_eq!(hotkey("F24"), Some((0, 0x87, "F24".into())));
        assert_eq!(
            hotkey("CTRL+space"),
            Some((CONTROL, 0x20, "Ctrl+Space".into()))
        );
        assert_eq!(
            hotkey("CTRL+7"),
            Some((CONTROL, u32::from(b'7'), "Ctrl+7".into()))
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
}
