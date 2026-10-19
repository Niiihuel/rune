//! Notifications on Windows (windows-base): the system's toasts (WinRT
//! `ToastNotifier`) under RUNE's own application ID, which the user's
//! registry names (`HKCU\Software\Classes\AppUserModelId\RUNE.Desktop`),
//! so an unpackaged RUNE may need no shortcut to be heard; the installer's
//! Start menu shortcut carries the same ID, for a Windows that wants one
//! (windows-instalador), and so does the process. A click on the toast,
//! or on one of its buttons, comes back while RUNE runs; a toast clicked in
//! the Action Center after RUNE closed does nothing. A ringing call is the
//! `incomingCall` scenario: it stays until it is withdrawn. The toast's
//! text is worked out without Windows, so it is tested everywhere.

use super::Notice;

/// What a click on the toast itself says.
const CLICK: &str = "default";

/// `text` safe inside an XML attribute or element.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            // XML 1.0 has no place for other controls.
            control if control.is_control() && !matches!(control, '\n' | '\t') => {}
            other => escaped.push(other),
        }
    }
    escaped
}

/// The toast's content for `notice`.
fn xml(notice: &Notice) -> String {
    let scenario = if notice.ringing {
        r#" scenario="incomingCall""#
    } else {
        ""
    };
    let actions: String = notice
        .buttons
        .iter()
        .map(|(action, label)| {
            format!(
                r#"<action content="{}" arguments="{}" activationType="foreground"/>"#,
                escape(label),
                escape(action)
            )
        })
        .collect();
    format!(
        r#"<toast launch="{CLICK}"{scenario}><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual>{}</toast>"#,
        escape(&notice.title),
        escape(&notice.body),
        if actions.is_empty() {
            String::new()
        } else {
            format!("<actions>{actions}</actions>")
        },
    )
}

/// What a toast's activation says, as one of the notice's own actions, or
/// `None` for a click on it; anything else is not this notice's.
fn clicked(notice: &Notice, arguments: &str) -> Option<Option<&'static str>> {
    if arguments == CLICK {
        return Some(None);
    }
    notice
        .buttons
        .iter()
        .find(|(action, _)| *action == arguments)
        .map(|(action, _)| Some(*action))
}

#[cfg(windows)]
pub(super) use toasts::{claim_application_id, spawn};

#[cfg(windows)]
mod toasts {
    use std::collections::HashMap;
    use std::sync::Arc;

    use windows::core::{IInspectable, Interface, HSTRING};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    use windows::UI::Notifications::{
        ToastActivatedEventArgs, ToastNotification, ToastNotificationManager, ToastNotifier,
    };

    use super::super::{Click, Request};
    use super::{clicked, xml};
    use crate::win::registry;

    /// RUNE's application ID, as the registry names it.
    const APPLICATION: &str = "RUNE.Desktop";
    const REGISTERED: &str = r"Software\Classes\AppUserModelId\RUNE.Desktop";

    pub(in crate::notifications) fn claim_application_id() {
        // SAFETY: a string Windows copies before returning.
        let _ = unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APPLICATION)) };
    }

    pub(in crate::notifications) fn spawn(
        mut requests: tokio::sync::mpsc::Receiver<Request>,
        report: impl Fn(Click) + Send + Sync + 'static,
    ) {
        let report: Arc<dyn Fn(Click) + Send + Sync> = Arc::new(report);
        // A thread that cannot start drops the receiver: showing then fails.
        let _ = std::thread::Builder::new()
            .name("rune-notifications".into())
            .spawn(move || {
                // SAFETY: WinRT on this thread, in the multithreaded
                // apartment, for the whole of its life.
                if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
                    return;
                }
                // The name the toasts are shown under; without it Windows
                // drops them. Written every start: RUNE may have moved.
                let _ = registry::set_text(REGISTERED, "DisplayName", "RUNE");
                let Ok(notifier) = ToastNotificationManager::CreateToastNotifierWithId(
                    &HSTRING::from(APPLICATION),
                ) else {
                    return;
                };
                let mut shown: HashMap<u64, ToastNotification> = HashMap::new();
                while let Some(request) = requests.blocking_recv() {
                    match request {
                        Request::Show(notice) => {
                            let key = notice.key;
                            match show(&notifier, notice, report.clone()) {
                                Some(toast) => {
                                    if let Some(old) = shown.insert(key, toast) {
                                        let _ = notifier.Hide(&old);
                                    }
                                }
                                None => eprintln!("rune: Windows did not show the notification"),
                            }
                        }
                        Request::Close(key) => {
                            if let Some(toast) = shown.remove(&key) {
                                let _ = notifier.Hide(&toast);
                            }
                        }
                        Request::Withdraw => {
                            for (_, toast) in shown.drain() {
                                let _ = notifier.Hide(&toast);
                            }
                        }
                    }
                }
            });
    }

    fn show(
        notifier: &ToastNotifier,
        notice: super::Notice,
        report: Arc<dyn Fn(Click) + Send + Sync>,
    ) -> Option<ToastNotification> {
        let content = XmlDocument::new().ok()?;
        content.LoadXml(&HSTRING::from(xml(&notice))).ok()?;
        let toast = ToastNotification::CreateToastNotification(&content).ok()?;
        let key = notice.key;
        toast
            .Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
                move |_, arguments| {
                    let arguments = arguments
                        .as_ref()
                        .and_then(|arguments| arguments.cast::<ToastActivatedEventArgs>().ok())
                        .and_then(|arguments| arguments.Arguments().ok())
                        .map(|arguments| arguments.to_string_lossy())
                        .unwrap_or_default();
                    if let Some(button) = clicked(&notice, &arguments) {
                        report((key, button));
                    }
                    Ok(())
                },
            ))
            .ok()?;
        notifier.Show(&toast).ok()?;
        Some(toast)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_toast_escapes_its_text_and_carries_its_buttons() {
        let notice = Notice::new(7, "Ada <ada@x>", "\"hola\" & chau\u{7}");
        let content = xml(&notice);
        assert!(
            content
                .contains("<text>Ada &lt;ada@x&gt;</text><text>&quot;hola&quot; &amp; chau</text>"),
            "{content}"
        );
        assert!(!content.contains("<actions>") && !content.contains("scenario"));
        let ringing = Notice::new(8, "Llamada", "Ada").ringing(&[("accept", "Aceptar")]);
        let content = xml(&ringing);
        assert!(content.contains(r#"scenario="incomingCall""#), "{content}");
        assert!(
            content.contains(
                r#"<action content="Aceptar" arguments="accept" activationType="foreground"/>"#
            ),
            "{content}"
        );
    }

    #[test]
    fn only_the_notice_s_own_actions_come_back() {
        let notice = Notice::new(8, "Llamada", "Ada").ringing(&[("accept", "Aceptar")]);
        assert_eq!(clicked(&notice, "default"), Some(None));
        assert_eq!(clicked(&notice, "accept"), Some(Some("accept")));
        assert_eq!(clicked(&notice, "decline"), None);
    }
}
