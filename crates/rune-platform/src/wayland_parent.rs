//! The window as the parent of the portal's dialogs on Wayland
//! (escritorio-linux): its toplevel exported with xdg-foreign v2
//! (`zxdg_exporter_v2`), whose handle the portal imports to put its dialog
//! over RUNE's window, modal to it. A compositor without the protocol leaves
//! the dialogs unparented, as before. X11 needs none of this: the window's
//! id is its parent ([`ParentWindow::x11`]).

use std::ffi::c_void;
use std::ptr::NonNull;

use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_surface::WlSurface};
use wayland_client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::foreign::zv2::client::{
    zxdg_exported_v2::{self, ZxdgExportedV2},
    zxdg_exporter_v2::ZxdgExporterV2,
};

use crate::file_dialogs::ParentWindow;

/// The handle the compositor gave the export.
#[derive(Default)]
struct Exported(Option<String>);

impl Dispatch<WlRegistry, GlobalListContents> for Exported {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Exported: ignore ZxdgExporterV2);

impl Dispatch<ZxdgExportedV2, ()> for Exported {
    fn event(
        state: &mut Self,
        _: &ZxdgExportedV2,
        event: zxdg_exported_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_exported_v2::Event::Handle { handle } = event {
            state.0 = Some(handle);
        }
    }
}

/// Export the toplevel `surface` of the Wayland connection `display` as a
/// dialog's parent; `None` when the compositor cannot. It blocks for two
/// round trips to the compositor, as winit's own window setup does, so call
/// it once and keep the result. The export lasts as long as the process:
/// the handle is valid only while its object lives, and RUNE has one window.
///
/// # Safety
///
/// `display` and `surface` are the live `wl_display` and the toplevel
/// `wl_surface` of RUNE's window, created with `wayland-client` (as winit
/// does), and stay alive for as long as the process uses the handle.
pub unsafe fn export(display: NonNull<c_void>, surface: NonNull<c_void>) -> Option<ParentWindow> {
    // A guest of winit's connection: dropping it never closes the display.
    let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };
    let connection = Connection::from_backend(backend);
    let (globals, queue) = registry_queue_init::<Exported>(&connection).ok()?;
    // Never dropped: its objects would be destroyed with it, and libwayland
    // warns of a queue destroyed with them attached.
    let queue = Box::leak(Box::new(queue));
    let handle = queue.handle();
    let exporter: ZxdgExporterV2 = globals.bind(&handle, 1..=1, ()).ok()?;
    let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.as_ptr().cast()) }.ok()?;
    let surface = WlSurface::from_id(&connection, id).ok()?;
    // Dropping a proxy sends nothing: the export stays until the process ends.
    let _exported = exporter.export_toplevel(&surface, &handle, ());
    let mut state = Exported::default();
    queue.roundtrip(&mut state).ok()?;
    ParentWindow::wayland(state.0.as_deref()?)
}
