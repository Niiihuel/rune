//! Linux login and verification run in their own process so GTK never owns
//! RUNE's UI loop. Both windows share one hardened, ephemeral WebKit setup.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::prelude::*;
#[cfg(test)]
use rune_discord_session::Challenge;
use webkit6::{gio, glib, prelude::*};
use zeroize::Zeroizing;

use crate::protocol;
use crate::surface::{self, Outcome, Surface};

struct State {
    opened: Instant,
    lifetime: Duration,
    /// The only document that may hand a result back.
    page: fn(&str) -> bool,
    closed: Cell<bool>,
    pending: Cell<bool>,
    evaluating: Cell<bool>,
    generation: Cell<u64>,
    code: Cell<i32>,
    result: RefCell<Option<Zeroizing<String>>>,
}

impl State {
    fn active(&self) -> bool {
        !self.closed.get() && self.opened.elapsed() < self.lifetime
    }

    fn finish(&self, main: &glib::MainLoop, code: i32) {
        if !self.closed.replace(true) {
            self.code.set(code);
            main.quit();
        }
    }

    fn can_accept(&self, generation: u64, uri: &str) -> bool {
        self.active() && self.generation.get() == generation && (self.page)(uri)
    }
}

pub fn run() -> i32 {
    surface::run_login(open)
}

pub fn run_captcha() -> i32 {
    surface::run_captcha(open)
}

// Preserve only the engine's static failure category across the private IPC
// boundary. Never forward page URLs, JavaScript errors or engine stderr.
fn process_exit_code(reason: webkit6::WebProcessTerminationReason) -> i32 {
    match reason {
        webkit6::WebProcessTerminationReason::Crashed => 4,
        webkit6::WebProcessTerminationReason::ExceededMemoryLimit => 7,
        webkit6::WebProcessTerminationReason::TerminatedByApi => 8,
        _ => 9,
    }
}

#[cfg(test)]
fn window(
    capability: &str,
    lifetime: Duration,
    fixture: Option<&str>,
) -> (i32, Option<Zeroizing<String>>) {
    open(surface::login(capability, fixture), lifetime)
}

#[cfg(test)]
fn captcha_window(
    capability: &str,
    challenge: &Challenge,
    lifetime: Duration,
    fixture: Option<&str>,
) -> (i32, Option<Zeroizing<String>>) {
    open(
        surface::captcha(capability, challenge, lifetime, fixture),
        lifetime,
    )
}

/// A temporary WebKit profile in memory, with nothing a login page does not
/// need: no inspector or console output, no local files, dialogs, popups,
/// clipboard, camera, microphone or media playback.
fn hardened_settings() -> webkit6::Settings {
    webkit6::Settings::builder()
        // The same browser every request to Discord presents (2026-10-19):
        // WebKit's own user agent, pinned so the engine and the adapter
        // never drift apart.
        .user_agent(rune_discord_session::user_agent())
        // Nothing reaches the person's files or another origin from a file.
        .allow_file_access_from_file_urls(false)
        .allow_universal_access_from_file_urls(false)
        .allow_top_navigation_to_data_urls(false)
        // No surface outside the one window.
        .javascript_can_open_windows_automatically(false)
        .allow_modal_dialogs(false)
        .javascript_can_access_clipboard(false)
        // No devices and no media: the page needs neither, and a missing
        // GStreamer sink crashed the web process (DISCORD_SESSION.md).
        .enable_media(false)
        .enable_webaudio(false)
        .enable_media_stream(false)
        .enable_webrtc(false)
        // Nothing of the page leaves through the inspector or stdout.
        .enable_developer_extras(false)
        .enable_write_console_messages_to_stdout(false)
        .build()
}

fn open(surface: Surface, lifetime: Duration) -> Outcome {
    // The same application id as RUNE's window, so the desktop names and
    // groups the temporary window as RUNE instead of a generic GTK application.
    glib::set_prgname(Some("rune"));
    glib::set_application_name("RUNE");
    if gtk4::init().is_err() {
        return (6, None);
    }
    let main = glib::MainLoop::new(None, false);
    let page = surface.page;
    let state = Rc::new(State {
        opened: Instant::now(),
        lifetime,
        page,
        closed: Cell::new(false),
        pending: Cell::new(false),
        evaluating: Cell::new(false),
        generation: Cell::new(0),
        code: Cell::new(6),
        result: RefCell::new(None),
    });
    let session = webkit6::NetworkSession::new_ephemeral();
    session.set_persistent_credential_storage_enabled(false);
    session.set_tls_errors_policy(webkit6::TLSErrorsPolicy::Fail);
    session.connect_download_started(|_, download| download.cancel());
    let settings = hardened_settings();
    let manager = webkit6::UserContentManager::new();
    manager.add_script(&webkit6::UserScript::new(
        &surface.script,
        webkit6::UserContentInjectedFrames::TopFrame,
        webkit6::UserScriptInjectionTime::Start,
        &[surface.script_origin],
        &[],
    ));
    let view = webkit6::WebView::builder()
        .network_session(&session)
        .settings(&settings)
        .user_content_manager(&manager)
        .build();
    let window = gtk4::Window::builder()
        .title(surface.title)
        // RUNE's icon (barra-e-icono), from the theme `rune --install` fills.
        .icon_name("rune")
        .default_width(surface.size.0)
        .default_height(surface.size.1)
        .child(&view)
        .build();
    let display = gtk4::prelude::WidgetExt::display(&window);
    let cancel = gio::Cancellable::new();

    view.connect_decide_policy(move |_, decision, kind| {
        let permitted = match kind {
            webkit6::PolicyDecisionType::NavigationAction => decision
                .downcast_ref::<webkit6::NavigationPolicyDecision>()
                .and_then(|navigation| navigation.navigation_action())
                .and_then(|action| action.request())
                .and_then(|request| request.uri())
                .is_some_and(|uri| page(&uri) || protocol::challenge_origin(&uri)),
            webkit6::PolicyDecisionType::Response => decision
                .downcast_ref::<webkit6::ResponsePolicyDecision>()
                .is_some_and(|response| {
                    response.is_mime_type_supported()
                        && (!response.is_main_frame_main_resource()
                            || response
                                .request()
                                .and_then(|request| request.uri())
                                .is_some_and(|uri| page(&uri)))
                }),
            _ => false,
        };
        if permitted {
            decision.use_();
        } else {
            decision.ignore();
        }
        true
    });
    view.connect_create(|_, _| None);
    view.connect_context_menu(|_, _, _| true);
    view.connect_print(|_, _| true);
    view.connect_run_file_chooser(|_, request| {
        request.cancel();
        true
    });
    view.connect_authenticate(|_, request| {
        request.cancel();
        true
    });
    let domain = surface.domain;
    view.connect_permission_request(move |view, request| {
        // Only the hosted challenge may request temporary third-party storage.
        // Camera, microphone, notifications and every other permission stay denied.
        let allowed = view.uri().is_some_and(|uri| page(&uri))
            && request
                .downcast_ref::<webkit6::WebsiteDataAccessPermissionRequest>()
                .is_some_and(|access| {
                    access
                        .current_domain()
                        .is_some_and(|current| current == domain)
                        && access
                            .requesting_domain()
                            .is_some_and(|requesting| protocol::challenge_domain(&requesting))
                });
        if allowed {
            request.allow();
        } else {
            request.deny();
        }
        true
    });
    {
        let state = state.clone();
        let main = main.clone();
        window.connect_close_request(move |_| {
            state.finish(&main, 2);
            glib::Propagation::Proceed
        });
    }
    {
        let state = state.clone();
        let main = main.clone();
        view.connect_web_process_terminated(move |_, reason| {
            state.finish(&main, process_exit_code(reason));
        });
    }
    {
        let state = state.clone();
        view.connect_load_changed(move |_, event| {
            if event == webkit6::LoadEvent::Started {
                state.generation.set(state.generation.get().wrapping_add(1));
                state.pending.set(false);
            }
        });
    }
    {
        let state = state.clone();
        let main = main.clone();
        view.connect_load_failed_with_tls_errors(move |_, _, _, _| {
            state.finish(&main, 6);
            true
        });
    }
    {
        let state = state.clone();
        let weak_view = view.downgrade();
        manager.connect_script_message_received(Some(surface.handler), move |_, value| {
            if state.active()
                && value.is_boolean()
                && value.to_boolean()
                && weak_view
                    .upgrade()
                    .and_then(|view| view.uri())
                    .is_some_and(|uri| page(&uri))
            {
                state.pending.set(true);
            }
        });
    }
    if !manager.register_script_message_handler(surface.handler, None) {
        return (6, None);
    }
    let finish = Rc::new(surface.finish);
    let take = surface.take;
    let tick = {
        let weak_view = view.downgrade();
        let state = state.clone();
        let main = main.clone();
        let cancel = cancel.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            if state.closed.get() {
                return glib::ControlFlow::Continue;
            }
            if !state.active() {
                state.finish(&main, 3);
                return glib::ControlFlow::Continue;
            }
            if state.evaluating.get() || !state.pending.replace(false) {
                return glib::ControlFlow::Continue;
            }
            state.evaluating.set(true);
            let Some(view) = weak_view.upgrade() else {
                state.finish(&main, 9);
                return glib::ControlFlow::Continue;
            };
            let state = state.clone();
            let main = main.clone();
            let weak_view = weak_view.clone();
            let finish = finish.clone();
            let generation = state.generation.get();
            view.evaluate_javascript(&take, None, None, Some(&cancel), move |result| {
                state.evaluating.set(false);
                if !weak_view
                    .upgrade()
                    .and_then(|view| view.uri())
                    .is_some_and(|uri| state.can_accept(generation, &uri))
                {
                    return;
                }
                let Ok(value) = result else {
                    state.finish(&main, 6);
                    return;
                };
                if !value.is_string() {
                    return;
                }
                let (code, reply) = finish(Zeroizing::new(value.to_string()));
                state.result.replace(reply);
                state.finish(&main, code);
            });
            glib::ControlFlow::Continue
        })
    };
    match &surface.html {
        Some(html) => view.load_html(html, Some(surface.url)),
        None => view.load_uri(surface.url),
    }
    window.present();
    main.run();
    state.closed.set(true);
    tick.remove();
    cancel.cancel();
    manager.unregister_script_message_handler(surface.handler, None);
    manager.remove_all_scripts();
    view.stop_loading();
    view.terminate_web_process();
    window.destroy();
    display.flush();
    let result = state.result.borrow_mut().take();
    (state.code.get(), result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_failure_categories_have_distinct_static_exit_codes() {
        use webkit6::WebProcessTerminationReason as Reason;
        for (reason, expected) in [
            (Reason::Crashed, 4),
            (Reason::ExceededMemoryLimit, 7),
            (Reason::TerminatedByApi, 8),
            (Reason::__Unknown(42), 9),
        ] {
            assert_eq!(process_exit_code(reason), expected);
        }
    }

    #[test]
    fn normal_engine_shutdown_cannot_replace_a_completed_handoff() {
        let state = State {
            opened: Instant::now(),
            lifetime: Duration::from_secs(10),
            page: protocol::discord_origin,
            closed: Cell::new(false),
            pending: Cell::new(false),
            evaluating: Cell::new(false),
            generation: Cell::new(1),
            code: Cell::new(6),
            result: RefCell::new(None),
        };
        let main = glib::MainLoop::new(None, false);
        state.finish(&main, 0);
        state.finish(
            &main,
            process_exit_code(webkit6::WebProcessTerminationReason::TerminatedByApi),
        );
        assert_eq!(state.code.get(), 0);
        assert!(state.closed.get());
    }

    #[test]
    fn a_pending_main_frame_result_cannot_cross_document_generations() {
        let state = State {
            opened: Instant::now(),
            lifetime: Duration::from_secs(10),
            page: protocol::discord_origin,
            closed: Cell::new(false),
            pending: Cell::new(true),
            evaluating: Cell::new(true),
            generation: Cell::new(1),
            code: Cell::new(6),
            result: RefCell::new(None),
        };
        assert!(state.can_accept(1, "https://discord.com/login"));
        state.generation.set(2);
        assert!(!state.can_accept(1, "https://discord.com/channels/@me"));
        assert!(!state.can_accept(2, "https://evil.invalid"));
        state.closed.set(true);
        assert!(!state.can_accept(2, "https://discord.com/login"));
    }

    #[test]
    #[ignore = "requires an owned native Wayland/X11 display; synthetic HTML only"]
    fn native_synthetic_handoff_closes_the_temporary_window() {
        let capability = protocol::capability().unwrap();
        let html = r#"<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'"><title>RUNE · Prueba local sintética</title><p>Prueba local; sin conexión a Discord ni datos reales.</p><script>
            window.webkit.messageHandlers.runeLoginWake.postMessage('SYNTHETIC_FORGED_IPC');
            window.webkit.messageHandlers.runeLoginWake.postMessage(true);
            setTimeout(() => {
                const request = new XMLHttpRequest();
                request.open('GET', '/api/v10/users/@me');
                request.setRequestHeader('Authorization', 'SYNTHETIC_NATIVE_SESSION');
                // Deliberately never send: this fixture makes no network request.
            }, 300);
        </script>"#;
        let (code, result) = window(&capability, Duration::from_secs(10), Some(html));
        assert_eq!(code, 0, "the synthetic handoff should finish cleanly");
        assert!(result.is_some_and(|reply| {
            reply.as_str() == format!("{capability}:SYNTHETIC_NATIVE_SESSION")
                && protocol::decode_reply(&capability, reply.as_bytes()).is_ok()
        }));
    }

    #[test]
    #[ignore = "requires an owned native Wayland/X11 display; synthetic HTML, its CSP blocks hCaptcha"]
    fn native_synthetic_verification_hands_back_one_answer() {
        // GTK belongs to one thread per process: run the window in a fresh one.
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "webview::tests::native_verification_fixture_child",
                "--ignored",
                "--test-threads=1",
            ])
            .env("RUNE_LOGIN_CAPTCHA_FIXTURE", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("the native verification fixture must start");
        assert!(
            status.success(),
            "the native verification fixture must succeed"
        );
    }

    #[test]
    #[ignore = "subprocess fixture for the native verification test"]
    fn native_verification_fixture_child() {
        if std::env::var_os("RUNE_LOGIN_CAPTCHA_FIXTURE").is_none() {
            return;
        }
        let capability = protocol::capability().unwrap();
        let challenge = Challenge::new("synthetic-site-key".into(), None, false).unwrap();
        // A fake widget answers at once; the real script load is refused by
        // the fixture's CSP, so nothing reaches the network. It answers only
        // to the adapter's own user agent in Apple's engine, which the web
        // client names Safari (browser identity, 2026-10-19).
        let html = r#"<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'"><title>RUNE · Prueba local sintética</title><div id="status"></div><div id="captcha"></div><button id="start" hidden></button><script>
            const same = navigator.userAgent === '__RUNE_AGENT__' && /Apple/.test(navigator.vendor);
            window.hcaptcha = { render: (_, options) => { options.callback(same ? 'P1_SYNTHETIC_ANSWER' : 'P1_OTHER_BROWSER'); return 1; }, setData() {} };
            document.addEventListener('DOMContentLoaded', () => window.runeCaptchaLoaded());
        </script>"#
            .replace("__RUNE_AGENT__", rune_discord_session::user_agent());
        let (code, result) = captcha_window(
            &capability,
            &challenge,
            Duration::from_secs(10),
            Some(&html),
        );
        assert_eq!(code, 0, "the synthetic verification should finish cleanly");
        assert!(result.is_some_and(|reply| {
            reply.as_str() == format!("{capability}:P1_SYNTHETIC_ANSWER")
                && protocol::decode_answer(&capability, reply.as_bytes()).is_ok()
        }));
    }

    #[tokio::test]
    #[ignore = "requires native display and GStreamer plugins; silent local WAV only"]
    async fn native_media_initialization_must_complete_before_handoff() {
        // The Rust harness creates a separate thread for each test, even with
        // --test-threads=1. GTK must instead be initialized in a fresh process.
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "webview::tests::native_media_fixture_child",
                "--ignored",
                "--test-threads=1",
            ])
            .env("RUNE_LOGIN_MEDIA_FIXTURE", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let status = tokio::time::timeout(Duration::from_secs(15), command.status())
            .await
            .expect("the native media fixture must finish within its deadline")
            .expect("the native media fixture must start");
        assert!(status.success(), "the native media fixture must succeed");
    }

    #[test]
    #[ignore = "subprocess fixture for the native media initialization test"]
    fn native_media_fixture_child() {
        if std::env::var_os("RUNE_LOGIN_MEDIA_FIXTURE").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return;
        }
        let capability = protocol::capability().unwrap();
        let (code, result) = window(
            &capability,
            Duration::from_secs(10),
            Some(include_str!("../tests/media.html")),
        );
        assert_eq!(code, 0, "native audio metadata loading must finish cleanly");
        assert!(result.is_some_and(|reply| {
            reply.as_str() == format!("{capability}:SYNTHETIC_MEDIA_SESSION")
                && protocol::decode_reply(&capability, reply.as_bytes()).is_ok()
        }));
    }

    #[test]
    #[ignore = "subprocess fixture for the owned native cancellation test"]
    fn native_idle_fixture_child() {
        let Some(path) = std::env::var_os("RUNE_LOGIN_FIXTURE_PID") else {
            return;
        };
        std::fs::write(path, std::process::id().to_string()).unwrap();
        let capability = protocol::capability().unwrap();
        let html = "<!doctype html><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'\"><title>RUNE · Prueba local de cierre</title><p>Prueba local, sin red ni credenciales.</p>";
        let _ = window(&capability, Duration::from_secs(30), Some(html));
    }

    // Enumerate only descendants of this test's owned helper. Never inspect
    // command lines, environments, browser profiles or another user's process.
    fn descendants(pid: u32) -> Vec<u32> {
        let mut result = Vec::new();
        if let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) {
            for task in tasks.flatten() {
                if let Ok(children) = std::fs::read_to_string(task.path().join("children")) {
                    for child in children.split_whitespace().filter_map(|id| id.parse().ok()) {
                        if !result.contains(&child) {
                            result.push(child);
                            result.extend(descendants(child));
                        }
                    }
                }
            }
        }
        result
    }

    fn is_running(pid: u32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
            stat.rsplit_once(") ")
                .is_some_and(|(_, fields)| !fields.starts_with("Z "))
        })
    }

    #[tokio::test]
    #[ignore = "requires an owned native Wayland/X11 display; synthetic HTML only"]
    async fn cancelling_native_helper_stops_its_observed_webkit_children() {
        let location = std::env::temp_dir().join(format!(
            "rune-login-native-child-{}-{}",
            std::process::id(),
            protocol::capability().unwrap()
        ));
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "webview::tests::native_idle_fixture_child",
                "--ignored",
                "--test-threads=1",
            ])
            .env("RUNE_LOGIN_FIXTURE_PID", &location);
        let task = tokio::spawn(crate::acquire_from(command, Duration::from_secs(20)));
        let mut observed = Vec::new();
        let mut engine_started = false;
        for _ in 0..200 {
            if let Ok(pid) = std::fs::read_to_string(&location) {
                let pid = pid.parse().unwrap();
                observed = descendants(pid);
                engine_started = observed.iter().any(|pid| {
                    std::fs::read_to_string(format!("/proc/{pid}/comm"))
                        .is_ok_and(|name| name.starts_with("WebKitWeb"))
                });
                observed.push(pid);
                if engine_started {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        task.abort();
        let _ = task.await;
        if location.exists() {
            std::fs::remove_file(location).unwrap();
        }
        assert!(engine_started, "the owned WebKit fixture must have started");
        for _ in 0..200 {
            if observed.iter().all(|pid| !is_running(*pid)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("an observed WebKit fixture process remained alive after cancellation");
    }
}
