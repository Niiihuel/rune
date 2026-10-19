//! A screen or a window through Windows.Graphics.Capture (Windows 10 1903
//! or later), on a Direct3D 11 device of its own: each frame kept is copied
//! from the GPU to memory and scaled into the queues, the rest are let go
//! at once. Windows draws its own border around what is captured while it
//! is. RUNE's picker lists what can be chosen ([`sources`]): the screens,
//! the main one first, and the windows people have on the taskbar, never
//! RUNE's own.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rune_i18n::{es, t, tr};
use windows::core::{Interface, BOOL};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, HDC, HMONITOR, MONITORINFO,
    MONITORINFOEXW, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindow, GetWindowLongPtrW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindow, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_TOOLWINDOW,
};

use super::super::{convert_within, lock, Frame, Keeper, Pixels, Plane, ScreenSource, Shared};
use super::{bounded, Com, CHECK, FIRST_FRAME, GONE, NO_FRAMES, UNAVAILABLE};

/// Windows listed at most, and the longest name kept.
const MAX_WINDOWS: usize = 32;
const MAX_NAME: usize = 128;
/// How often a new frame is looked for: a 60 Hz screen's at least.
const POLL: Duration = Duration::from_millis(4);

const OLD_WINDOWS: &str =
    es("Esta versión de Windows no permite capturar la pantalla: hace falta Windows 10 (1903) o posterior.");
const WINDOW_GONE: &str = es("La ventana elegida ya no está abierta.");

/// What the picker's id asks for.
#[derive(Debug, PartialEq, Eq)]
enum Target {
    /// A screen by its device name (`\\.\DISPLAY1`); `None` is the main one.
    Screen(Option<String>),
    Window(isize),
}

/// The target of an id [`sources`] gave; `None` names none of them.
fn target(id: Option<&str>) -> Option<Target> {
    let Some(id) = id else {
        return Some(Target::Screen(None));
    };
    match id.split_once(':')? {
        ("screen", device) if !device.is_empty() => Some(Target::Screen(Some(device.to_owned()))),
        ("window", handle) => handle
            .parse()
            .ok()
            .filter(|handle| *handle != 0)
            .map(Target::Window),
        _ => None,
    }
}

/// One monitor as [`sources`] offers it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Monitor {
    device: String,
    primary: bool,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

/// The monitors as the picker shows them: the main one first, the others
/// from left to right, each named by its place.
fn screens(mut monitors: Vec<Monitor>) -> Vec<ScreenSource> {
    monitors.sort_by_key(|monitor| (!monitor.primary, monitor.left, monitor.top));
    monitors
        .iter()
        .enumerate()
        .map(|(at, monitor)| {
            let (number, width, height) = (at + 1, monitor.width, monitor.height);
            ScreenSource {
                id: format!("screen:{}", monitor.device),
                name: if monitor.primary {
                    tr!(
                        "Pantalla {number} (principal) · {width}×{height}",
                        number,
                        width,
                        height
                    )
                } else {
                    tr!(
                        "Pantalla {number} · {width}×{height}",
                        number,
                        width,
                        height
                    )
                },
                window: false,
            }
        })
        .collect()
}

unsafe extern "system" fn monitor_found(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    found: LPARAM,
) -> BOOL {
    // SAFETY: `found` is the vector `monitors` lent for this enumeration.
    let found = unsafe { &mut *(found.0 as *mut Vec<HMONITOR>) };
    found.push(monitor);
    true.into()
}

/// The monitors on now, with their handles.
fn monitors() -> Vec<(HMONITOR, Monitor)> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only pushes to `handles`, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_found),
            LPARAM(&mut handles as *mut Vec<HMONITOR> as isize),
        );
    }
    handles
        .into_iter()
        .filter_map(|handle| {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            // SAFETY: an info that says its own, extended, size.
            let read = unsafe { GetMonitorInfoW(handle, &mut info as *mut _ as *mut MONITORINFO) };
            if !read.as_bool() {
                return None;
            }
            let area = info.monitorInfo.rcMonitor;
            let device = text(&info.szDevice)?;
            Some((
                handle,
                Monitor {
                    device,
                    // MONITORINFOF_PRIMARY.
                    primary: info.monitorInfo.dwFlags & 1 != 0,
                    left: area.left,
                    top: area.top,
                    width: area.right - area.left,
                    height: area.bottom - area.top,
                },
            ))
        })
        .collect()
}

/// The text of a NUL-ended UTF-16 field.
fn text(field: &[u16]) -> Option<String> {
    let end = field
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(field.len());
    String::from_utf16(&field[..end])
        .ok()
        .filter(|text| !text.is_empty())
}

/// What decides whether a window is offered.
struct Seen {
    visible: bool,
    owned: bool,
    tool: bool,
    cloaked: bool,
    class: String,
    title: String,
}

/// A window people see on the taskbar, with a title, not the desktop's.
fn offered(seen: &Seen) -> bool {
    seen.visible
        && !seen.owned
        && !seen.tool
        && !seen.cloaked
        && !seen.title.trim().is_empty()
        && !matches!(seen.class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd")
}

unsafe extern "system" fn window_found(window: HWND, found: LPARAM) -> BOOL {
    // SAFETY: `found` is the vector `windows` lent for this enumeration.
    let found = unsafe { &mut *(found.0 as *mut Vec<HWND>) };
    found.push(window);
    (found.len() < 4096).into()
}

/// The windows offered now, front to back, at most [`MAX_WINDOWS`].
fn windows() -> Vec<ScreenSource> {
    let mut handles: Vec<HWND> = Vec::new();
    // SAFETY: the callback only pushes to `handles`, which outlives the call.
    unsafe {
        let _ = EnumWindows(
            Some(window_found),
            LPARAM(&mut handles as *mut Vec<HWND> as isize),
        );
    }
    let me = std::process::id();
    handles
        .into_iter()
        .filter_map(|window| {
            // RUNE's own windows are never offered, nor asked anything: their
            // title would be a message to this process's interface, which
            // waits for it (Wine never answered while RUNE drew).
            let mut pid = 0;
            // SAFETY: a plain query; a closed window gives no process.
            unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
            if pid == me || pid == 0 {
                return None;
            }
            // SAFETY: plain queries of a window that may have closed since:
            // then they fail, and it is not offered.
            let seen = unsafe {
                let mut cloaked = 0u32;
                let cloaked = DwmGetWindowAttribute(
                    window,
                    DWMWA_CLOAKED,
                    &mut cloaked as *mut u32 as *mut _,
                    4,
                )
                .is_ok()
                    && cloaked != 0;
                let mut class = [0u16; 64];
                let length = GetClassNameW(window, &mut class).max(0) as usize;
                // Another process's title is read without asking its window.
                let mut title = [0u16; 256];
                let written = GetWindowTextW(window, &mut title).max(0) as usize;
                Seen {
                    visible: IsWindowVisible(window).as_bool(),
                    owned: GetWindow(window, GW_OWNER).is_ok_and(|owner| !owner.is_invalid()),
                    tool: GetWindowLongPtrW(window, GWL_EXSTYLE) & WS_EX_TOOLWINDOW.0 as isize != 0,
                    cloaked,
                    class: String::from_utf16_lossy(&class[..length.min(class.len())]),
                    title: String::from_utf16_lossy(&title[..written.min(title.len())]),
                }
            };
            offered(&seen).then(|| ScreenSource {
                id: format!("window:{}", window.0 as isize),
                name: bounded(&seen.title, MAX_NAME),
                window: true,
            })
        })
        .take(MAX_WINDOWS)
        .collect()
}

/// The screens, the main one first, then the windows: what RUNE's picker
/// offers. Quick, but it asks every window: off the interface's thread.
pub fn sources() -> Vec<ScreenSource> {
    let mut sources = screens(monitors().into_iter().map(|(_, monitor)| monitor).collect());
    sources.extend(windows());
    sources
}

/// The capture item of `chosen`, or why there is none.
fn item(chosen: Option<&str>) -> Result<GraphicsCaptureItem, String> {
    let unavailable = |_| t(UNAVAILABLE).to_owned();
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(unavailable)?;
    match target(chosen).ok_or_else(|| t(WINDOW_GONE).to_owned())? {
        Target::Window(handle) => {
            let window = HWND(handle as *mut _);
            // SAFETY: a plain check; a handle that is no window fails it.
            if !unsafe { IsWindow(Some(window)) }.as_bool() {
                return Err(t(WINDOW_GONE).into());
            }
            // SAFETY: a window that exists, asked for once.
            unsafe { interop.CreateForWindow(window) }.map_err(unavailable)
        }
        Target::Screen(device) => {
            let monitor = monitors()
                .into_iter()
                .find(|(_, monitor)| Some(&monitor.device) == device.as_ref())
                .map(|(handle, _)| handle);
            let monitor = match (monitor, device) {
                (Some(monitor), _) => monitor,
                // SAFETY: a plain query of the main monitor.
                (None, None) => unsafe {
                    MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY)
                },
                (None, Some(_)) => return Err(t(GONE).into()),
            };
            // SAFETY: a monitor that is on, asked for once.
            unsafe { interop.CreateForMonitor(monitor) }.map_err(unavailable)
        }
    }
}

/// A Direct3D 11 device of its own: the GPU's, or Windows' software one.
fn device() -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext, IDirect3DDevice)> {
    let mut last = windows::core::Error::empty();
    for driver in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
        let (mut device, mut context) = (None, None);
        // SAFETY: valid out-pointers; the objects are owned here.
        let made = unsafe {
            D3D11CreateDevice(
                None,
                driver,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        };
        match (made, device, context) {
            (Ok(()), Some(device), Some(context)) => {
                let dxgi: IDXGIDevice = device.cast()?;
                // SAFETY: a DXGI device of a live Direct3D device.
                let direct: IDirect3DDevice =
                    unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }?.cast()?;
                return Ok((device, context, direct));
            }
            (Err(error), ..) => last = error,
            _ => {}
        }
    }
    Err(last)
}

/// A texture the CPU reads, of `size`, kept while frames keep that size.
struct Staging {
    texture: ID3D11Texture2D,
    size: (u32, u32),
}

/// Copy the frame's picture to memory and convert it within `max`.
fn picture(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    frame: &Direct3D11CaptureFrame,
    content: SizeInt32,
    staging: &mut Option<Staging>,
    max: (u32, u32),
) -> Option<Frame> {
    // SAFETY: Direct3D calls on this thread's own device; the mapped bytes
    // are read only between Map and Unmap, within the size Map gave.
    unsafe {
        let texture: ID3D11Texture2D = frame
            .Surface()
            .ok()?
            .cast::<IDirect3DDxgiInterfaceAccess>()
            .ok()?
            .GetInterface()
            .ok()?;
        let mut description = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut description);
        // A window may have shrunk under its buffer: only its content.
        let width = u32::try_from(content.Width).ok()?.min(description.Width);
        let height = u32::try_from(content.Height).ok()?.min(description.Height);
        if width == 0 || height == 0 {
            return None;
        }
        if staging
            .as_ref()
            .is_none_or(|kept| kept.size != (width, height))
        {
            let wanted = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            let mut made = None;
            device
                .CreateTexture2D(&wanted, None, Some(&mut made))
                .ok()?;
            *staging = Some(Staging {
                texture: made?,
                size: (width, height),
            });
        }
        let target = &staging.as_ref()?.texture;
        let area = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: width,
            bottom: height,
            back: 1,
        };
        context.CopySubresourceRegion(target, 0, 0, 0, 0, &texture, 0, Some(&area));
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context
            .Map(target, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .ok()?;
        let converted = (!mapped.pData.is_null()).then(|| {
            let bytes =
                std::slice::from_raw_parts(mapped.pData as *const u8, mapped.DepthPitch as usize);
            let plane = Plane {
                bytes,
                stride: mapped.RowPitch as usize,
            };
            convert_within(Pixels::Bgr4, width, height, &[plane], max)
        });
        context.Unmap(target, 0);
        converted.flatten()
    }
}

/// Read `chosen` into `shared` until it stops, ends or fails.
pub(super) fn read(chosen: Option<&str>, shared: &Arc<Shared>) -> Result<(), String> {
    let _com = Com::start()?;
    let unavailable = |_| t(UNAVAILABLE).to_owned();
    if !GraphicsCaptureSession::IsSupported().unwrap_or(false) {
        return Err(t(OLD_WINDOWS).into());
    }
    let item = item(chosen)?;
    let (device, context, direct) = device().map_err(unavailable)?;
    let format = DirectXPixelFormat::B8G8R8A8UIntNormalized;
    let mut size = item.Size().map_err(unavailable)?;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&direct, format, 2, size)
        .map_err(unavailable)?;
    let session = pool.CreateCaptureSession(&item).map_err(unavailable)?;
    // The window closed, or the screen went away.
    let closed = Arc::new(AtomicBool::new(false));
    let told = closed.clone();
    let _ = item.Closed(&TypedEventHandler::new(move |_, _| {
        told.store(true, Ordering::Release);
        Ok(())
    }));
    session.StartCapture().map_err(unavailable)?;
    {
        let mut stats = lock(&shared.stats);
        stats.format = "BGRA";
        stats.memory = "d3d11";
    }
    let (mut keeper, mut staging) = (Keeper::default(), None);
    let started = Instant::now();
    let mut checked = started;
    let result = loop {
        let now = Instant::now();
        if now.duration_since(checked) >= CHECK {
            checked = now;
            if shared.stopped() {
                break Ok(());
            }
            if closed.load(Ordering::Acquire) {
                break Err(t(GONE).to_owned());
            }
            if !shared.running() && now.duration_since(started) > FIRST_FRAME {
                break Err(t(NO_FRAMES).to_owned());
            }
        }
        // Nothing new: Windows sends a frame only when something changed.
        let Ok(frame) = pool.TryGetNextFrame() else {
            std::thread::sleep(POLL);
            continue;
        };
        let Ok(content) = frame.ContentSize() else {
            let _ = frame.Close();
            continue;
        };
        if (content.Width, content.Height) != (size.Width, size.Height) {
            // A new size: the buffers follow it from the next frame on.
            size = content;
            lock(&shared.stats).size = (content.Width as u32, content.Height as u32);
            let _ = frame.Close();
            if pool.Recreate(&direct, format, 2, size).is_err() {
                break Err(t(UNAVAILABLE).to_owned());
            }
            continue;
        }
        lock(&shared.stats).size = (content.Width as u32, content.Height as u32);
        shared.deliver(&mut keeper, now, |max| {
            picture(&device, &context, &frame, content, &mut staging, max)
        });
        let _ = frame.Close();
    };
    let _ = session.Close();
    let _ = pool.Close();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_s_ids_name_a_screen_or_a_window_and_nothing_else() {
        assert_eq!(target(None), Some(Target::Screen(None)));
        assert_eq!(
            target(Some(r"screen:\\.\DISPLAY2")),
            Some(Target::Screen(Some(r"\\.\DISPLAY2".into())))
        );
        assert_eq!(target(Some("window:132456")), Some(Target::Window(132456)));
        // A window that cannot be named is never the whole screen instead.
        for wrong in [
            "window:",
            "window:0",
            "window:x",
            "screen:",
            "portal-token",
            "",
        ] {
            assert_eq!(target(Some(wrong)), None, "{wrong}");
        }
    }

    #[test]
    fn screens_are_offered_the_main_one_first_then_left_to_right() {
        let monitor = |device: &str, primary, left| Monitor {
            device: device.into(),
            primary,
            left,
            top: 0,
            width: 1920,
            height: 1080,
        };
        let offered = screens(vec![
            monitor(r"\\.\DISPLAY3", false, 1920),
            monitor(r"\\.\DISPLAY1", false, -1920),
            monitor(r"\\.\DISPLAY2", true, 0),
        ]);
        let ids: Vec<&str> = offered.iter().map(|source| source.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                r"screen:\\.\DISPLAY2",
                r"screen:\\.\DISPLAY1",
                r"screen:\\.\DISPLAY3"
            ]
        );
        assert_eq!(offered[0].name, "Pantalla 1 (principal) · 1920×1080");
        assert_eq!(offered[2].name, "Pantalla 3 · 1920×1080");
        assert!(offered.iter().all(|source| !source.window));
    }

    #[test]
    fn only_the_taskbar_s_windows_are_offered() {
        let seen = || Seen {
            visible: true,
            owned: false,
            tool: false,
            cloaked: false,
            class: "Chrome_WidgetWin_1".into(),
            title: "Un juego".into(),
        };
        assert!(offered(&seen()));
        for hidden in [
            Seen {
                visible: false,
                ..seen()
            },
            Seen {
                owned: true,
                ..seen()
            },
            Seen {
                tool: true,
                ..seen()
            },
            Seen {
                cloaked: true,
                ..seen()
            },
            Seen {
                title: " ".into(),
                ..seen()
            },
            Seen {
                class: "Progman".into(),
                ..seen()
            },
        ] {
            assert!(!offered(&hidden));
        }
    }

    // Wine lists its own desktop's monitors and windows.
    #[test]
    fn the_system_lists_at_least_one_screen() {
        let sources = sources();
        assert!(sources
            .first()
            .is_some_and(|first| !first.window && first.id.starts_with("screen:")));
        assert!(sources.iter().filter(|source| source.window).count() <= MAX_WINDOWS);
    }
}
