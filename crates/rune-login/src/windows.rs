//! Windows login and verification (windows-login): the same temporary,
//! isolated windows as Linux's, on WebView2, the engine Windows ships. Each
//! runs in its own helper process with a profile of its own, InPrivate, in a
//! folder that is deleted once its browser has exited. Only the main frame's
//! own page can hand a result back, through the private pipe the parent
//! created; the page's message to RUNE is a bare wake, never the result.
//!
//! A third, hidden helper reads what a page in this engine sees of its
//! browser, for the adapter to present that same one (`Engine`).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use ::windows::core::{w, Interface, BOOL, HSTRING, PCWSTR, PWSTR};
use ::windows::Win32::Foundation::{E_POINTER, HWND, LPARAM, LRESULT, RECT, WPARAM};
use ::windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::UI::HiDpi::{
    GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use ::windows::Win32::UI::Shell::SHCreateMemStream;
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, GetMessageW, GetSystemMetrics, KillTimer, LoadCursorW, PostQuitMessage,
    RegisterClassExW, SetTimer, ShowWindow, TranslateMessage, CW_USEDEFAULT, HICON, IDC_ARROW,
    LR_DEFAULTCOLOR, MSG, SM_CXICON, SM_CXSMICON, SW_SHOWNORMAL, WINDOW_EX_STYLE, WM_CLOSE,
    WM_SIZE, WM_TIMER, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
};
use rune_discord_session::Engine;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    BasicAuthenticationRequestedEventHandler, BrowserProcessExitedEventHandler,
    CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    DownloadStartingEventHandler, ExecuteScriptCompletedHandler, NavigationCompletedEventHandler,
    NavigationStartingEventHandler, NewWindowRequestedEventHandler,
    PermissionRequestedEventHandler, ProcessFailedEventHandler, WebMessageReceivedEventHandler,
    WebResourceRequestedEventHandler,
};
use zeroize::Zeroizing;

use crate::protocol::{self, CAPTCHA_PAGE, RUNTIME_MISSING};
use crate::surface::{self, Outcome, Surface};
use crate::{AGENT_VARIABLE, ENGINE_LIFETIME};

/// RUNE's icon (barra-e-icono), the same file the installer ships.
const ICON: &[u8] = include_bytes!("../../../assets/icons/rune.ico");

/// The window's one timer: lifetime and pending results, every 100 ms.
const TICK: usize = 1;
/// How long a closed window's browser may take to exit before its profile
/// folder is deleted anyway (what it still holds stays until the next run).
const EXIT_WAIT: Duration = Duration::from_secs(5);
/// Profile folders a helper left behind (killed, crashed) are deleted by the
/// next one once older than any window lives.
const STALE: Duration = Duration::from_secs(20 * 60);

pub fn run() -> i32 {
    surface::run_login(|surface, lifetime| open(surface, lifetime, true))
}

pub fn run_captcha() -> i32 {
    surface::run_captcha(|surface, lifetime| open(surface, lifetime, true))
}

/// Read, in a hidden window, what a page in this engine sees of its browser.
pub fn run_engine() -> i32 {
    if !surface::piped() {
        return 6;
    }
    let (code, result) = open(engine_surface(), ENGINE_LIFETIME, false);
    surface::reply(code, result)
}

/// RUNE's own empty page, which only tells the probe it loaded; the probe
/// then reads what the web client builds its identity from.
fn engine_surface() -> Surface {
    let page = "if (window === window.top && location.href === 'https://captcha.rune.invalid/') window.chrome.webview.postMessage(true);";
    let read = format!("(() => {{ if (window !== window.top || location.href !== '{CAPTCHA_PAGE}') return null; const hints = navigator.userAgentData; return JSON.stringify({{ user_agent: navigator.userAgent, vendor: navigator.vendor, brands: hints ? hints.brands.map(({{ brand, version }}) => ({{ brand, version }})) : [], platform: hints ? hints.platform : '', language: (navigator.languages || [])[0] || navigator.language, timezone: Intl.DateTimeFormat().resolvedOptions().timeZone }}); }})()");
    Surface {
        title: "RUNE",
        size: (320, 240),
        page: protocol::captcha_page,
        domain: "rune.invalid",
        html: Some("<!doctype html><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'\"><title>RUNE</title>".into()),
        url: CAPTCHA_PAGE,
        handler: "",
        script: page.into(),
        script_origin: "",
        take: read,
        finish: Box::new(|json| {
            if json.len() <= protocol::MAX_ENGINE && serde_json::from_str::<Engine>(&json).is_ok() {
                (0, Some(json))
            } else {
                (6, None)
            }
        }),
    }
}

/// One temporary window's state, owned by its thread.
struct Shown {
    opened: Instant,
    lifetime: Duration,
    /// The only document that may hand a result back.
    page: fn(&str) -> bool,
    closed: Cell<bool>,
    /// The page woke RUNE: take its value at the next tick.
    pending: Cell<bool>,
    evaluating: Cell<bool>,
    /// Each top-level navigation's own, so a value never crosses documents.
    generation: Cell<u64>,
    code: Cell<i32>,
    result: RefCell<Option<Zeroizing<String>>>,
    take: String,
    finish: surface::Finish,
    controller: RefCell<Option<ICoreWebView2Controller>>,
    webview: RefCell<Option<ICoreWebView2>>,
}

thread_local! {
    /// The window procedure's way to its window; one window per helper.
    static SHOWN: RefCell<Option<Rc<Shown>>> = const { RefCell::new(None) };
}

impl Shown {
    fn active(&self) -> bool {
        !self.closed.get() && self.opened.elapsed() < self.lifetime
    }

    /// End with `code`, once: a later end never replaces the first.
    fn close(&self, code: i32) {
        if !self.closed.replace(true) {
            self.code.set(code);
            // SAFETY: posts WM_QUIT to this thread's own message loop.
            unsafe { PostQuitMessage(0) };
        }
    }

    fn can_accept(&self, generation: u64, uri: &str) -> bool {
        self.active() && self.generation.get() == generation && (self.page)(uri)
    }

    /// Every 100 ms: the lifetime, then a woken page's value.
    fn tick(self: &Rc<Self>) {
        if self.closed.get() {
            return;
        }
        if !self.active() {
            return self.close(3);
        }
        if self.evaluating.get() || !self.pending.replace(false) {
            return;
        }
        let Some(webview) = self.webview.borrow().clone() else {
            return self.close(9);
        };
        self.evaluating.set(true);
        let generation = self.generation.get();
        let weak = Rc::downgrade(self);
        let view = webview.clone();
        let handler = ExecuteScriptCompletedHandler::create(Box::new(move |result, json| {
            let json = Zeroizing::new(json);
            let Some(shown) = weak.upgrade() else {
                return Ok(());
            };
            shown.evaluating.set(false);
            if !source(&view).is_some_and(|uri| shown.can_accept(generation, &uri)) {
                return Ok(());
            }
            if result.is_err() {
                shown.close(6);
                return Ok(());
            }
            // `null` (nothing yet) and anything but a string wait for another wake.
            if let Ok(Some(value)) = serde_json::from_str::<Option<String>>(&json) {
                let (code, reply) = (shown.finish)(Zeroizing::new(value));
                shown.result.replace(reply);
                shown.close(code);
            }
            Ok(())
        }));
        // SAFETY: a COM call on the thread that created the WebView.
        if unsafe { webview.ExecuteScript(&HSTRING::from(self.take.as_str()), &handler) }.is_err() {
            self.close(6);
        }
    }

    /// The WebView fills the window's client area.
    fn fit(&self, window: HWND) {
        let mut area = RECT::default();
        if let Some(controller) = self.controller.borrow().as_ref() {
            // SAFETY: this thread's own window and controller.
            unsafe {
                if GetClientRect(window, &mut area).is_ok() {
                    let _ = controller.SetBounds(area);
                }
            }
        }
    }
}

/// A string WebView2 allocated, copied out and freed.
fn owned(read: impl FnOnce(*mut PWSTR) -> ::windows::core::Result<()>) -> Option<String> {
    let mut value = PWSTR::null();
    read(&mut value).ok()?;
    if value.is_null() {
        return None;
    }
    // SAFETY: WebView2 returned a NUL-terminated string allocated with
    // CoTaskMemAlloc, which is ours to free once copied.
    unsafe {
        let text = value.to_string().ok();
        CoTaskMemFree(Some(value.0 as _));
        text
    }
}

fn source(webview: &ICoreWebView2) -> Option<String> {
    // SAFETY: a property read on the WebView's own thread.
    owned(|value| unsafe { webview.Source(value) })
}

/// Where this window's browser keeps its profile: a fresh folder of its own
/// under the user's temporary folder, after deleting what killed or crashed
/// helpers left. Never another browser's or RUNE's own data.
fn profile() -> Option<PathBuf> {
    let base = std::env::temp_dir().join("RUNE-webview");
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            let old = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                .is_some_and(|age| age > STALE);
            if old {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let name = protocol::capability().ok()?;
    let folder = base.join(&name[..32]);
    std::fs::create_dir_all(&folder).ok()?;
    Some(folder)
}

/// Delete `folder`, retrying while the exiting browser still holds a file.
fn forget(folder: &Path) {
    for _ in 0..10 {
        if std::fs::remove_dir_all(folder).is_ok() || !folder.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The image of the `.ico` `file` to draw at `size` pixels: the smallest
/// at least that large, else the largest. `None` if it does not read.
fn icon_image(file: &[u8], size: i32) -> Option<&[u8]> {
    let count = usize::from(u16::from_le_bytes(file.get(4..6)?.try_into().ok()?));
    (0..count)
        .filter_map(|index| {
            let entry = file.get(6 + 16 * index..6 + 16 * (index + 1))?;
            let side = if entry[0] == 0 {
                256
            } else {
                i32::from(entry[0])
            };
            let length = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
            let offset = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
            Some((side, file.get(offset..offset.checked_add(length)?)?))
        })
        .min_by_key(|(side, _)| {
            if *side >= size {
                (0, *side)
            } else {
                (1, -*side)
            }
        })
        .map(|(_, image)| image)
}

/// RUNE's icon at `size` pixels; the window keeps Windows' default without.
fn icon(size: i32) -> HICON {
    icon_image(ICON, size)
        // SAFETY: the bytes are one image of the icon file, bitmap or PNG,
        // as `CreateIconFromResourceEx` reads them (format 3.0).
        .and_then(|image| unsafe {
            CreateIconFromResourceEx(image, true, 0x0003_0000, size, size, LR_DEFAULTCOLOR).ok()
        })
        .unwrap_or_default()
}

/// Whether any WebView2 Runtime is installed, as its own loader looks.
fn runtime_installed() -> bool {
    owned(|version| {
        // SAFETY: the loader writes one allocated string or nothing.
        unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), version) }
    })
    .is_some_and(|version| !version.is_empty())
}

extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let shown = SHOWN.with(|shown| shown.borrow().clone());
    match (message, shown) {
        (WM_SIZE, Some(shown)) => shown.fit(window),
        (WM_TIMER, Some(shown)) if wparam.0 == TICK => shown.tick(),
        // Closing the window ends the attempt; RUNE destroys it afterwards.
        (WM_CLOSE, Some(shown)) => shown.close(2),
        // SAFETY: the default handling of this window's own message.
        _ => return unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
    LRESULT(0)
}

fn open(surface: Surface, lifetime: Duration, visible: bool) -> Outcome {
    if !runtime_installed() {
        return (RUNTIME_MISSING, None);
    }
    let Some(folder) = profile() else {
        return (6, None);
    };
    // SAFETY: this helper's main thread hosts one single-threaded apartment.
    let outcome = unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_err() {
            (6, None)
        } else {
            show(surface, lifetime, visible, &folder).unwrap_or((6, None))
        }
    };
    forget(&folder);
    outcome
}

/// The window, its WebView and its message loop until it ends.
///
/// # Safety
/// On the thread that initialized COM as a single-threaded apartment.
unsafe fn show(
    surface: Surface,
    lifetime: Duration,
    visible: bool,
    folder: &Path,
) -> ::windows::core::Result<Outcome> {
    let instance = GetModuleHandleW(None)?;
    let class = w!("RUNE.SessionWindow");
    RegisterClassExW(&WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(procedure),
        hInstance: instance.into(),
        lpszClassName: class,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hIcon: icon(GetSystemMetrics(SM_CXICON)),
        hIconSm: icon(GetSystemMetrics(SM_CXSMICON)),
        ..Default::default()
    });
    let scale = |length: i32| length * GetDpiForSystem() as i32 / 96;
    let window = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        &HSTRING::from(surface.title),
        WS_OVERLAPPEDWINDOW,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        scale(surface.size.0),
        scale(surface.size.1),
        None,
        None,
        Some(instance.into()),
        None,
    )?;
    let environment = environment(folder)?;
    let controller = controller(&environment, window)?;
    let webview = controller.CoreWebView2()?;
    let shown = Rc::new(Shown {
        opened: Instant::now(),
        lifetime,
        page: surface.page,
        closed: Cell::new(false),
        pending: Cell::new(false),
        evaluating: Cell::new(false),
        generation: Cell::new(0),
        code: Cell::new(6),
        result: RefCell::new(None),
        take: surface.take,
        finish: surface.finish,
        controller: RefCell::new(Some(controller.clone())),
        webview: RefCell::new(Some(webview.clone())),
    });
    SHOWN.with(|slot| slot.replace(Some(shown.clone())));
    harden(&webview)?;
    guard(&webview, &shown)?;
    if let Some(html) = surface.html {
        serve(&environment, &webview, surface.url, html)?;
    }
    let script = HSTRING::from(surface.script);
    let added = mpsc::channel();
    let sender = added.0;
    webview.AddScriptToExecuteOnDocumentCreated(
        &script,
        &webview2_com::AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(
            move |result, _| {
                let _ = sender.send(result);
                Ok(())
            },
        )),
    )?;
    webview2_com::wait_with_pump(added.1)
        .map_err(|_| ::windows::core::Error::from(E_POINTER))??;
    shown.fit(window);
    if visible {
        let _ = ShowWindow(window, SW_SHOWNORMAL);
    }
    SetTimer(Some(window), TICK, 100, None);
    webview.Navigate(&HSTRING::from(surface.url))?;
    let mut message = MSG::default();
    while GetMessageW(&mut message, None, 0, 0).0 > 0 {
        let _ = TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    let _ = KillTimer(Some(window), TICK);
    shown.closed.set(true);
    let outcome = (shown.code.get(), shown.result.borrow_mut().take());
    // Let the browser exit before its folder goes: close the WebView,
    // release every reference to it, and wait a bounded while.
    let exited = Rc::new(Cell::new(false));
    if let Ok(environment) = environment.cast::<ICoreWebView2Environment5>() {
        let flag = exited.clone();
        let mut token = 0;
        let _ = environment.add_BrowserProcessExited(
            &BrowserProcessExitedEventHandler::create(Box::new(move |_, _| {
                flag.set(true);
                Ok(())
            })),
            &mut token,
        );
    }
    let _ = controller.Close();
    shown.controller.replace(None);
    shown.webview.replace(None);
    SHOWN.with(|slot| slot.replace(None));
    drop((webview, controller, shown));
    let _ = DestroyWindow(window);
    let deadline = Instant::now() + EXIT_WAIT;
    let wake = SetTimer(None, 0, 50, None);
    while !exited.get() && Instant::now() < deadline && GetMessageW(&mut message, None, 0, 0).0 > 0
    {
        let _ = TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    let _ = KillTimer(None, wake);
    Ok(outcome)
}

/// A WebView2 environment whose profile lives in `folder` alone.
unsafe fn environment(folder: &Path) -> ::windows::core::Result<ICoreWebView2Environment> {
    let (sender, receiver) = mpsc::channel();
    CreateCoreWebView2EnvironmentWithOptions(
        PCWSTR::null(),
        &HSTRING::from(folder.as_os_str()),
        None,
        &CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |result, created| {
            let _ = sender.send(result.and_then(|()| created.ok_or(E_POINTER.into())));
            Ok(())
        })),
    )?;
    webview2_com::wait_with_pump(receiver).map_err(|_| ::windows::core::Error::from(E_POINTER))?
}

/// The window's WebView, InPrivate: its browsing data stays in memory.
unsafe fn controller(
    environment: &ICoreWebView2Environment,
    window: HWND,
) -> ::windows::core::Result<ICoreWebView2Controller> {
    let environment = environment.cast::<ICoreWebView2Environment10>()?;
    let options = environment.CreateCoreWebView2ControllerOptions()?;
    options.SetIsInPrivateModeEnabled(true)?;
    let (sender, receiver) = mpsc::channel();
    environment.CreateCoreWebView2ControllerWithOptions(
        window,
        &options,
        &CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |result, created| {
            let _ = sender.send(result.and_then(|()| created.ok_or(E_POINTER.into())));
            Ok(())
        })),
    )?;
    webview2_com::wait_with_pump(receiver).map_err(|_| ::windows::core::Error::from(E_POINTER))?
}

/// Nothing a login page does not need: no developer tools, menus, dialogs,
/// browser shortcuts (print, find, save), saved passwords or autofill. Its
/// user agent is WebView2's own, the one the adapter presents; it is set
/// only if the runtime changed since that one was read, because setting it
/// may drop the client hints a page sees.
unsafe fn harden(webview: &ICoreWebView2) -> ::windows::core::Result<()> {
    let settings = webview.Settings()?;
    settings.SetAreDevToolsEnabled(false)?;
    settings.SetAreDefaultContextMenusEnabled(false)?;
    settings.SetAreDefaultScriptDialogsEnabled(false)?;
    settings.SetAreHostObjectsAllowed(false)?;
    settings.SetIsStatusBarEnabled(false)?;
    settings.SetIsWebMessageEnabled(true)?;
    if let Ok(keys) = settings.cast::<ICoreWebView2Settings3>() {
        keys.SetAreBrowserAcceleratorKeysEnabled(false)?;
    }
    if let Ok(forms) = settings.cast::<ICoreWebView2Settings4>() {
        forms.SetIsPasswordAutosaveEnabled(false)?;
        forms.SetIsGeneralAutofillEnabled(false)?;
    }
    if let (Ok(presented), Ok(agent)) = (
        std::env::var(AGENT_VARIABLE),
        settings.cast::<ICoreWebView2Settings2>(),
    ) {
        if owned(|current| agent.UserAgent(current)).as_deref() != Some(presented.as_str()) {
            agent.SetUserAgent(&HSTRING::from(presented))?;
        }
    }
    Ok(())
}

/// The page's own origin at the top, hCaptcha's in frames; one bare wake
/// from the page; no popups, downloads, permissions, sign-in prompts.
unsafe fn guard(webview: &ICoreWebView2, shown: &Rc<Shown>) -> ::windows::core::Result<()> {
    let mut token = 0;
    let page = shown.page;
    let weak = Rc::downgrade(shown);
    webview.add_NavigationStarting(
        &NavigationStartingEventHandler::create(Box::new(move |_, args| {
            let (Some(args), Some(shown)) = (args, weak.upgrade()) else {
                return Ok(());
            };
            if owned(|uri| args.Uri(uri)).is_some_and(|uri| page(&uri)) {
                shown.generation.set(shown.generation.get().wrapping_add(1));
                shown.pending.set(false);
            } else {
                args.SetCancel(true)?;
            }
            Ok(())
        })),
        &mut token,
    )?;
    webview.add_FrameNavigationStarting(
        &NavigationStartingEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let allowed = owned(|uri| args.Uri(uri)).is_some_and(|uri| {
                page(&uri)
                    || protocol::challenge_origin(&uri)
                    || uri == "about:blank"
                    || uri == "about:srcdoc"
            });
            if !allowed {
                args.SetCancel(true)?;
            }
            Ok(())
        })),
        &mut token,
    )?;
    let weak = Rc::downgrade(shown);
    webview.add_WebMessageReceived(
        &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            let (Some(args), Some(shown)) = (args, weak.upgrade()) else {
                return Ok(());
            };
            let wake = owned(|json| args.WebMessageAsJson(json)).as_deref() == Some("true");
            if wake && shown.active() && owned(|uri| args.Source(uri)).is_some_and(|uri| page(&uri))
            {
                shown.pending.set(true);
            }
            Ok(())
        })),
        &mut token,
    )?;
    let weak = Rc::downgrade(shown);
    webview.add_NavigationCompleted(
        &NavigationCompletedEventHandler::create(Box::new(move |_, args| {
            let (Some(args), Some(shown)) = (args, weak.upgrade()) else {
                return Ok(());
            };
            let mut success = BOOL::default();
            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
            args.IsSuccess(&mut success)?;
            args.WebErrorStatus(&mut status)?;
            // A certificate WebView2 refused ends the attempt, as on Linux.
            if !success.as_bool()
                && (COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_COMMON_NAME_IS_INCORRECT.0
                    ..=COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_IS_INVALID.0)
                    .contains(&status.0)
            {
                shown.close(6);
            }
            Ok(())
        })),
        &mut token,
    )?;
    let weak = Rc::downgrade(shown);
    webview.add_ProcessFailed(
        &ProcessFailedEventHandler::create(Box::new(move |_, args| {
            let (Some(args), Some(shown)) = (args, weak.upgrade()) else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
            args.ProcessFailedKind(&mut kind)?;
            let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON::default();
            if let Ok(detail) = args.cast::<ICoreWebView2ProcessFailedEventArgs2>() {
                let _ = detail.Reason(&mut reason);
            }
            // Only the static category crosses the pipe, as on Linux.
            let code = match kind {
                COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => 9,
                _ if reason == COREWEBVIEW2_PROCESS_FAILED_REASON_OUT_OF_MEMORY => 7,
                COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
                | COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => 4,
                // GPU, utility and frame processes come back by themselves.
                _ => return Ok(()),
            };
            shown.close(code);
            Ok(())
        })),
        &mut token,
    )?;
    webview.add_NewWindowRequested(
        &NewWindowRequestedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                // Handled without a window: `window.open` gets nothing.
                args.SetHandled(true)?;
            }
            Ok(())
        })),
        &mut token,
    )?;
    webview.add_PermissionRequested(
        &PermissionRequestedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
            }
            Ok(())
        })),
        &mut token,
    )?;
    webview.cast::<ICoreWebView2_4>()?.add_DownloadStarting(
        &DownloadStartingEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                args.SetCancel(true)?;
            }
            Ok(())
        })),
        &mut token,
    )?;
    if let Ok(webview) = webview.cast::<ICoreWebView2_10>() {
        webview.add_BasicAuthenticationRequested(
            &BasicAuthenticationRequestedEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    args.SetCancel(true)?;
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    Ok(())
}

/// `html` as `url`'s document, from memory: every other request to that
/// origin gets a 404 and never reaches the network.
unsafe fn serve(
    environment: &ICoreWebView2Environment,
    webview: &ICoreWebView2,
    url: &'static str,
    html: String,
) -> ::windows::core::Result<()> {
    let origin = url::Url::parse(url)
        .map(|url| url.origin().ascii_serialization())
        .map_err(|_| ::windows::core::Error::from(E_POINTER))?;
    webview.AddWebResourceRequestedFilter(
        &HSTRING::from(format!("{origin}/*")),
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
    )?;
    let environment = environment.clone();
    let mut token = 0;
    webview.add_WebResourceRequested(
        &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let request = args.Request()?;
            let own = owned(|uri| request.Uri(uri)).as_deref() == Some(url)
                && owned(|method| request.Method(method)).as_deref() == Some("GET");
            let (status, phrase, body) = if own {
                (200, "OK", html.as_bytes())
            } else {
                (404, "Not Found", &[][..])
            };
            let response = environment.CreateWebResourceResponse(
                SHCreateMemStream(Some(body)).as_ref(),
                status,
                &HSTRING::from(phrase),
                w!("Content-Type: text/html; charset=utf-8\r\nCache-Control: no-store"),
            )?;
            args.SetResponse(&response)
        })),
        &mut token,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_hands_back_only_an_engine() {
        let probe = engine_surface();
        assert!((probe.page)(CAPTCHA_PAGE) && !(probe.page)("https://discord.com/"));
        let engine = r#"{"user_agent":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0","vendor":"Google Inc.","brands":[],"platform":"Windows","language":"es-AR","timezone":"America/Argentina/Buenos_Aires"}"#;
        let (code, reply) = (probe.finish)(Zeroizing::new(engine.into()));
        assert_eq!(code, 0);
        assert_eq!(reply.as_deref().map(String::as_str), Some(engine));
        for other in ["null", "{}", "SYNTHETIC_SESSION_MARKER"] {
            assert_eq!((probe.finish)(Zeroizing::new(other.into())), (6, None));
        }
    }

    #[test]
    fn the_window_draws_the_icons_nearest_image() {
        let side = |size| icon_image(ICON, size).map(<[u8]>::len);
        // rune.ico: 16, 24, 32, 48, 64, 128 and 256 (PNG) pixels.
        assert_eq!(side(16), Some(1128));
        assert_eq!(side(20), side(24));
        assert_eq!(side(32), Some(4264));
        assert!(icon_image(ICON, 300).is_some_and(|image| image.starts_with(b"\x89PNG")));
        assert!(icon_image(ICON, 256).is_some_and(|image| image.starts_with(b"\x89PNG")));
        for broken in [&b""[..], &b"\0\0\x01\0\x05\0"[..], &ICON[..40]] {
            assert!(icon_image(broken, 32).is_none());
        }
    }

    #[test]
    #[ignore = "opens a native WebView2 window on Windows; a local page, no network"]
    fn native_probe_reads_its_own_engine() {
        let (code, reply) = open(engine_surface(), ENGINE_LIFETIME, false);
        assert_eq!(code, 0, "the WebView2 Runtime must be installed");
        let engine: Engine = serde_json::from_str(&reply.unwrap()).unwrap();
        assert!(engine.user_agent.contains("Windows NT"));
    }
}
