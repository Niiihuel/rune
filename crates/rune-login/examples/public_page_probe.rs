//! Manual runtime probe: a fresh public page, no input or session handoff.
//! Native engine diagnostics are safe to inspect here because the view never
//! accepts credentials and does not import an existing browser profile.

#[cfg(all(feature = "webview", target_os = "linux"))]
fn main() {
    use std::{cell::Cell, rc::Rc, time::Duration};

    use gtk4::prelude::*;
    use webkit6::{glib, prelude::*};

    gtk4::init().expect("the probe requires a native display");
    let main = glib::MainLoop::new(None, false);
    let failed = Rc::new(Cell::new(false));
    let loaded = Rc::new(Cell::new(false));
    let session = webkit6::NetworkSession::new_ephemeral();
    session.set_persistent_credential_storage_enabled(false);
    session.set_tls_errors_policy(webkit6::TLSErrorsPolicy::Fail);
    let settings = webkit6::Settings::new();
    settings.set_enable_write_console_messages_to_stdout(false);
    settings.set_enable_developer_extras(false);
    settings.set_enable_media_stream(false);
    settings.set_enable_webrtc(false);
    settings.set_enable_media(false);
    settings.set_enable_webaudio(false);
    let view = webkit6::WebView::builder()
        .network_session(&session)
        .settings(&settings)
        .build();
    // The public page must never become an interactive login surface.
    view.set_sensitive(false);
    view.connect_create(|_, _| None);
    view.connect_permission_request(|_, request| {
        request.deny();
        true
    });
    let window = gtk4::Window::builder()
        .title("RUNE · Prueba pública sin entrada de datos")
        .default_width(920)
        .default_height(720)
        .child(&view)
        .build();
    {
        let main = main.clone();
        let failed = failed.clone();
        view.connect_web_process_terminated(move |_, reason| {
            eprintln!("probe: web-process-terminated {reason:?}");
            failed.set(true);
            main.quit();
        });
    }
    {
        let loaded = loaded.clone();
        view.connect_load_changed(move |_, event| {
            eprintln!("probe: load {event:?}");
            if event == webkit6::LoadEvent::Finished {
                loaded.set(true);
            }
        });
    }
    view.connect_load_failed(|_, _, _, _| {
        eprintln!("probe: load-failed");
        false
    });
    {
        let main = main.clone();
        window.connect_close_request(move |_| {
            main.quit();
            glib::Propagation::Proceed
        });
    }
    {
        let main = main.clone();
        glib::timeout_add_local_once(Duration::from_secs(20), move || main.quit());
    }
    if std::env::args().any(|arg| arg == "--media-fixture") {
        // Ten milliseconds of original silent PCM. Loading (without playing)
        // exercises the same native media initialization as a web audio element.
        view.load_html(
            include_str!("../tests/media.html"),
            Some("https://discord.com/login"),
        );
    } else {
        view.load_uri("https://discord.com/login");
    }
    window.present();
    main.run();
    let success = loaded.get() && !failed.get();
    view.stop_loading();
    view.terminate_web_process();
    window.destroy();
    eprintln!("probe: public-page-survived={success}");
    std::process::exit(if success { 0 } else { 1 });
}

#[cfg(not(all(feature = "webview", target_os = "linux")))]
fn main() {
    eprintln!("This probe requires Linux and --features webview.");
    std::process::exit(2);
}
