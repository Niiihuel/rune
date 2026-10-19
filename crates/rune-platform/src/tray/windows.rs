//! The tray icon on Windows (windows-base): an icon of the notification
//! area (`Shell_NotifyIconW`) owned by a hidden window on a thread of its
//! own, with the same menu as on Linux. A click opens RUNE; the right
//! button (or the menu key) shows the menu. When Explorer restarts it sends
//! `TaskbarCreated` and the icon is added again. The menu and its labels
//! are worked out without Windows, so they are tested everywhere.

use rune_i18n::t;

use super::{mark, TrayEvent, TrayState, TrayStatus};

const OPEN: usize = 1;
const MUTE: usize = 3;
const DEAFEN: usize = 4;
const QUIT: usize = 7;
const STATUS_FIRST: usize = 10;

/// One line of the menu.
#[derive(Debug, PartialEq)]
enum Item {
    Entry {
        id: usize,
        label: String,
        checked: bool,
        enabled: bool,
    },
    /// A submenu: the status, only with an account.
    Submenu {
        label: String,
        items: Vec<Item>,
        enabled: bool,
    },
    Separator,
}

fn entry(id: usize, label: &str, checked: bool) -> Item {
    Item::Entry {
        id,
        label: label.to_owned(),
        checked,
        enabled: true,
    }
}

/// The menu for `state`, in its language.
fn menu(state: &TrayState) -> Vec<Item> {
    let _language = rune_i18n::scoped(state.language);
    vec![
        entry(OPEN, t("Abrir RUNE"), false),
        Item::Separator,
        entry(MUTE, t("Silenciar micrófono"), state.muted),
        entry(DEAFEN, t("Ensordecer"), state.deafened),
        Item::Submenu {
            label: t("Estado").to_owned(),
            items: TrayStatus::ALL
                .into_iter()
                .zip(STATUS_FIRST..)
                .map(|(choice, id)| entry(id, choice.label(), state.status == Some(choice)))
                .collect(),
            enabled: state.status.is_some(),
        },
        Item::Separator,
        entry(QUIT, t("Salir de RUNE"), false),
    ]
}

/// What the menu's command `id` asks for.
fn command(id: usize) -> Option<TrayEvent> {
    Some(match id {
        OPEN => TrayEvent::Open,
        MUTE => TrayEvent::ToggleMute,
        DEAFEN => TrayEvent::ToggleDeafen,
        QUIT => TrayEvent::Quit,
        status => TrayEvent::Status(*TrayStatus::ALL.get(status.checked_sub(STATUS_FIRST)?)?),
    })
}

/// The tooltip: the name and the summary, cut to what the shell shows
/// (127 characters and the NUL).
fn tooltip(state: &TrayState) -> [u16; 128] {
    let mut tip = [0u16; 128];
    let text = format!("RUNE\n{}", state.summary());
    for (slot, unit) in tip.iter_mut().zip(text.encode_utf16().take(127)) {
        *slot = unit;
    }
    tip
}

/// The mark at `size` pixels, as a 32-bit Windows bitmap reads it:
/// blue, green, red and alpha, from the top row down.
fn bgra(size: usize, dot: bool) -> Vec<u8> {
    let (_, _, argb) = mark::pixmap(size, dot);
    argb.chunks_exact(4)
        .flat_map(|pixel| [pixel[3], pixel[2], pixel[1], pixel[0]])
        .collect()
}

#[cfg(windows)]
pub(super) use shell::spawn;

#[cfg(windows)]
mod shell {
    use std::cell::RefCell;
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::watch;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE,
        NIM_MODIFY, NIM_SETVERSION, NIN_SELECT, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
        DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW,
        GetSystemMetrics, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW,
        SetForegroundWindow, SetTimer, TrackPopupMenu, TranslateMessage, HICON, HMENU, ICONINFO,
        MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MSG, SM_CXSMICON,
        TPM_BOTTOMALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CONTEXTMENU,
        WM_DESTROY, WM_NULL, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
    };

    use super::{bgra, command, menu, tooltip, Item, TrayEvent, TrayState};
    use crate::win::wide;

    /// What the icon's window says when the icon is used.
    const CALLBACK: u32 = WM_APP + 1;
    /// The menu key or Enter on the icon.
    const KEY_SELECT: u32 = NIN_SELECT | 1;
    const ICON: u32 = 1;
    const TIMER: usize = 1;
    /// How often the thread looks for a new state, or its handle gone.
    const STEP: Duration = Duration::from_millis(200);

    type Report = Arc<dyn Fn(TrayEvent) + Send + Sync>;

    /// What the window procedure works with: this thread's only.
    struct Context {
        window: HWND,
        state: watch::Receiver<TrayState>,
        report: Report,
        icon: Option<HICON>,
        /// The icon carries the dot now.
        dot: bool,
        shown: bool,
        /// `TaskbarCreated`: Explorer (re)started.
        taskbar_created: u32,
    }

    thread_local! {
        static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) };
    }

    pub(in crate::tray) fn spawn(
        state: watch::Receiver<TrayState>,
        report: impl Fn(TrayEvent) + Send + Sync + 'static,
    ) {
        let report: Report = Arc::new(report);
        let failed = report.clone();
        let started = std::thread::Builder::new()
            .name("rune-tray".into())
            .spawn(move || {
                // SAFETY: the window, its class and its icon belong to this
                // thread, which pumps its messages until the handle goes.
                if unsafe { run(state, report.clone()) }.is_none() {
                    report(TrayEvent::Unavailable);
                }
            });
        if started.is_err() {
            failed(TrayEvent::Unavailable);
        }
    }

    unsafe fn run(state: watch::Receiver<TrayState>, report: Report) -> Option<()> {
        unsafe {
            let instance = GetModuleHandleW(PCWSTR::null()).ok()?;
            let class = w!("RUNE.Tray");
            let registered = RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            });
            // Zero also when a tray before this one registered it: the
            // window below fails if the class is really missing.
            let _ = registered;
            // Never shown: it only owns the icon and hears the shell. Not a
            // message-only window, which would miss `TaskbarCreated`.
            let window = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class,
                w!("RUNE"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .ok()?;
            CONTEXT.with_borrow_mut(|context| {
                *context = Some(Context {
                    window,
                    state,
                    report,
                    icon: None,
                    dot: false,
                    shown: false,
                    taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
                })
            });
            with(|context| context.add());
            SetTimer(Some(window), TIMER, STEP.as_millis() as u32, None);
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            CONTEXT.with_borrow_mut(|context| context.take());
            Some(())
        }
    }

    fn with<T>(act: impl FnOnce(&mut Context) -> T) -> Option<T> {
        // A message that arrives while the context is borrowed (the menu's
        // own loop) is left to the default procedure.
        CONTEXT.with(|context| context.try_borrow_mut().ok()?.as_mut().map(act))
    }

    unsafe extern "system" fn procedure(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let handled = match message {
            WM_TIMER => match with(Context::step) {
                // The handle is gone: WM_DESTROY removes the icon and ends
                // the loop, once the context is free again.
                Some(true) => {
                    // SAFETY: this thread's own window.
                    let _ = unsafe { DestroyWindow(window) };
                    Some(())
                }
                other => other.map(drop),
            },
            CALLBACK => match (lparam.0 as u32) & 0xffff {
                // Version 4 says a click or Enter so; the raw button
                // messages that also come would open it twice.
                NIN_SELECT | KEY_SELECT => with(|context| (context.report)(TrayEvent::Open)),
                WM_CONTEXTMENU => {
                    // The anchor comes in `wparam`, as version 4 sends it.
                    let x = (wparam.0 & 0xffff) as u16 as i16 as i32;
                    let y = ((wparam.0 >> 16) & 0xffff) as u16 as i16 as i32;
                    with(|context| (context.state.borrow().clone(), context.report.clone())).map(
                        |(state, report)| {
                            // SAFETY: this thread's window, which outlives the menu.
                            if let Some(event) = unsafe { show_menu(window, &state, x, y) } {
                                report(event);
                            }
                        },
                    )
                }
                _ => None,
            },
            WM_DESTROY => with(|context| context.remove()),
            other if with(|context| other == context.taskbar_created) == Some(true) => {
                with(|context| {
                    context.shown = false;
                    context.add();
                })
            }
            _ => None,
        };
        if handled.is_some() && message != WM_DESTROY {
            return LRESULT(0);
        }
        // SAFETY: the default for everything else, with what was received.
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }

    impl Context {
        fn data(&self) -> NOTIFYICONDATAW {
            NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.window,
                uID: ICON,
                ..Default::default()
            }
        }

        /// Put the icon in the notification area; say whether it shows.
        fn add(&mut self) {
            let state = self.state.borrow_and_update().clone();
            self.dot = state.mentions > 0;
            let mut data = self.data();
            data.uFlags = NIF_ICON | NIF_TIP | NIF_MESSAGE | NIF_SHOWTIP;
            data.uCallbackMessage = CALLBACK;
            data.hIcon = self.icon(self.dot).unwrap_or_default();
            data.szTip = tooltip(&state);
            // SAFETY: a filled structure of this window's icon.
            let added = unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool();
            if added {
                data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
                // SAFETY: as above.
                let _ = unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) };
            }
            if added != self.shown || !added {
                self.shown = added;
                (self.report)(if added {
                    TrayEvent::Shown
                } else {
                    TrayEvent::Unavailable
                });
            }
        }

        /// Show a new state; `true` once the handle is gone.
        fn step(&mut self) -> bool {
            match self.state.has_changed() {
                Err(_) => return true,
                Ok(false) => {}
                Ok(true) if !self.shown => {
                    self.state.mark_unchanged();
                }
                Ok(true) => {
                    let state = self.state.borrow_and_update().clone();
                    let mut data = self.data();
                    data.uFlags = NIF_TIP | NIF_SHOWTIP;
                    data.szTip = tooltip(&state);
                    let dot = state.mentions > 0;
                    if dot != self.dot {
                        self.dot = dot;
                        if let Some(icon) = self.icon(dot) {
                            data.uFlags |= NIF_ICON;
                            data.hIcon = icon;
                        }
                    }
                    // SAFETY: a filled structure of this window's icon.
                    let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
                }
            }
            false
        }

        fn remove(&mut self) {
            let data = self.data();
            // SAFETY: this window's icon, and the picture made for it.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
                if let Some(icon) = self.icon.take() {
                    let _ = DestroyIcon(icon);
                }
                PostQuitMessage(0);
            }
        }

        /// The mark at the system's small icon size, replacing the last one.
        fn icon(&mut self, dot: bool) -> Option<HICON> {
            // SAFETY: plain calls; the bitmaps are copied into the icon and
            // deleted, and the old icon is destroyed once replaced.
            unsafe {
                let size = GetSystemMetrics(SM_CXSMICON).clamp(16, 256);
                let pixels = bgra(size as usize, dot);
                let header = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: size,
                        // Negative: the rows go from the top down.
                        biHeight: -size,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut bits = std::ptr::null_mut();
                let color =
                    CreateDIBSection(None, &header, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
                std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
                // The alpha says what shows; the mask lets all of it.
                let mask_bits = vec![0u8; (size as usize).div_ceil(16) * 2 * size as usize];
                let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr().cast()));
                let icon = CreateIconIndirect(&ICONINFO {
                    fIcon: true.into(),
                    hbmMask: mask,
                    hbmColor: color,
                    ..Default::default()
                });
                let _ = DeleteObject(color.into());
                let _ = DeleteObject(mask.into());
                let icon = icon.ok()?;
                if let Some(old) = self.icon.replace(icon) {
                    let _ = DestroyIcon(old);
                }
                Some(icon)
            }
        }
    }

    /// Show the menu at `x`, `y` and wait for a choice.
    ///
    /// # Safety
    /// `window` is this thread's own.
    unsafe fn show_menu(window: HWND, state: &TrayState, x: i32, y: i32) -> Option<TrayEvent> {
        unsafe {
            let menu_handle = CreatePopupMenu().ok()?;
            let built = append(menu_handle, &menu(state));
            let (mut x, mut y) = (x, y);
            if (x, y) == (0, 0) {
                let mut cursor = POINT::default();
                if GetCursorPos(&mut cursor).is_ok() {
                    (x, y) = (cursor.x, cursor.y);
                }
            }
            // As the shell asks: the menu closes when the person clicks
            // elsewhere only if its window is in front, and the next
            // message lets it go.
            let _ = SetForegroundWindow(window);
            let chosen = TrackPopupMenu(
                menu_handle,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY | TPM_BOTTOMALIGN,
                x,
                y,
                None,
                window,
                None,
            );
            let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(menu_handle);
            built?;
            command(chosen.0 as usize)
        }
    }

    /// Fill `menu` with `items`; a submenu is destroyed with its parent.
    ///
    /// # Safety
    /// `menu` is a menu this thread made.
    unsafe fn append(menu: HMENU, items: &[Item]) -> Option<()> {
        for item in items {
            unsafe {
                match item {
                    Item::Separator => AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()),
                    Item::Entry {
                        id,
                        label,
                        checked,
                        enabled,
                    } => {
                        let mut flags = MF_STRING;
                        if *checked {
                            flags |= MF_CHECKED;
                        }
                        if !*enabled {
                            flags |= MF_GRAYED;
                        }
                        AppendMenuW(menu, flags, *id, PCWSTR(wide(label).as_ptr()))
                    }
                    Item::Submenu {
                        label,
                        items,
                        enabled,
                    } => {
                        let submenu = CreatePopupMenu().ok()?;
                        append(submenu, items)?;
                        let mut flags = MF_STRING | MF_POPUP;
                        if !*enabled {
                            flags |= MF_GRAYED;
                        }
                        AppendMenuW(
                            menu,
                            flags,
                            submenu.0 as usize,
                            PCWSTR(wide(label).as_ptr()),
                        )
                    }
                }
                .ok()?;
            }
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_has_the_linux_menu_s_lines_and_its_status_only_with_an_account() {
        let state = TrayState {
            muted: true,
            status: Some(TrayStatus::DoNotDisturb),
            ..Default::default()
        };
        let items = menu(&state);
        assert_eq!(items.len(), 7);
        assert_eq!(items[2], entry(MUTE, "Silenciar micrófono", true));
        assert_eq!(items[3], entry(DEAFEN, "Ensordecer", false));
        let Item::Submenu {
            items: statuses,
            enabled,
            ..
        } = &items[4]
        else {
            panic!("the status is a submenu")
        };
        assert!(*enabled);
        assert_eq!(statuses[2], entry(12, "No molestar", true));
        let Item::Submenu { enabled, .. } = &menu(&TrayState::default())[4] else {
            panic!("the status is a submenu")
        };
        assert!(!enabled, "without an account there is no status");
        // In the language of the interface.
        let english = menu(&TrayState {
            language: rune_i18n::Language::English,
            ..Default::default()
        });
        assert_eq!(english[0], entry(OPEN, "Open RUNE", false));
    }

    #[test]
    fn each_command_is_the_menu_s_choice() {
        assert_eq!(command(OPEN), Some(TrayEvent::Open));
        assert_eq!(command(QUIT), Some(TrayEvent::Quit));
        assert_eq!(command(13), Some(TrayEvent::Status(TrayStatus::Invisible)));
        // Nothing chosen (0), or another number.
        assert_eq!(command(0), None);
        assert_eq!(command(14), None);
    }

    #[test]
    fn the_tooltip_and_the_icon_are_what_the_shell_reads() {
        let state = TrayState {
            mentions: 2,
            ..Default::default()
        };
        let tip = tooltip(&state);
        let text =
            String::from_utf16_lossy(&tip[..tip.iter().position(|unit| *unit == 0).unwrap()]);
        assert_eq!(text, "RUNE\n2 menciones");
        assert_eq!(tip[127], 0, "always ends in NUL");
        let pixels = bgra(16, false);
        assert_eq!(pixels.len(), 16 * 16 * 4);
        // RUNE's icon (barra-e-icono), its first pixel's channels turned.
        let (_, _, argb) = mark::pixmap(16, false);
        assert_eq!(&pixels[..4], &[argb[3], argb[2], argb[1], argb[0]]);
    }
}
