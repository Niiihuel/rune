//! RUNE's desktop window: limn's tree painted by its egui backend, the
//! state and the client wired together, and the frame loop that ties input,
//! provider events and painting.
//!
//! `cargo run -p rune-desktop` opens it on the simulated provider.
//! `-- --bench` scrolls the longest conversation for a few seconds, prints
//! what each frame cost, and exits. `-- --screenshot shot.png` saves the
//! window once the provider is ready and exits; `--channel <name>` opens that
//! channel first and `--action <id>` clicks a control first (repeatable), to
//! capture a given state. Mock captures also accept `--capture-draft <text>`
//! and `--capture-hover` to exercise the editor beside the last message, or
//! `--capture-hover-id <id>` to hold the pointer over a control, its hint open;
//! `--action shift` holds Shift.

// Windows (windows-base): a window, not a console program; what RUNE writes
// to stderr goes to its log file (`log.rs`). Tests keep their console.
#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod alerts;
mod animations;
mod audio;
mod capture_fixture;
// Desktop, language and local voice (P6).
mod desktop;
mod fallback_fonts;
mod gpu;
// Stability: settings kept by account, with any provider.
mod kept;
// What stderr says, also kept in a rotated file (Unix).
#[cfg(any(unix, windows))]
mod log;
mod metrics;
mod paste;
mod ptt;
mod shortcuts;
mod sounds;
mod uploads;
mod video;
// Linux desktop (escritorio-linux).
mod window;
// RUNE's own updates (actualizaciones).
mod update;
// The window's own title bar and RUNE's icon (barra-e-icono).
mod frame;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use eframe::egui;
use limn::{Element, Key, Point, Size, UiEvent, UiTree};
use rune_core::{
    Backoff, Client, ClientConfig, Deadlines, StorageTicket, StorageWriteError, Update,
};
use rune_i18n::{es, t, tr};
use rune_model::ChannelId;
use rune_provider::mock::{wall_clock, MockConfig, MockControl, MockProvider};
use rune_provider::{AuthenticationState, Provider};
use rune_state::Store;
use rune_ui::desktop::Shortcut as VoiceKey;
use rune_ui::{Intent, Ui, ViewContext};

use metrics::{FrameTimes, Metrics};

/// Provider events applied per frame at most; the rest wait for the next,
/// which is asked for at once, so a burst never stalls a frame.
const EVENTS_PER_FRAME: usize = 256;

/// Only host controls are copied; text, paste and IME payloads stay in the converter.
#[derive(Clone, Copy)]
enum NativeHostEvent {
    WindowFocused(bool),
}

/// What the command line asked for.
#[derive(Default)]
struct Arguments {
    provider: ProviderChoice,
    /// `--provider` was given: it wins over the choice remembered.
    provider_given: bool,
    discord_client_id: Option<String>,
    bench: bool,
    screenshot: Option<PathBuf>,
    channel: Option<String>,
    actions: Vec<String>,
    capture_draft: Option<String>,
    capture_hover: bool,
    /// Hold the pointer over the control with this ID, to capture its hint.
    capture_hover_id: Option<String>,
    capture_drop: bool,
    capture_attachments: bool,
    /// Three pictures to open in the viewer after the latest message.
    capture_viewer: bool,
    /// The attachments' picture plays a local two-frame animation.
    capture_animation: bool,
    capture_invitation: bool,
    /// Join the simulated club and capture what it asks (rules and onboarding).
    capture_onboarding: bool,
    capture_rich: bool,
    /// Cards with pictures after the latest message.
    capture_media: bool,
    /// An audio file and a video after the latest message, the video playing.
    capture_player: bool,
    /// Play the conversation's voice message, and capture it playing.
    capture_voice: bool,
    capture_search: Option<String>,
    capture_context: bool,
    /// `--capture-splash <ms>`: the opening screen as it is then, ready or not.
    capture_splash: Option<Duration>,
    /// Keep the local store while capturing, to see a start from the cache.
    data: bool,
    /// How long the simulated service takes to answer, as on a slow network.
    latency: Option<Duration>,
    // Desktop, language and local voice (P6).
    /// Links given on the command line, opened as one clicked in RUNE.
    links: Vec<String>,
    /// The claim on being the only RUNE, for an interactive run.
    instance: Option<desktop::Instance>,
    // Interface languages (i18n).
    /// `--language es|en`: the interface's language for this run, as if
    /// chosen in the settings; captures use it to be the same anywhere.
    language: Option<rune_i18n::Language>,
    // Desktop session and games (escritorio-sesion-y-juegos).
    /// `--start-minimized`, as the session's autostart entry starts RUNE
    /// when asked to: only the tray, no window.
    start_minimized: bool,
    // RUNE's own updates (actualizaciones).
    /// What the check before the window found.
    updates: update::Startup,
    // The window's own title bar (barra-e-icono).
    /// `--title-bar auto|rune|system`: who draws it this run, kept nowhere;
    /// captures use it to show either.
    title_bar: Option<rune_ui::title_bar::TitleBar>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ProviderChoice {
    #[default]
    Mock,
    Discord,
    DiscordSession,
}

impl ProviderChoice {
    const ALL: [Self; 3] = [Self::Mock, Self::Discord, Self::DiscordSession];

    /// What `--provider`, the interface and the configuration call it.
    fn key(self) -> &'static str {
        match self {
            Self::Mock => "mock",
            Self::Discord => "discord",
            Self::DiscordSession => "discord-session",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.key() == key)
    }

    /// Why it cannot start here, when it cannot.
    fn unavailable(self, discord_client_id: Option<&str>) -> Option<&'static str> {
        match self {
            Self::Mock => None,
            Self::Discord if discord_client_id.is_none_or(|id| id.trim().is_empty()) => Some(es(
                "Falta el ID público de tu aplicación: abrí RUNE con --discord-client-id <id> o RUNE_DISCORD_CLIENT_ID.",
            )),
            Self::Discord => None,
            Self::DiscordSession if cfg!(all(feature = "session-login", any(target_os = "linux", windows))) => {
                None
            }
            Self::DiscordSession => Some(es(
                "Esta compilación no incluye el inicio de sesión no oficial: hace falta Linux o Windows y la feature session-login.",
            )),
        }
    }

    /// As the interface offers it, with where its data comes from.
    fn option(self, discord_client_id: Option<&str>) -> rune_ui::ProviderOption {
        let (name, kind, notice) = match self {
            Self::Mock => (
                es("Simulación local"),
                rune_provider::ProviderKind::Simulated,
                es("Datos inventados en este equipo, sin conexión con ningún servicio."),
            ),
            Self::Discord => (
                es("Discord con OAuth2"),
                rune_provider::ProviderKind::Official,
                es("Autorización oficial en el navegador: tu identidad y la lista de tus servidores, sin mensajes ni voz."),
            ),
            Self::DiscordSession => (
                es("Discord con sesión personal"),
                rune_provider::ProviderKind::Unofficial,
                es("Tu sesión en una ventana de inicio propia de RUNE: conversaciones, mensajes y voz, recordada en el llavero del sistema."),
            ),
        };
        rune_ui::ProviderOption {
            key: self.key(),
            name: name.into(),
            kind,
            notice: notice.into(),
            unavailable: self.unavailable(discord_client_id).map(str::to_owned),
        }
    }
}

/// Where the provider last chosen is kept: only its key, never a credential.
fn provider_config() -> Option<PathBuf> {
    window::config_path("provider")
}

/// The provider chosen last time, if one was and it is still known.
fn remembered_provider() -> Option<ProviderChoice> {
    let key = std::fs::read_to_string(provider_config()?).ok()?;
    ProviderChoice::from_key(key.trim())
}

/// Where a start with nothing remembered begins: the Discord login when this
/// build has it, so a release never opens on the simulation by itself; the
/// simulation only for its own development options.
fn unremembered_provider(arguments: &Arguments) -> ProviderChoice {
    let simulated = arguments.data || arguments.latency.is_some();
    if !simulated && ProviderChoice::DiscordSession.unavailable(None).is_none() {
        ProviderChoice::DiscordSession
    } else {
        ProviderChoice::Mock
    }
}

/// Keep `choice` for the next start, off the interface's thread.
fn remember_provider(choice: ProviderChoice) {
    let Some(path) = provider_config() else {
        return;
    };
    std::thread::spawn(move || {
        let saved = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, choice.key()));
        if let Err(error) = saved {
            eprintln!("rune: could not remember the provider: {error}");
        }
    });
}

fn parse_arguments(
    arguments: impl IntoIterator<Item = String>,
    discord_client_id: Option<String>,
) -> Result<Arguments, String> {
    let mut parsed = Arguments {
        discord_client_id,
        ..Arguments::default()
    };
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--provider" => {
                parsed.provider = arguments
                    .next()
                    .as_deref()
                    .and_then(ProviderChoice::from_key)
                    .ok_or(t("--provider requiere mock, discord o discord-session"))?;
                parsed.provider_given = true;
            }
            "--discord-client-id" => {
                parsed.discord_client_id = Some(arguments.next().ok_or(t(
                    "--discord-client-id requiere el ID público de la aplicación",
                ))?);
            }
            "--bench" => parsed.bench = true,
            "--screenshot" => parsed.screenshot = arguments.next().map(PathBuf::from),
            "--channel" => parsed.channel = arguments.next(),
            "--action" => parsed.actions.extend(arguments.next()),
            "--capture-draft" => {
                parsed.capture_draft = Some(
                    arguments
                        .next()
                        .ok_or(t("--capture-draft requiere texto"))?,
                );
            }
            "--capture-hover" => parsed.capture_hover = true,
            "--capture-hover-id" => {
                parsed.capture_hover_id = Some(
                    arguments
                        .next()
                        .ok_or(t("--capture-hover-id requiere un id"))?,
                );
            }
            "--capture-drop" => parsed.capture_drop = true,
            "--capture-attachments" => parsed.capture_attachments = true,
            "--capture-viewer" => parsed.capture_viewer = true,
            "--capture-animation" => {
                parsed.capture_attachments = true;
                parsed.capture_animation = true;
            }
            "--capture-invitation" => parsed.capture_invitation = true,
            "--capture-onboarding" => parsed.capture_onboarding = true,
            "--capture-rich" => parsed.capture_rich = true,
            "--capture-media" => parsed.capture_media = true,
            "--capture-player" => parsed.capture_player = true,
            "--capture-voice" => parsed.capture_voice = true,
            "--capture-search" => {
                parsed.capture_search = Some(
                    arguments
                        .next()
                        .ok_or(t("--capture-search requiere texto"))?,
                )
            }
            "--capture-context" => parsed.capture_context = true,
            "--capture-splash" => {
                parsed.capture_splash = Some(Duration::from_millis(
                    arguments
                        .next()
                        .and_then(|millis| millis.parse().ok())
                        .ok_or(t("--capture-splash requiere milisegundos"))?,
                ));
            }
            "--data" => parsed.data = true,
            "--language" => {
                parsed.language = Some(
                    arguments
                        .next()
                        .as_deref()
                        .and_then(rune_i18n::Language::from_code)
                        .ok_or(t("--language requiere es o en"))?,
                );
            }
            "--latency" => {
                parsed.latency = arguments
                    .next()
                    .and_then(|millis| millis.parse().ok())
                    .map(Duration::from_millis)
            }
            // The window's own title bar (barra-e-icono).
            "--title-bar" => {
                use rune_ui::title_bar::TitleBar;
                parsed.title_bar = Some(match arguments.next().as_deref() {
                    Some("auto") => TitleBar::Automatic,
                    Some("rune") => TitleBar::Rune,
                    Some("system") => TitleBar::System,
                    _ => return Err(t("--title-bar requiere auto, rune o system").into()),
                });
            }
            // Desktop session and games (escritorio-sesion-y-juegos).
            rune_platform::autostart::MINIMIZED => parsed.start_minimized = true,
            // Desktop (P6): a link, as a later start hands it over.
            link if link.starts_with("https://") || link.starts_with("http://") => {
                parsed.links.push(link.to_owned());
            }
            other => {
                return Err(tr!("argumento desconocido: {other}; opciones: --provider mock|discord|discord-session, --discord-client-id <id>, --bench, --screenshot <archivo>, --channel <nombre>, --action <id>, --data, --latency <ms>, --language es|en, --title-bar auto|rune|system, o un enlace https://", other));
            }
        }
    }
    if parsed.provider != ProviderChoice::Mock && (parsed.bench || parsed.latency.is_some()) {
        return Err(t("--bench y --latency sólo se usan con --provider mock").into());
    }
    if (parsed.capture_draft.is_some()
        || parsed.capture_hover
        || parsed.capture_hover_id.is_some()
        || parsed.capture_drop
        || parsed.capture_attachments
        || parsed.capture_viewer
        || parsed.capture_invitation
        || parsed.capture_onboarding
        || parsed.capture_rich
        || parsed.capture_media
        || parsed.capture_player
        || parsed.capture_voice
        || parsed.capture_search.is_some()
        || parsed.capture_context
        || parsed.capture_splash.is_some())
        && (parsed.provider != ProviderChoice::Mock || parsed.screenshot.is_none())
    {
        return Err(t("Las opciones --capture-* requieren --provider mock y --screenshot").into());
    }
    if parsed.capture_search.is_some() && parsed.capture_context {
        return Err(t("Capturá búsqueda y contexto en ejecuciones separadas").into());
    }
    if parsed.provider == ProviderChoice::Discord
        && parsed
            .discord_client_id
            .as_ref()
            .is_none_or(|id| id.trim().is_empty())
    {
        return Err(t("Discord requiere el ID público de tu aplicación: --discord-client-id <id> o RUNE_DISCORD_CLIENT_ID. Registrá el redirect http://127.0.0.1:48731/oauth/callback en el Developer Portal. No ingreses contraseñas ni tokens.").into());
    }
    Ok(parsed)
}

type SelectedProvider = (Arc<dyn Provider>, Option<Arc<MockControl>>);

fn selected_provider(arguments: &Arguments) -> Result<SelectedProvider, String> {
    let mut config = MockConfig {
        ambient: !arguments.bench && arguments.screenshot.is_none(),
        ..MockConfig::default()
    };
    if let Some(latency) = arguments.latency {
        config.latency = latency;
    }
    // Without a remembered account, the login page is the first and only
    // window: RUNE's own opens once there is a session.
    let first_login = arguments.screenshot.is_none() && !arguments.bench;
    // Captures and the benchmark start from the command line alone.
    let proxy = if first_login {
        remembered_proxy()
    } else {
        rune_provider::ApiProxy::Automatic
    };
    provider_for(
        arguments.provider,
        arguments.discord_client_id.clone(),
        config,
        first_login,
        &proxy,
    )
}

// Desktop session and games (escritorio-sesion-y-juegos).
/// Where the proxy for the providers' REST requests is kept: an address
/// only, never a credential (`ApiProxy` refuses one).
const PROXY: &str = "proxy";

/// The proxy kept, or the environment's when none is or it does not read.
fn remembered_proxy() -> rune_provider::ApiProxy {
    window::remembered(PROXY)
        .and_then(|kept| rune_provider::ApiProxy::from_kept(&kept))
        .unwrap_or_default()
}

/// Start the provider `choice`. `first_login`: before the window exists,
/// an unofficial session without a remembered account opens its login.
/// Its REST requests go as `proxy` says.
fn provider_for(
    choice: ProviderChoice,
    discord_client_id: Option<String>,
    mock: MockConfig,
    first_login: bool,
    proxy: &rune_provider::ApiProxy,
) -> Result<SelectedProvider, String> {
    match choice {
        ProviderChoice::Mock => {
            let provider = Arc::new(MockProvider::new(mock));
            let control = provider.control();
            Ok((provider, Some(control)))
        }
        ProviderChoice::Discord => {
            let id = discord_client_id.ok_or(t("Falta el ID público de la aplicación"))?;
            let provider = rune_discord::DiscordProvider::with_proxy(id, proxy)
                .map_err(|error| error.to_string())?;
            Ok((Arc::new(provider), None))
        }
        ProviderChoice::DiscordSession => {
            #[cfg(all(feature = "session-login", any(target_os = "linux", windows)))]
            {
                let login: Arc<dyn rune_discord_session::SessionLogin> =
                    if first_login && !rune_discord_session::has_remembered_accounts() {
                        let first = rune_login::HostedLogin::new().acquire_blocking();
                        if first.as_ref().is_err_and(rune_login::cancelled) {
                            std::process::exit(0);
                        }
                        if let Err(error) = &first {
                            // LoginError contains only a fixed local label, never a credential.
                            eprintln!("rune: initial login failed: {error}");
                        }
                        Arc::new(rune_login::PrefetchedLogin::new(first))
                    } else {
                        Arc::new(rune_login::HostedLogin::new())
                    };
                let provider =
                    rune_discord_session::DiscordSessionProvider::with_proxy(login, proxy)
                        .map_err(|error| error.to_string())?;
                Ok((Arc::new(provider), None))
            }
            #[cfg(not(all(feature = "session-login", any(target_os = "linux", windows))))]
            {
                let _ = (first_login, proxy);
                Err(t("El login no oficial requiere Linux o Windows y compilar con --features session-login. En NixOS, usá nix-shell --arg withLogin true; la interfaz principal sigue siendo limn.").into())
            }
        }
    }
}

/// Where the GPU chosen in the settings is kept (escritorio-linux).
const GPU: &str = "gpu";
/// `RUNE_GPU` of a start again after the chosen GPU could not open the window.
const GPU_FALLBACK: &str = "fallback";

/// Choose the GPU before anything reads the environment: the Vulkan loader
/// and wgpu read these variables when the window opens. Returns whether the
/// loader was limited to one vendor's drivers (see `gpu`), and what the
/// settings show of it.
fn choose_gpu() -> (bool, rune_ui::desktop::Graphics) {
    let set = |name: &str| std::env::var_os(name).is_some();
    // Called first thing in `main`, before any thread exists, so changing the
    // environment cannot race a reader.
    if !set("WGPU_POWER_PREF") {
        std::env::set_var("WGPU_POWER_PREF", "low");
    }
    let gpus = gpu::display_controllers();
    let chosen = window::remembered(GPU);
    // Read before RUNE sets a variable of its own.
    let fallback = std::env::var("RUNE_GPU").as_deref() == Ok(GPU_FALLBACK);
    let graphics = rune_ui::desktop::Graphics {
        gpus: gpu::choices(&gpus),
        chosen: chosen.clone(),
        drawing: None,
        overridden: gpu::overridden(set)
            .filter(|_| !fallback)
            .map(str::to_owned),
        fallback,
        // The start before was limited to the person's GPU, or else to RUNE's.
        fallback_chosen: fallback && gpu::chosen_filter(&gpus, chosen.as_deref()).is_some(),
    };
    let filter = gpu::filter(&gpus, chosen.as_deref(), set);
    if let Some(filter) = filter {
        std::env::set_var("VK_LOADER_DRIVERS_SELECT", filter);
    }
    (filter.is_some(), graphics)
}

fn main() -> eframe::Result {
    // The optional browser owns a separate process and event loop. It never
    // initializes the main window, its domain store, audio or SQLite.
    #[cfg(all(feature = "session-login", any(target_os = "linux", windows)))]
    match std::env::args().nth(1).as_deref() {
        Some(rune_login::HELPER_ARGUMENT) => std::process::exit(rune_login::run_helper()),
        // An invitation's hCaptcha, solved by the person on RUNE's own page.
        Some(rune_login::CAPTCHA_ARGUMENT) => std::process::exit(rune_login::run_captcha_helper()),
        // What WebView2 tells a page of its browser (windows-login).
        Some(rune_login::ENGINE_ARGUMENT) => std::process::exit(rune_login::run_engine_helper()),
        _ => {}
    }
    // Every artifact carries the third-party notices (THIRD_PARTY_NOTICES.md,
    // generated by cargo-about): the program itself prints them.
    if std::env::args().nth(1).as_deref() == Some("--licenses") {
        print!("{}", include_str!("../../../THIRD_PARTY_NOTICES.md"));
        return Ok(());
    }
    // Until the settings are read, what is written is in the session's language.
    rune_i18n::set(rune_i18n::Language::system());
    // RUNE's own updates (actualizaciones): this program becomes the
    // installation's first version.
    #[cfg(unix)]
    if std::env::args().nth(1).as_deref() == Some("--install") {
        std::process::exit(update::install_self());
    }
    let started = Instant::now();
    let (limited, graphics) = choose_gpu();
    #[cfg(any(unix, windows))]
    let mut logging = None;
    let configured = parse_arguments(
        std::env::args().skip(1),
        std::env::var("RUNE_DISCORD_CLIENT_ID").ok(),
    )
    .map(|mut arguments| {
        // The last choice, unless `--provider` says otherwise; captures and
        // the benchmark always start from the command line alone.
        let interactive = arguments.screenshot.is_none() && !arguments.bench;
        if interactive && !arguments.provider_given {
            arguments.provider = remembered_provider()
                .filter(|choice| {
                    choice
                        .unavailable(arguments.discord_client_id.as_deref())
                        .is_none()
                })
                .unwrap_or_else(|| unremembered_provider(&arguments));
        }
        arguments
    })
    .and_then(|mut arguments| {
        // Before any window (windows-instalador): on Windows, the ID its
        // toasts and Start menu shortcut carry.
        rune_platform::notifications::claim_application_id();
        // Desktop (P6): one RUNE per person. A later start brings the first
        // to the front with its links, and leaves; captures and the
        // benchmark always run on their own. Before the update's check
        // (actualizaciones), so a later start never waits for it.
        #[cfg(any(target_os = "linux", windows))]
        if arguments.screenshot.is_none() && !arguments.bench {
            match rune_platform::single_instance::claim(&arguments.links) {
                rune_platform::single_instance::Launch::First(instance) => {
                    arguments.instance = Some(instance);
                }
                rune_platform::single_instance::Launch::Forwarded => {
                    eprintln!("rune: {}", t("RUNE ya estaba abierto: se trajo al frente."));
                    std::process::exit(0);
                }
                rune_platform::single_instance::Launch::Alone => {}
            }
        }
        // Keep startup/login failures even before the main window exists;
        // captures, benchmarks and forwarded launches leave the log alone.
        #[cfg(any(unix, windows))]
        {
            logging = (arguments.screenshot.is_none() && !arguments.bench)
                .then(log::start)
                .flatten();
        }
        // RUNE's own updates (actualizaciones): before a login or a window
        // opens; a mandatory one opens its own window instead.
        arguments.updates = update::startup(arguments.screenshot.is_some(), arguments.bench);
        // `--language` reaches that window too; the settings are not read yet.
        if let Some(language) = arguments.language {
            rune_i18n::set(language);
        }
        if let Some(result) = update::mandatory(&arguments.updates, arguments.screenshot.clone()) {
            if let Err(error) = result {
                eprintln!("rune: {error}");
                #[cfg(any(unix, windows))]
                drop(logging.take());
                std::process::exit(1);
            }
            #[cfg(any(unix, windows))]
            drop(logging.take());
            std::process::exit(0);
        }
        selected_provider(&arguments).map(|provider| (arguments, provider))
    });
    let (mut arguments, (provider, mock)) = configured.unwrap_or_else(|error| {
        eprintln!("rune: {error}");
        #[cfg(any(unix, windows))]
        drop(logging.take());
        std::process::exit(2);
    });
    // Desktop session and games (escritorio-sesion-y-juegos): started by the
    // session in the tray, the window waits unseen (X11; Wayland maps it
    // anyway, and the tray hides it once it shows).
    arguments.start_minimized &= arguments.screenshot.is_none() && !arguments.bench;
    // Own themes (temas-propios): the desktop shows through the window only
    // if it opens able to; captures and the benchmark stay opaque.
    let transparency = if arguments.screenshot.is_none() && !arguments.bench {
        window::transparency()
    } else {
        0
    };
    // The window's own title bar (barra-e-icono): as the person chose.
    let title_bar = arguments.title_bar.unwrap_or_else(frame::chosen);
    let chrome = frame::Chrome::current(title_bar);
    // The opening screen's splash: a small tile in the middle of the screen
    // until the session says more than its stage, then the usual window
    // (`WindowFrame::grow`). Captures, the benchmark and a start in the
    // tray open at the usual size.
    let compact = arguments.screenshot.is_none() && !arguments.bench && !arguments.start_minimized;
    let (size, min_size) = if compact {
        (frame::SPLASH_SIZE, frame::SPLASH_SIZE.into())
    } else {
        (frame::SIZE, chrome.min_size())
    };
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("RUNE")
            .with_app_id("rune")
            .with_icon(frame::icon())
            .with_inner_size(size)
            .with_min_inner_size(min_size)
            .with_decorations(!compact && chrome == frame::Chrome::System)
            .with_resizable(!compact)
            .with_transparent(transparency > 0)
            .with_visible(!arguments.start_minimized),
        centered: true,
        ..Default::default()
    };
    let result = eframe::run_native(
        "RUNE",
        options,
        Box::new(move |creation| {
            limn_egui::install_fonts(&creation.egui_ctx);
            limn_egui::set_color_emoji(Some(rune_components::twemoji::picture));
            limn_egui::set_icon_pack(rune_components::glyphs::pack);
            let mut app = App::new(&creation.egui_ctx, started, arguments, provider, mock);
            // Linux desktop (escritorio-linux).
            let mut graphics = graphics;
            graphics.drawing = creation.wgpu_render_state.as_ref().map(|render| {
                let adapter = render.adapter.get_info();
                format!("{} ({:?})", adapter.name, adapter.backend)
            });
            app.attach_window(window::Parent::of(creation), graphics, title_bar);
            app.frame = frame::WindowFrame::new(chrome, compact);
            rune_ui::themes::window_known(&mut app.ui, transparency, transparency > 0);
            Ok(Box::new(app))
        }),
    );
    // The GPU's video pipelines end on threads of their own: let them, so
    // their drivers are not torn down under them as the process exits.
    #[cfg(target_os = "linux")]
    rune_media::video::gpu::settle(std::time::Duration::from_millis(500));
    // RUNE's own updates (actualizaciones): the version just installed,
    // which keeps the log itself.
    if let Some(installed) = update::installed() {
        #[cfg(any(unix, windows))]
        drop(logging);
        update::restart(installed);
    }
    match result {
        // The chosen GPU's driver was not there after all: start again with
        // every GPU, as if RUNE had not chosen, and the settings say so.
        Err(error) if limited => {
            eprintln!(
                "rune: could not open the window on the chosen GPU ({error}); trying every GPU"
            );
            // The new start keeps the log itself, on the terminal's stderr.
            #[cfg(any(unix, windows))]
            drop(logging);
            let again = std::env::current_exe().and_then(|program| {
                std::process::Command::new(program)
                    .args(std::env::args_os().skip(1))
                    .env_remove("VK_LOADER_DRIVERS_SELECT")
                    .env("RUNE_GPU", GPU_FALLBACK)
                    .status()
            });
            std::process::exit(again.map_or(1, |status| status.code().unwrap_or(1)))
        }
        other => other,
    }
}

/// Where each account's key for the local store lives: the system keyring
/// (owner decision 2026-10-18). Tests never reach the person's.
fn keyring() -> Option<Arc<dyn rune_platform::secrets::SecretStore>> {
    (!cfg!(test)).then(rune_platform::secrets::system)
}

/// Where the local store lives: the platform's data directory.
fn data_path() -> Option<PathBuf> {
    Some(rune_platform::folders::data()?.join("rune.db"))
}

/// `event` with its wheel or trackpad movement scaled by the person's speed
/// (imagenes-y-lectura), before the tree scrolls with it.
fn scaled_scroll(event: UiEvent, factor: f32) -> UiEvent {
    match event {
        UiEvent::Scroll { position, delta_y } => UiEvent::Scroll {
            position,
            delta_y: delta_y * factor,
        },
        UiEvent::ScrollDirect { position, delta_y } => UiEvent::ScrollDirect {
            position,
            delta_y: delta_y * factor,
        },
        UiEvent::Scroll2D {
            position,
            delta_x,
            delta_y,
            direct,
            shift,
        } => UiEvent::Scroll2D {
            position,
            delta_x: delta_x * factor,
            delta_y: delta_y * factor,
            direct,
            shift,
        },
        other => other,
    }
}

/// Where downloaded pictures are kept between runs: the platform's cache
/// directory (imagenes-y-lectura).
fn cache_path() -> Option<PathBuf> {
    rune_platform::folders::cache()
}

/// The budgets and retention of the pictures on disk the person chose.
fn image_cache_limits(prefs: &rune_ui::Prefs) -> rune_core::ImageCacheLimits {
    rune_core::ImageCacheLimits {
        media_bytes: prefs.cache_bytes(rune_ui::reading::CachePart::Media),
        artwork_bytes: prefs.cache_bytes(rune_ui::reading::CachePart::Artwork),
        retention: Duration::from_secs(u64::from(prefs.cache_days) * 24 * 60 * 60),
    }
}

/// A prepared picture as limn draws it.
fn preview(data: &rune_core::ImageData, name: &str) -> Option<limn::RasterImage> {
    limn::RasterImage::new(
        data.sha256.clone(),
        limn::RasterImageFormat::Png,
        data.size_bytes,
        data.base64.clone(),
        name,
    )
}

/// A profile picture just chosen, whole, for the editor to frame it
/// (edicion-de-perfil).
fn picture_source(source: &rune_core::PictureSource) -> Option<rune_ui::PictureSource> {
    Some(rune_ui::PictureSource {
        image: preview(&source.preview, t("Imagen elegida"))?,
        width: source.preview.width,
        height: source.preview.height,
        animated: source.animated,
    })
}

/// The newest message of `channel` a capture's `last:{what}` acts on: one
/// with reactions, the account's own poll, or a plain one (P4).
fn last_message(store: &Store, channel: ChannelId, what: &str) -> Option<rune_model::MessageId> {
    let me = store.me();
    store
        .timeline(channel)?
        .messages()
        .iter()
        .rev()
        .find(|message| {
            !message.delivery.is_local()
                && match what {
                    "reactions" => !message.reactions.is_empty(),
                    "voters" | "end-poll" => message.poll.is_some() && Some(message.author) == me,
                    // A line that names a server's emoji among its words.
                    "emoji" => {
                        message.text_references.iter().any(|reference| {
                            matches!(reference.target, rune_model::TextTarget::Emoji { .. })
                        }) && !rune_components::text::only_emoji(
                            &message.content,
                            &message.text_references,
                        )
                    }
                    // A forward, just under the pictures of the simulation's
                    // message content (contenido-de-mensajes).
                    "forward" => message.forwarded.is_some(),
                    // An invitation that names a server's event
                    // (eventos-programados).
                    "event-invitation" => message.text_references.iter().any(|reference| {
                        reference.target == rune_model::TextTarget::Invitation
                            && reference.source.contains("?event=")
                    }),
                    _ => {
                        message.kind == rune_model::MessageKind::Default
                            && message.poll.is_none()
                            && !message.content.is_empty()
                    }
                }
        })
        .map(|message| message.id)
}

/// Native-drop QA records the explicit rejection of unpositioned legacy input.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DropCapture {
    Waiting,
    Rejected,
}

/// A capture of the window asked for with `--screenshot`.
struct Capture {
    path: PathBuf,
    channel: Option<String>,
    actions: Vec<String>,
    draft: Option<String>,
    hover_last_message: bool,
    hover_id: Option<String>,
    /// `--action shift`: Shift held, as limn says it (limn-teclado-y-superficies).
    hold_shift: bool,
    /// Wait for a native drop and capture its rejection after the usual interval.
    file_drop: Option<DropCapture>,
    attachments: bool,
    /// Add [`capture_fixture::viewer`] after the latest message.
    viewer: bool,
    animation: Option<Arc<rune_core::Animation>>,
    invitation: bool,
    /// Invitation cards answer from the fixture, without a provider.
    invitation_cards: bool,
    rich: bool,
    /// Add [`capture_fixture::media`]; with players, play its video once settled.
    media: bool,
    players: bool,
    media_play: bool,
    voice: bool,
    search: Option<String>,
    context: bool,
    fixture: Option<(rune_model::RemoteImage, limn::RasterImage)>,
    /// When the provider was ready and the state set: the capture waits a
    /// moment after it for history and fonts to arrive.
    ready: Option<Instant>,
    requested: bool,
    /// Actions given as `after:{id}`, run in order once the conversation
    /// settled, each waiting a moment after the last (P3).
    later: Vec<String>,
    // Message actions (P4).
    /// Actions that wait for the open conversation's messages.
    deferred: Vec<String>,
    /// When to take an opening screen's picture (`--capture-splash`).
    splash_at: Option<Instant>,
    // Rules and onboarding (2026-10-18).
    /// `--capture-onboarding`: whether the club was joined yet.
    onboarding: Option<bool>,
}

/// Whether the provider's own login window just closed: completed (on to
/// the account's check) or cancelled.
fn login_closed(before: &AuthenticationState, after: &AuthenticationState) -> bool {
    matches!(before, AuthenticationState::AwaitingExternal)
        && !matches!(after, AuthenticationState::AwaitingExternal)
}

/// When the account signed in changed, hand the interface what it kept,
/// before anything opens with the other's places.
fn account_settings(kept: &mut kept::Kept, ui: &mut Ui, store: &Store, client: &Client) {
    let provider = ui.providers.current.unwrap_or_default();
    let legacy = client.storage_account().is_some() && client.storage_account() == store.me();
    if kept.sign_in(provider, store.me(), legacy) {
        ui.restore_account(|key| kept.get(key));
    }
}

/// A capture waits for either conversation data or an authorization page.
fn capture_ready(store: &Store) -> bool {
    !store.servers().is_empty() || !matches!(store.authentication, AuthenticationState::Unavailable)
}

/// The scripted scroll of `--bench`.
struct Bench {
    frames: usize,
    times: Vec<FrameTimes>,
    opened: bool,
}

/// One current value per internal setting key, never a queue of edits.
#[derive(Default)]
struct PendingStorage {
    settings: BTreeMap<String, Option<String>>,
    forget: bool,
    confirming_forget: bool,
    closing: bool,
    closed: bool,
    ticket: Option<StorageTicket>,
    error: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct StorageCompletion {
    close: bool,
    forgot: bool,
}

impl PendingStorage {
    fn set_setting(&mut self, key: String, value: Option<String>) {
        self.settings.insert(key, value);
        if self.closing {
            self.ticket = None;
        }
    }

    fn begin_close(
        &mut self,
        settings: impl IntoIterator<Item = (String, Option<String>)>,
    ) -> bool {
        if self.closing || self.closed {
            return false;
        }
        self.closing = true;
        self.ticket = None;
        self.settings.extend(settings);
        true
    }

    fn cancel_close(&mut self) {
        self.closing = false;
        self.ticket = None;
    }

    fn submit_settings(
        &mut self,
        mut submit: impl FnMut(&str, Option<String>) -> Result<(), StorageWriteError>,
    ) -> Result<(), StorageWriteError> {
        while let Some((key, value)) = self.settings.first_key_value() {
            let key = key.clone();
            submit(&key, value.clone())?;
            self.settings.remove(&key);
        }
        Ok(())
    }

    fn submit_forget(
        &mut self,
        mut submit: impl FnMut() -> Result<(), StorageWriteError>,
    ) -> Result<(), StorageWriteError> {
        if self.forget {
            submit()?;
            self.forget = false;
            self.confirming_forget = true;
            self.ticket = None;
        }
        Ok(())
    }

    fn advance_barrier(
        &mut self,
        dirty_drafts: bool,
        mut flush: impl FnMut() -> Result<Option<StorageTicket>, StorageWriteError>,
        committed: bool,
    ) -> Result<StorageCompletion, StorageWriteError> {
        if self.closed
            || (!self.closing && !self.confirming_forget)
            || dirty_drafts
            || !self.settings.is_empty()
            || self.forget
        {
            return Ok(StorageCompletion::default());
        }
        if self.ticket.is_some() {
            if !committed {
                return Ok(StorageCompletion::default());
            }
        } else if let Some(ticket) = flush()? {
            self.ticket = Some(ticket);
            return Ok(StorageCompletion::default());
        }
        let completed = StorageCompletion {
            close: self.closing,
            forgot: self.confirming_forget,
        };
        self.closed = completed.close;
        self.confirming_forget = false;
        self.ticket = None;
        Ok(completed)
    }

    fn waiting(&self) -> bool {
        !self.settings.is_empty()
            || self.forget
            || self.confirming_forget
            || (self.closing && !self.closed)
    }
}

fn keep_drafts(
    ui: &mut Ui,
    mut submit: impl FnMut(ChannelId, Option<String>) -> Result<(), StorageWriteError>,
) -> Result<(), StorageWriteError> {
    loop {
        let draft = ui
            .dirty_drafts()
            .next()
            .map(|(channel, revision, text)| (channel, revision, text.map(str::to_owned)));
        let Some((channel, revision, text)) = draft else {
            return Ok(());
        };
        submit(channel, text)?;
        ui.acknowledge_draft(channel, revision);
    }
}

fn keep_read_marks(
    store: &mut Store,
    mut submit: impl FnMut(
        rune_model::UserId,
        ChannelId,
        rune_model::ReadMark,
    ) -> Result<(), StorageWriteError>,
) -> Result<(), StorageWriteError> {
    let Some(account) = store.me() else {
        return Ok(());
    };
    for (channel, mark) in store.dirty_read_marks() {
        submit(account, channel, mark)?;
        store.read_mark_saved(channel, mark);
    }
    Ok(())
}

struct App {
    tree: UiTree,
    ui: Ui,
    store: Store,
    client: Client,
    mock: Option<Arc<MockControl>>,
    fonts_attached: bool,
    /// Focus asked for; given after the next layout, which holds the control.
    focus: Option<(String, Option<usize>)>,
    audio: audio::Audio,
    /// The desktop's push-to-talk session; dropping the app closes it.
    global_ptt: ptt::GlobalPushToTalk,
    /// Keys first pressed in the window since the voice toggles last read
    /// them, with the modifiers held (limn-teclado-y-superficies).
    pressed: Vec<(Key, limn::Modifiers)>,
    /// Notifications of the system for new messages.
    alerts: alerts::Alerts,
    /// The tray, later starts, the theme, the font and this device's
    /// activity (P6).
    desktop: desktop::Desktop,
    /// A Ctrl+V of copied files' names, held while Core reads the clipboard.
    file_paste: Option<paste::FilePaste>,
    animations: animations::Animations,
    /// The screen and camera previews, released with the window.
    video: video::Video,
    /// The attachment whose video pictures are on a surface.
    clip_surface: Option<rune_model::AttachmentId>,
    /// The zoom last given to egui.
    zoom: f32,
    drafts_kept: Instant,
    pending_storage: PendingStorage,
    /// The settings kept on this device, the device's and each account's.
    kept: kept::Kept,
    /// Bounded native metadata rejection; never retains dropped paths.
    drop_rejection: Option<uploads::NativeDropRejection>,
    read_marks_to_restore: Option<(rune_model::UserId, Vec<(ChannelId, rune_model::ReadMark)>)>,
    devices: Option<mpsc::Receiver<Result<rune_media::Devices, String>>>,
    /// The public application ID for the OAuth2 provider, to start it later.
    discord_client_id: Option<String>,
    /// This run may keep data on this device: not a benchmark, nor a
    /// capture without `--data`. A simulation started later uses the store.
    local_data: bool,
    metrics: Metrics,
    title: String,
    bench: Option<Bench>,
    capture: Option<Capture>,
    // Linux desktop (escritorio-linux).
    /// The window as the parent of the portal's dialogs.
    parent: window::Parent,
    // RUNE's own updates (actualizaciones).
    updates: update::Updates,
    // The window's own title bar (barra-e-icono).
    frame: frame::WindowFrame,
}

impl App {
    fn new(
        ctx: &egui::Context,
        started: Instant,
        mut arguments: Arguments,
        provider: Arc<dyn Provider>,
        mock: Option<Arc<MockControl>>,
    ) -> Self {
        let bench = arguments.bench;
        limn_egui::set_image_cache_entries(rune_core::IMAGE_TEXTURE_ENTRIES);
        let simulated = mock.is_some();
        let wake_ctx = ctx.clone();
        let local = !bench && (arguments.screenshot.is_none() || arguments.data);
        let mut client = Client::start(
            provider.clone(),
            ClientConfig {
                wake: Arc::new(move || wake_ctx.request_repaint()),
                // With any provider: one without an identity keeps settings only.
                storage: local.then(data_path).flatten(),
                keyring: keyring(),
                backoff: Backoff::default(),
                deadlines: Deadlines::default(),
                image_cache: local.then(cache_path).flatten(),
            },
        );
        // The startup reads below want the key of an account named from the
        // start; a change of provider gets it later, through `poll`.
        client.wait_for_key();
        // A service-owned application key is independent of the chat account.
        // Captures never inherit it or query an external expression service.
        if arguments.screenshot.is_none() && !bench {
            let config = std::env::var("GIPHY_API_KEY")
                .ok()
                .and_then(|key| rune_core::GiphyConfig::new(key).ok());
            client.configure_gifs(config);
        }
        let mut store = Store::new();
        store.provider = Some(provider.info());
        let mut ui = Ui::new();
        // Tests write Spanish, whatever the machine's session speaks.
        ui.desktop.system_language = if cfg!(test) {
            rune_i18n::Language::Spanish
        } else {
            rune_i18n::Language::system()
        };
        ui.providers = rune_ui::Providers {
            options: ProviderChoice::ALL
                .into_iter()
                .map(|choice| choice.option(arguments.discord_client_id.as_deref()))
                .collect(),
            current: Some(arguments.provider.key()),
            ..Default::default()
        };
        let mut startup = Vec::new();
        let mut read_marks_to_restore = client.storage_account().and_then(|account| {
            match client.read_marks() {
                Ok(marks) => Some((account, marks)),
                Err(error) => {
                    ui.notify(t("No se pudieron restaurar las lecturas"), Some(tr!("{error} Las nuevas marcas quedarán sólo en memoria hasta reiniciar o borrar los datos locales.", error = error.label())), true, Instant::now());
                    None
                }
            }
        });
        let mut kept = kept::Kept::new(client.settings());
        if arguments.screenshot.is_none() || arguments.data {
            ui.restore(|key| kept.get(key));
        }
        if arguments.language.is_some() {
            ui.prefs.language = arguments.language;
        }
        rune_i18n::set(ui.language());
        // The key came above: with the person's budgets the disk of its
        // pictures opens now, before the first frame asks for any.
        client.set_image_cache_limits(image_cache_limits(&ui.prefs));
        if simulated && (arguments.screenshot.is_none() || arguments.data) {
            ui.drafts.extend(client.drafts());
            // Their files come back as each conversation opens (envios-y-subidas).
            client.load_draft_files();
            // The last session's servers and channels, at once, while this one starts.
            if let Some(snapshot) = client.cached_snapshot() {
                let applied = store.apply_cached_snapshot(snapshot);
                account_settings(&mut kept, &mut ui, &store, &client);
                if let Some((account, marks)) = read_marks_to_restore.take() {
                    store.restore_read_marks(account, marks);
                }
                startup = rune_ui::on_applied(&mut ui, &mut store, &applied, Instant::now());
            }
        }
        if let Some(error) = client.storage_error() {
            // An opening error can include SQLite data: log only the category.
            eprintln!("rune: local storage notice: unavailable at startup");
            ui.notify(
                t("Sin almacenamiento local"),
                Some(error.to_owned()),
                true,
                Instant::now(),
            );
        }
        let mut tree = UiTree::new(Element::spacer());
        tree.set_reduced_motion(
            ui.reduced_motion || limn_egui::prefers_reduced_motion() == Some(true),
        );
        // RUNE's own updates (actualizaciones).
        let updates = update::Updates::new(
            std::mem::take(&mut arguments.updates),
            arguments.screenshot.is_none() && !bench && !cfg!(test),
            &mut ui,
            {
                let wake = ctx.clone();
                Arc::new(move || wake.request_repaint())
            },
        );
        let mut app = Self {
            tree,
            ui,
            store,
            client,
            mock,
            fonts_attached: false,
            focus: None,
            // Only a provider that carries audio hands its link over.
            audio: audio::Audio::with_link(provider.take_voice_link()),
            global_ptt: ptt::GlobalPushToTalk::default(),
            pressed: Vec::new(),
            alerts: alerts::Alerts::new({
                let wake = ctx.clone();
                Arc::new(move || wake.request_repaint())
            }),
            desktop: desktop::Desktop::new(
                ctx,
                arguments.screenshot.is_none() && !bench,
                arguments.instance.take(),
                std::mem::take(&mut arguments.links),
                arguments.start_minimized,
            ),
            animations: animations::Animations::default(),
            file_paste: None,
            video: video::Video::with_link(provider.take_video_link()),
            clip_surface: None,
            zoom: 1.0,
            drafts_kept: Instant::now(),
            pending_storage: PendingStorage::default(),
            kept,
            drop_rejection: None,
            read_marks_to_restore,
            devices: None,
            discord_client_id: arguments.discord_client_id,
            local_data: local,
            metrics: Metrics::new(started),
            title: String::new(),
            bench: bench.then_some(Bench {
                frames: 0,
                times: Vec::new(),
                opened: false,
            }),
            capture: arguments.screenshot.map(|path| Capture {
                path,
                channel: arguments.channel,
                // Read before `actions` takes them.
                hold_shift: arguments.actions.iter().any(|action| action == "shift"),
                actions: arguments.actions,
                draft: arguments.capture_draft,
                hover_last_message: arguments.capture_hover,
                hover_id: arguments.capture_hover_id,
                file_drop: arguments.capture_drop.then_some(DropCapture::Waiting),
                attachments: arguments.capture_attachments,
                viewer: arguments.capture_viewer,
                animation: (arguments.capture_animation || arguments.capture_media)
                    .then(capture_fixture::animation),
                invitation: arguments.capture_invitation,
                invitation_cards: arguments.capture_invitation,
                rich: arguments.capture_rich,
                media: arguments.capture_media || arguments.capture_player,
                players: arguments.capture_player,
                media_play: arguments.capture_player,
                voice: arguments.capture_voice,
                search: arguments.capture_search,
                context: arguments.capture_context,
                fixture: None,
                ready: None,
                requested: false,
                later: Vec::new(),
                deferred: Vec::new(),
                splash_at: arguments.capture_splash.map(|after| started + after),
                onboarding: arguments.capture_onboarding.then_some(false),
            }),
            parent: window::Parent::default(),
            updates,
            frame: frame::WindowFrame::default(),
        };
        app.run(ctx, startup);
        app
    }

    // Linux desktop (escritorio-linux).
    /// Captures, the benchmark and tests leave the person's window settings
    /// alone.
    fn keeps_window_settings(&self) -> bool {
        self.capture.is_none() && self.bench.is_none() && !cfg!(test)
    }

    /// The window eframe opened, and what was read before it: who draws
    /// its title bar and the GPUs it can draw with.
    fn attach_window(
        &mut self,
        parent: window::Parent,
        graphics: rune_ui::desktop::Graphics,
        title_bar: rune_ui::title_bar::TitleBar,
    ) {
        self.parent = parent;
        rune_ui::desktop::window_known(&mut self.ui, title_bar, graphics);
        // Desktop session and games (escritorio-sesion-y-juegos).
        if self.keeps_window_settings() {
            rune_ui::desktop::proxy_known(&mut self.ui, remembered_proxy());
        }
    }

    /// Carry out what the interface asked for.
    fn run(&mut self, ctx: &egui::Context, intents: Vec<Intent>) {
        for intent in intents {
            match intent {
                Intent::Command(command) => {
                    // Captures must not open a credential-bearing login window
                    // nor enter a remembered account.
                    if self.capture.is_some()
                        && matches!(
                            command,
                            rune_provider::Command::BeginSignIn
                                | rune_provider::Command::UseAccount(_)
                        )
                        && self.store.provider.as_ref().is_some_and(|provider| {
                            provider.kind == rune_provider::ProviderKind::Unofficial
                        })
                    {
                        continue;
                    }
                    let rejected = match &command {
                        rune_provider::Command::LoadContext {
                            request,
                            channel,
                            anchor,
                            ..
                        } => {
                            let permit = if self.ui.channel() == Some(*channel) {
                                self.store.begin_context(
                                    *channel,
                                    *request,
                                    *anchor,
                                    rune_model::HistorySource::Provider,
                                )
                            } else {
                                rune_state::Permit::Denied(
                                    t("La conversación seleccionada cambió.").into(),
                                )
                            };
                            permit.reason().map(|reason| rune_state::Applied::Failed {
                                failure: rune_provider::Failure::Context {
                                    request: *request,
                                    channel: *channel,
                                },
                                reason: reason.to_owned(),
                                ambiguous: false,
                            })
                        }
                        rune_provider::Command::LoadPins { request, channel } => {
                            let permit = self.store.begin_pins(*channel, *request);
                            permit.reason().map(|reason| rune_state::Applied::Failed {
                                failure: rune_provider::Failure::Pins {
                                    request: *request,
                                    channel: *channel,
                                },
                                reason: reason.to_owned(),
                                ambiguous: false,
                            })
                        }
                        rune_provider::Command::Cancel(request) => {
                            // Local cancellation must not depend on queue admission.
                            self.store.cancel_pins(*request);
                            self.store.cancel_context(*request);
                            None
                        }
                        _ => None,
                    };
                    if let Some(applied) = rejected {
                        let now = Instant::now();
                        // This read was never admitted. Provider responses still
                        // require an active Store request; a local denial does not.
                        let intents =
                            rune_ui::on_applied(&mut self.ui, &mut self.store, &applied, now);
                        self.run(ctx, intents);
                        continue;
                    }
                    self.client.command(command);
                }
                Intent::LoadLatest(channel) => self.client.load_latest(channel),
                Intent::SearchLocal {
                    request,
                    query,
                    cursor,
                } => {
                    let channel = query.channel;
                    if self.ui.channel() == Some(channel)
                        && self
                            .store
                            .abilities(channel)
                            .is_some_and(|abilities| abilities.read_history.allowed())
                        && self.client.local_history_available()
                    {
                        self.client.search_local(request, query, cursor);
                    } else {
                        let intents = rune_ui::local_search_result(
                            &mut self.ui,
                            &self.store,
                            request,
                            channel,
                            Err(rune_model::LocalReadError::Unavailable),
                            Instant::now(),
                        );
                        self.run(ctx, intents);
                    }
                }
                Intent::LoadContext {
                    request,
                    channel,
                    anchor,
                    source,
                } => {
                    if source == rune_model::HistorySource::Provider {
                        self.run(
                            ctx,
                            vec![Intent::Command(rune_provider::Command::LoadContext {
                                request,
                                channel,
                                anchor,
                                limit: rune_core::PAGE + 1,
                            })],
                        );
                    } else {
                        let permit = if self.ui.channel() == Some(channel) {
                            self.store.begin_context(channel, request, anchor, source)
                        } else {
                            rune_state::Permit::Denied(
                                t("La conversación seleccionada cambió.").into(),
                            )
                        };
                        if self.ui.channel() == Some(channel)
                            && permit.allowed()
                            && self.client.local_history_available()
                        {
                            self.client.load_cached_context(request, channel, anchor);
                        } else {
                            let applied = rune_state::Applied::Failed {
                                failure: rune_provider::Failure::Context { request, channel },
                                reason: permit
                                    .reason()
                                    .unwrap_or(rune_model::LocalReadError::Unavailable.label())
                                    .to_owned(),
                                ambiguous: false,
                            };
                            self.store.cancel_context(request);
                            let intents = rune_ui::on_applied(
                                &mut self.ui,
                                &mut self.store,
                                &applied,
                                Instant::now(),
                            );
                            self.run(ctx, intents);
                        }
                    }
                }
                Intent::LoadOlder(channel, before) => self.client.load_older(channel, before),
                Intent::CancelHistoryExcept(channel) => self.client.cancel_history_except(channel),
                Intent::Send {
                    channel,
                    content,
                    reply_to,
                    mention_reply,
                } => {
                    let nonce =
                        self.client
                            .send_message(channel, content.clone(), reply_to, mention_reply);
                    self.store
                        .add_pending(channel, content, reply_to, nonce, wall_clock());
                    if !mention_reply {
                        self.store.quiet_reply(nonce);
                    }
                }
                Intent::SendSticker { channel, sticker } => {
                    let nonce = self.client.send_sticker(channel, sticker.id);
                    self.store
                        .add_pending_sticker(channel, sticker, nonce, wall_clock());
                }
                Intent::PickAttachment { request, channel } => {
                    self.pick_attachment(ctx, request, channel);
                }
                Intent::PasteAttachments { request, channel } => {
                    self.paste_attachments(ctx, request, channel);
                }
                Intent::CancelUpload { request } => self.client.cancel_upload(request),
                Intent::RemoveAttachment { channel: _, token } => {
                    self.client.discard_attachment(token);
                }
                Intent::SendAttachments {
                    request,
                    channel,
                    content,
                    reply_to,
                    files,
                    mention_reply,
                } => self.send_attachments(
                    ctx,
                    request,
                    channel,
                    content,
                    reply_to,
                    mention_reply,
                    files,
                ),
                Intent::RetryAttachmentSend {
                    request,
                    channel,
                    message,
                } => self.retry_attachment_send(ctx, request, channel, message),
                Intent::SetInput { id, value } => {
                    let end = value.chars().count();
                    self.tree.set_input_value(&id, value);
                    self.tree.set_input_cursor(&id, end);
                }
                Intent::Focus(id) => self.focus = Some((id, None)),
                Intent::FocusInput { id, cursor } => {
                    // An existing composer must receive the next event in this
                    // same frame. Only newly-created fields defer their focus.
                    let focused = self.tree.focus(&id);
                    self.tree.set_input_cursor(&id, cursor);
                    self.focus = (!focused).then_some((id, Some(cursor)));
                }
                // Message actions (P4): a selection kept after formatting.
                Intent::SelectInput { id, start, end } => {
                    let focused = self.tree.focus(&id);
                    self.tree.set_input_selection(&id, start, end);
                    self.focus = (!focused).then_some((id, Some(end)));
                }
                // Desktop (P6).
                Intent::SetAutostart { on, minimized } => self.desktop.set_autostart(on, minimized),
                Intent::ListFonts => self.desktop.list_fonts(),
                // Desktop session and games (escritorio-sesion-y-juegos).
                Intent::ListPrograms => self.desktop.list_programs(),
                Intent::SetProxy(proxy) => {
                    if self.keeps_window_settings() {
                        let kept =
                            (proxy != rune_provider::ApiProxy::Automatic).then(|| proxy.kept());
                        window::remember(PROXY, kept);
                    }
                }
                // Pictures on disk (imagenes-y-lectura).
                Intent::ClearImageCache(part) => self.client.clear_image_cache(match part {
                    rune_ui::reading::CachePart::Media => rune_core::ImageCachePart::Media,
                    rune_ui::reading::CachePart::Artwork => rune_core::ImageCachePart::Artwork,
                }),
                // The window's own title bar (barra-e-icono).
                Intent::TitleBar(choice) => {
                    self.frame.chrome = frame::Chrome::current(choice);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(
                        self.frame.chrome == frame::Chrome::System,
                    ));
                    ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(
                        self.frame.chrome.min_size(),
                    ));
                    if self.keeps_window_settings() {
                        frame::keep(choice);
                    }
                }
                Intent::Window(action) => ctx.send_viewport_cmd(frame::command(action)),
                Intent::DrawWith(gpu) => {
                    if self.keeps_window_settings() {
                        window::remember(GPU, gpu);
                    }
                }
                Intent::ReplaceSavedFile { request } => {
                    // Nothing waits for the answer (a capture's save never
                    // started in Core): the bar must not stay «Guardando».
                    if !self.client.replace_save(request) {
                        rune_ui::save_finished(
                            &mut self.ui,
                            request,
                            Err(Some(
                                t("No se pudo reemplazar el archivo en esa carpeta.").into(),
                            )),
                        );
                    }
                }
                // Own themes (temas-propios); a capture opens no system dialog.
                Intent::ImportTheme { request } => {
                    if self.capture.is_some() {
                        let intents = rune_ui::themes::imported(
                            &mut self.ui,
                            request,
                            Err(
                                t("El selector de archivos no está disponible en este equipo.")
                                    .into(),
                            ),
                            Instant::now(),
                        );
                        self.run(ctx, intents);
                    } else {
                        self.client.import_theme(request, self.parent.get());
                    }
                }
                Intent::ExportTheme {
                    request,
                    filename,
                    contents,
                } => {
                    let started = match self.capture {
                        Some(_) => Err(t(
                            "El selector de archivos no está disponible en este equipo.",
                        )),
                        None => self.client.export_theme(
                            request,
                            &filename,
                            contents,
                            self.parent.get(),
                        ),
                    };
                    if let Err(reason) = started {
                        rune_ui::themes::exported(
                            &mut self.ui,
                            request,
                            Err(reason.into()),
                            Instant::now(),
                        );
                    }
                }
                Intent::SetTransparency(percent) => {
                    if self.keeps_window_settings() {
                        window::remember(
                            window::TRANSPARENCY,
                            (percent > 0).then(|| percent.to_string()),
                        );
                    }
                }
                // RUNE's own updates (actualizaciones).
                Intent::CheckForUpdates => self.updates.check(&mut self.ui),
                Intent::InstallUpdate => self.updates.install(&mut self.ui),
                Intent::CheckUpdatesAtStart(on) => {
                    if self.keeps_window_settings() {
                        update::set_at_start(on);
                    }
                }
                Intent::CancelSoundFolder => self.client.cancel_folder(),
                Intent::ChooseSoundFolder { request } => {
                    // A capture opens no system dialog.
                    let started = match self.capture {
                        Some(_) => Err(t(
                            "El selector de archivos no está disponible en este equipo.",
                        )),
                        None => self.client.choose_folder(request, self.parent.get()),
                    };
                    if let Err(reason) = started {
                        let intents =
                            rune_ui::sound_folder_chosen(&mut self.ui, request, Err(reason.into()));
                        self.run(ctx, intents);
                    }
                }
                Intent::ScrollToEnd(id) => self.tree.scroll_to_end(&id),
                Intent::ScrollToIndex { list, index } => self.tree.scroll_to_index(&list, index),
                Intent::SetScrollOffset { list, offset } => {
                    self.tree.set_scroll_offset(&list, offset)
                }
                Intent::Copy(text) => ctx.copy_text(text),
                Intent::OpenUrl(url) => {
                    if self.capture.is_none() {
                        if let Some(target) = self.client.invitation_target(&url) {
                            let intents =
                                rune_ui::open_invitation(&mut self.ui, &self.store, target);
                            self.run(ctx, intents);
                        } else {
                            rune_platform::open::web(&url);
                        }
                    }
                }
                Intent::OpenInBrowser(url) => {
                    if self.capture.is_none() {
                        rune_platform::open::web(&url);
                    }
                }
                Intent::InspectInvitation(input) => {
                    if self.capture.is_none() {
                        let target = self.client.invitation_target(&input);
                        let intents = rune_ui::invitation_input_result(
                            &mut self.ui,
                            &self.store,
                            &input,
                            target,
                        );
                        self.run(ctx, intents);
                    }
                }
                Intent::ReviewInvitation {
                    channel,
                    message,
                    account,
                    source,
                } => {
                    let current = self.store.me() == Some(account)
                        && self.store.invitation_preview().allowed()
                        && self.store.channel(channel).is_some_and(Store::visible)
                        && self
                            .store
                            .timeline(channel)
                            .and_then(|timeline| timeline.get(message))
                            .is_some_and(|message| {
                                rune_components::message::invitation_links(message)
                                    .contains(&source.as_str())
                            });
                    if self.capture.is_none() && current {
                        if let Some(target) = self.client.invitation_target(&source) {
                            let intents =
                                rune_ui::open_invitation(&mut self.ui, &self.store, target);
                            self.run(ctx, intents);
                        } else {
                            self.ui.notify(
                                t("No se pudo revisar la invitación"),
                                Some(t("El enlace ya no es válido para esta conexión.").into()),
                                true,
                                Instant::now(),
                            );
                        }
                    }
                }
                Intent::PreviewInvitation { request, target } => {
                    if self.capture.is_none() {
                        self.client.preview_invitation(request, target);
                    }
                }
                Intent::CancelInvitation(request) => self.client.cancel_invitation(request),
                Intent::SearchGifs {
                    request,
                    query,
                    offset,
                } => {
                    if self.capture.is_none() {
                        self.client.search_gifs(request, query, offset);
                    }
                }
                Intent::CancelGifSearch { request } => self.client.cancel_gifs(request),
                Intent::PreviewDocument {
                    request,
                    source,
                    size,
                } => {
                    // A capture never downloads: it pages through its local picture.
                    let fixture = self
                        .capture
                        .as_ref()
                        .and_then(|capture| capture.fixture.as_ref())
                        .map(|(_, image)| image.clone());
                    if let Some(image) = fixture {
                        let intents = rune_ui::document_result(
                            &mut self.ui,
                            &self.store,
                            request,
                            1,
                            3,
                            Vec::new(),
                            Ok(image),
                        );
                        self.run(ctx, intents);
                    } else if self.capture.is_none() {
                        if let Err(error) = self.client.preview_document(request, source, size) {
                            let intents = rune_ui::document_result(
                                &mut self.ui,
                                &self.store,
                                request,
                                1,
                                1,
                                Vec::new(),
                                Err(error.label().into()),
                            );
                            self.run(ctx, intents);
                        }
                    }
                }
                Intent::DocumentPage {
                    request,
                    page,
                    marks,
                } => {
                    // A capture highlights its own page picture: nothing is rendered.
                    if self.capture.is_some() && !marks.is_empty() {
                        let image = capture_fixture::document_page(&marks)
                            .ok_or_else(|| t("No se pudo mostrar la vista previa.").to_owned());
                        let intents = rune_ui::document_result(
                            &mut self.ui,
                            &self.store,
                            request,
                            page,
                            3,
                            marks,
                            image,
                        );
                        self.run(ctx, intents);
                        continue;
                    }
                    if let Err(error) = self.client.document_page(request, page, marks.clone()) {
                        let pages = self.ui.document_viewer.as_ref().map_or(1, |v| v.pages);
                        let intents = rune_ui::document_result(
                            &mut self.ui,
                            &self.store,
                            request,
                            page,
                            pages,
                            marks,
                            Err(error.label().into()),
                        );
                        self.run(ctx, intents);
                    }
                }
                Intent::CancelDocument { request } => self.client.cancel_document(request),
                Intent::DocumentText { request } => {
                    // A capture reads its own words: nothing is extracted.
                    let read = if self.capture.is_some() {
                        Ok(capture_fixture::document_text())
                    } else {
                        match self.client.document_text(request) {
                            Ok(()) => continue,
                            Err(error) => Err(error.label().to_owned()),
                        }
                    };
                    rune_ui::document_text(&mut self.ui, request, read);
                }
                Intent::PlayVoice {
                    request,
                    source,
                    size,
                    kind,
                } => {
                    // A fetched clip arrives through `poll_voice`.
                    let loaded = match source {
                        Some(source) => {
                            self.client.load_voice(request, source, size).err().map(Err)
                        }
                        // Only the simulation has clips without a file: its
                        // tones and test pattern come with RUNE.
                        None if self.client.info().kind
                            == rune_provider::ProviderKind::Simulated =>
                        {
                            // A WebM plays the sample in that format.
                            let webm = self.ui.voice_player.as_ref().is_some_and(|player| {
                                self.store
                                    .timeline(player.channel)
                                    .and_then(|timeline| timeline.get(player.message))
                                    .and_then(|message| {
                                        message.files().find(|file| file.id == player.attachment)
                                    })
                                    .is_some_and(|file| file.filename.ends_with(".webm"))
                            });
                            let sample = match kind {
                                rune_ui::Played::Voice => audio::SIMULATED_VOICE,
                                rune_ui::Played::Audio => audio::SIMULATED_AUDIO,
                                rune_ui::Played::Video if webm => audio::SIMULATED_WEBM,
                                rune_ui::Played::Video => audio::SIMULATED_VIDEO,
                            };
                            self.audio.load_voice(request, Arc::from(sample));
                            Some(Ok(()))
                        }
                        None => Some(Err(t("Este adjunto no tiene un archivo para reproducir."))),
                    };
                    if let Some(loaded) = loaded {
                        rune_ui::voice_loaded(&mut self.ui, request, loaded.map_err(str::to_owned));
                    }
                }
                Intent::StopVoice { request } => self.client.cancel_voice(request),
                Intent::DownloadFile {
                    source,
                    filename,
                    size_bytes,
                } => {
                    if self.capture.is_none() {
                        match self.client.download_file(source, &filename, size_bytes) {
                            Ok(()) => self.ui.notify(
                                t("Guardando en Descargas…"),
                                Some(filename),
                                false,
                                Instant::now(),
                            ),
                            Err(reason) => self.ui.notify(
                                t("No se pudo descargar"),
                                Some(reason.into()),
                                true,
                                Instant::now(),
                            ),
                        }
                    }
                }
                // Message content (contenido-de-mensajes).
                Intent::CopyFile {
                    source,
                    filename,
                    size_bytes,
                    image,
                } => {
                    if self.capture.is_none() {
                        let copied = if image {
                            rune_core::Copied::Image
                        } else {
                            rune_core::Copied::File
                        };
                        match self.client.copy_file(source, &filename, size_bytes, copied) {
                            Ok(()) => self.ui.notify(
                                t("Copiando…"),
                                Some(filename),
                                false,
                                Instant::now(),
                            ),
                            Err(reason) => self.ui.notify(
                                t("No se pudo copiar"),
                                Some(reason.into()),
                                true,
                                Instant::now(),
                            ),
                        }
                    }
                }
                Intent::SaveAttachmentAs {
                    request,
                    channel,
                    message,
                    attachment,
                } => {
                    let started = match rune_ui::attachment_download(
                        &self.store,
                        channel,
                        message,
                        attachment,
                    ) {
                        // A capture shows the choosing state; no system dialog opens.
                        Some(_) if self.capture.is_some() => Ok(()),
                        // Over the window (escritorio-linux).
                        Some((source, filename, size)) => {
                            let parent = self.parent.get();
                            self.client
                                .save_attachment_as(request, source, &filename, size, parent)
                        }
                        None => Err(t("El archivo ya no está disponible.")),
                    };
                    if let Err(reason) = started {
                        rune_ui::save_finished(&mut self.ui, request, Err(Some(reason.into())));
                    }
                }
                Intent::CancelSaveAttachment { request } => {
                    self.client.cancel_save(request);
                    rune_ui::save_finished(&mut self.ui, request, Err(None));
                }
                Intent::SaveSetting { key, value } => self.keep_setting(&key, value),
                Intent::SimulateDisconnect => {
                    if let Some(mock) = &self.mock {
                        mock.drop_connection();
                    }
                }
                Intent::SimulateSendFailure => {
                    if let Some(mock) = &self.mock {
                        mock.fail_next_send();
                    }
                }
                Intent::SimulateSendSilence => {
                    if let Some(mock) = &self.mock {
                        mock.ignore_next_send();
                    }
                }
                // Private calls.
                Intent::SimulateIncomingCall => {
                    if let Some(mock) = &self.mock {
                        mock.ring_me();
                    }
                }
                Intent::SimulateCallElsewhere => {
                    if let Some(mock) = &self.mock {
                        mock.answer_elsewhere();
                    }
                }
                Intent::ForgetLocalData => {
                    self.pending_storage.forget = true;
                }
                Intent::Resend { channel, message } => {
                    let Some(unsent) = self.store.unsent(channel, message) else {
                        continue;
                    };
                    if !unsent.files.is_empty() {
                        let request = self.ui.fresh_request();
                        self.retry_attachment_send(ctx, request, channel, message);
                        continue;
                    }
                    // The same nonce cannot repeat a message that maybe
                    // arrived, but only if the provider says so.
                    let idempotent = self.client.info().capabilities.idempotent_send;
                    let nonce = if idempotent {
                        unsent.nonce
                    } else {
                        self.client.fresh_nonce()
                    };
                    self.store.resend(channel, message, nonce);
                    if let Some(sticker) = unsent.sticker {
                        self.client.resend_sticker(channel, sticker, nonce);
                    } else {
                        if !unsent.mention_reply {
                            self.store.quiet_reply(nonce);
                        }
                        self.client.resend(
                            channel,
                            unsent.content,
                            unsent.reply_to,
                            nonce,
                            unsent.mention_reply,
                        );
                    }
                }
                Intent::StartMeter(device) => {
                    self.ui.audio.input = device;
                    self.ui.audio.testing = true;
                    self.audio.retry();
                }
                Intent::StopMeter => {
                    self.ui.audio.testing = false;
                    self.ui.audio.loopback = false;
                }
                Intent::RefreshDevices => {
                    // Repeated clicks share the outstanding enumeration.
                    if self.devices.is_some() {
                        continue;
                    }
                    // Asking the audio system can take a moment: off this thread.
                    let (answer, receiver) = mpsc::channel();
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let _ =
                            answer.send(rune_media::devices().map_err(|error| error.to_string()));
                        ctx.request_repaint();
                    });
                    self.devices = Some(receiver);
                }
                Intent::ReducedMotion(reduced) => self.tree.set_reduced_motion(reduced),
                Intent::Reveal { id, margin } => self.tree.reveal_at_start(&id, margin),
                Intent::Repaint => ctx.request_repaint(),
                // Checked again here: the button may be stale by now.
                // A capture answers with a picture made here: no dialog opens.
                Intent::PickProfileImage {
                    request,
                    kind,
                    animated,
                } => match self.capture {
                    Some(_) => {
                        let (picked, source) = match capture_fixture::profile_picture(
                            kind,
                            rune_provider::Crop::default(),
                        ) {
                            Ok(Some((image, preview, source))) => {
                                (Ok(Some((image, preview))), Some(source))
                            }
                            Ok(None) => (Ok(None), None),
                            Err(reason) => (Err(reason), None),
                        };
                        let intents = rune_ui::profile_image_picked(
                            &mut self.ui,
                            request,
                            picked,
                            source,
                            Instant::now(),
                        );
                        self.run(ctx, intents);
                    }
                    None => {
                        let parent = self.parent.get();
                        self.client
                            .pick_profile_image(request, kind, animated, parent)
                    }
                },
                // Profile editing (edicion-de-perfil): a capture frames its own picture.
                Intent::FrameProfileImage {
                    request,
                    kind,
                    crop,
                    animated,
                } => match self.capture {
                    Some(_) => {
                        let picked = capture_fixture::profile_picture(kind, crop)
                            .map(|picked| picked.map(|(image, preview, _)| (image, preview)));
                        let intents = rune_ui::profile_image_picked(
                            &mut self.ui,
                            request,
                            picked,
                            None,
                            Instant::now(),
                        );
                        self.run(ctx, intents);
                    }
                    None => self
                        .client
                        .frame_profile_image(request, kind, crop, animated),
                },
                Intent::CancelProfileImage => self.client.cancel_profile_image(),
                // As a profile's picture: a capture answers with one made here.
                Intent::PickExpressionImage { request, kind } => match self.capture {
                    Some(_) => {
                        let picked = capture_fixture::expression_picture(kind);
                        let intents = rune_ui::expression_image_picked(
                            &mut self.ui,
                            request,
                            picked,
                            Instant::now(),
                        );
                        self.run(ctx, intents);
                    }
                    None => {
                        let parent = self.parent.get();
                        self.client.pick_expression_image(request, kind, parent)
                    }
                },
                Intent::UseProvider(key) => self.switch_provider(ctx, key),
                Intent::CreatePost {
                    request,
                    forum,
                    title,
                    tags,
                    content,
                    files,
                } => self.create_post(request, forum, title, tags, content, files),
                // Apps' modals with files (comandos-de-apps): nothing went,
                // so the modal says why as if its app had refused.
                Intent::InteractWithFiles { command, files } => {
                    let failure = rune_provider::Failure::of(&command);
                    if let (Err(error), Some(failure)) =
                        (self.client.interact_with_files(command, files), failure)
                    {
                        self.store.apply(
                            rune_provider::Event::Failed {
                                failure,
                                reason: error.label().into(),
                                ambiguous: false,
                            },
                            Instant::now(),
                        );
                    }
                }
                Intent::PickGroupIcon { channel } => {
                    if self
                        .store
                        .group_abilities(channel)
                        .is_some_and(|abilities| abilities.icon.allowed())
                    {
                        let parent = self.parent.get();
                        self.client.pick_group_icon(channel, parent);
                    }
                }
            }
        }
        // Apply mute, device and processing changes in the same input frame.
        self.audio
            .step(&mut self.ui, &mut self.store, Instant::now());
    }

    /// Start the provider `key` in place of the one in use: a new client and
    /// a new store, the window's state as at a change of account, and the
    /// choice kept for the next start. The benchmark never switches; a
    /// capture keeps nothing, and its sign-in never opens a login window.
    fn switch_provider(&mut self, ctx: &egui::Context, key: &'static str) {
        let Some(choice) = ProviderChoice::from_key(key) else {
            return;
        };
        if self.bench.is_some()
            || choice
                .unavailable(self.discord_client_id.as_deref())
                .is_some()
        {
            return;
        }
        let started = provider_for(
            choice,
            self.discord_client_id.clone(),
            MockConfig::default(),
            false,
            // The proxy chosen last (escritorio-sesion-y-juegos).
            &self.ui.desktop.proxy.clone(),
        );
        let (provider, mock) = match started {
            Ok(started) => started,
            Err(reason) => {
                self.ui.notify(
                    t("No se pudo cambiar de proveedor"),
                    Some(reason),
                    true,
                    Instant::now(),
                );
                return;
            }
        };
        let wake = ctx.clone();
        // The old client goes with its jobs: uploads, saves, searches, pictures.
        self.client = Client::start(
            provider.clone(),
            ClientConfig {
                wake: Arc::new(move || wake.request_repaint()),
                storage: self.local_data.then(data_path).flatten(),
                keyring: keyring(),
                backoff: Backoff::default(),
                deadlines: Deadlines::default(),
                image_cache: self.local_data.then(cache_path).flatten(),
            },
        );
        // Ready for the key, which comes through `poll`.
        self.client
            .set_image_cache_limits(image_cache_limits(&self.ui.prefs));
        // Decoded for this screen's density (pulido-ui, 2026-10-19).
        self.client.set_image_density(ctx.pixels_per_point());
        self.store = Store::new();
        self.store.provider = Some(provider.info());
        self.mock = mock;
        self.read_marks_to_restore = None;
        // A barrier of the old client's store means nothing to the new one.
        self.pending_storage.ticket = None;
        self.file_paste = None;
        self.devices = None;
        limn_egui::clear_image_cache();
        self.animations.clear();
        self.video.clear();
        self.audio = audio::Audio::with_link(provider.take_voice_link());
        self.video = video::Video::with_link(provider.take_video_link());
        let intents = rune_ui::provider_changed(&mut self.ui, key);
        account_settings(&mut self.kept, &mut self.ui, &self.store, &self.client);
        self.run(ctx, intents);
        if self.local_data {
            remember_provider(choice);
        }
        ctx.request_repaint();
    }

    fn context(&self, window: Size, now: Instant) -> ViewContext {
        let channel = self.ui.channel();
        let rows =
            channel.and_then(|channel| self.tree.visible_rows(&rune_ui::timeline_id(channel)));
        ViewContext {
            window,
            now,
            wall: wall_clock(),
            at_end: channel.is_none_or(|channel| {
                self.tree
                    .is_scroll_at_end(&rune_ui::timeline_id(channel), 48.0)
            }),
            loading_older: channel.is_some_and(|channel| {
                self.client.is_loading(channel) && !self.client.is_loading_latest(channel)
            }),
            loading_latest: channel.is_some_and(|channel| self.client.is_loading_latest(channel)),
            metrics: if self.ui.show_metrics {
                self.metrics.lines(self.store.loaded_messages(), rows)
            } else {
                Vec::new()
            },
        }
    }

    /// What the local store kept of the account the provider named once its
    /// key was in: its drafts, its reading marks and, while its session is
    /// not ready yet, its last snapshot, shown as the cache's until the
    /// session's own arrives.
    fn restored(&mut self, restored: rune_core::Restored) -> rune_state::Applied {
        for (channel, text) in restored.drafts {
            self.ui.drafts.entry(channel).or_insert(text);
        }
        match restored.read_marks {
            Ok(marks) => self.read_marks_to_restore = Some((restored.account, marks)),
            Err(error) => self.ui.notify(
                t("No se pudieron restaurar las lecturas"),
                Some(tr!("{error} Las nuevas marcas quedarán sólo en memoria hasta reiniciar o borrar los datos locales.", error = error.label())),
                true,
                Instant::now(),
            ),
        }
        let applied = restored
            .snapshot
            .map_or(rune_state::Applied::Nothing, |snapshot| {
                self.store.apply_cached_snapshot(snapshot)
            });
        // A session already ready takes the marks now; else with its Ready.
        if self.store.me() == Some(restored.account)
            && !matches!(applied, rune_state::Applied::Ready)
        {
            if let Some((account, marks)) = self.read_marks_to_restore.take() {
                self.store.restore_read_marks(account, marks);
            }
        }
        applied
    }

    /// Apply what the client reported since the last frame.
    fn drain(&mut self, ctx: &egui::Context, now: Instant) {
        rune_ui::set_upload_limits(&mut self.ui, self.client.upload_limits());
        let updates = self.client.poll(EVENTS_PER_FRAME);
        if updates.len() >= EVENTS_PER_FRAME {
            ctx.request_repaint();
        }
        for update in updates {
            let changed_channel = match &update {
                Update::Provider(
                    rune_provider::Event::MessageCreated { message, .. }
                    | rune_provider::Event::MessageUpdated(message),
                ) => Some(message.channel),
                Update::Provider(rune_provider::Event::MessagePatched(patch)) => {
                    Some(patch.channel)
                }
                Update::Provider(
                    rune_provider::Event::MessageDeleted { channel, .. }
                    | rune_provider::Event::HistoryLoaded { channel, .. }
                    | rune_provider::Event::ContextLoaded { channel, .. },
                ) => Some(*channel),
                _ => None,
            };
            // A message just created may alert; captures never do.
            let created = match &update {
                Update::Provider(rune_provider::Event::MessageCreated { message, .. })
                    if self.capture.is_none() =>
                {
                    Some(message.clone())
                }
                _ => None,
            };
            let before = self.store.authentication.clone();
            let applied = match update {
                Update::Provider(event) => {
                    if matches!(
                        &event,
                        rune_provider::Event::Authentication(
                            rune_provider::AuthenticationState::SignedOut
                                | rune_provider::AuthenticationState::Failed { .. }
                        )
                    ) {
                        limn_egui::clear_image_cache();
                        self.animations.clear();
                    }
                    self.store.apply(event, now)
                }
                Update::Cached { channel, messages } => self.store.apply_cached(channel, messages),
                Update::Restored(restored) => self.restored(*restored),
            };
            account_settings(&mut self.kept, &mut self.ui, &self.store, &self.client);
            // The login window closed, done or cancelled: RUNE asks for its own
            // back. X11 gives it; on Wayland only the activation that a
            // request for attention makes can, as the compositor allows.
            if login_closed(&before, &self.store.authentication) && self.capture.is_none() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                    egui::UserAttentionType::Informational,
                ));
            }
            if matches!(applied, rune_state::Applied::Ready) {
                if let Some((account, marks)) = self.read_marks_to_restore.take() {
                    self.store.restore_read_marks(account, marks);
                }
            }
            self.desktop.applied(&applied);
            let mut intents = changed_channel
                .map(|channel| rune_ui::local_messages_changed(&mut self.ui, channel))
                .unwrap_or_default();
            intents.extend(rune_ui::on_applied(
                &mut self.ui,
                &mut self.store,
                &applied,
                now,
            ));
            self.run(ctx, intents);
            self.alerts.account(self.store.me());
            if let Some(message) = created {
                let reading =
                    ctx.input(|input| input.focused) && self.ui.channel() == Some(message.channel);
                let alert =
                    rune_ui::message_alert(&self.ui, &self.store, &message, reading, wall_clock());
                if let Some(alert) = alert {
                    if alert.notify {
                        self.alerts.show(&alert);
                    }
                    if alert.sound {
                        self.audio.play(rune_ui::CallSound::Message);
                    }
                }
            }
        }
        // Private calls: a call that starts ringing the account, while the
        // window is away, is told by the system too; its click opens it,
        // its buttons answer or decline it, and it goes when it stops.
        let (calls, stopped) = rune_ui::call_alerts(&mut self.ui, &self.store);
        if self.capture.is_none() && !ctx.input(|input| input.focused) {
            for alert in &calls {
                self.alerts.show(alert);
            }
        }
        self.alerts.calls_stopped(&stopped);
        if let Some((account, channel, button)) = self.alerts.clicked() {
            let declined = button == Some(rune_ui::CALL_DECLINE);
            let intents = match button {
                None => rune_ui::open_alert(&mut self.ui, &mut self.store, account, channel),
                Some(button) => rune_ui::answer_alert(
                    &mut self.ui,
                    &mut self.store,
                    account,
                    channel,
                    button == rune_ui::CALL_ANSWER,
                    now,
                ),
            };
            // Declining leaves the window where it is.
            if !intents.is_empty() && !declined {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            self.run(ctx, intents);
        }
        let intents = rune_ui::local_history_changed(
            &mut self.ui,
            &self.store,
            self.client.local_history_available(),
        );
        self.run(ctx, intents);
        if let Some(result) = self.client.poll_local_read() {
            match result {
                rune_core::LocalReadResult::Search {
                    request,
                    channel,
                    result,
                } => {
                    let result = if self.ui.channel() == Some(channel)
                        && self
                            .store
                            .abilities(channel)
                            .is_some_and(|abilities| abilities.read_history.allowed())
                    {
                        result
                    } else {
                        Err(rune_model::LocalReadError::Unavailable)
                    };
                    let intents = rune_ui::local_search_result(
                        &mut self.ui,
                        &self.store,
                        request,
                        channel,
                        result,
                        now,
                    );
                    self.run(ctx, intents);
                }
                rune_core::LocalReadResult::Context {
                    request,
                    channel,
                    result,
                } => {
                    let applied = if self.ui.channel() == Some(channel)
                        && self
                            .store
                            .abilities(channel)
                            .is_some_and(|abilities| abilities.read_history.allowed())
                    {
                        match result {
                            Ok(context) => {
                                self.store.apply_cached_context(request, channel, context)
                            }
                            Err(error) => self.store.apply(
                                rune_provider::Event::Failed {
                                    failure: rune_provider::Failure::Context { request, channel },
                                    reason: error.label().to_owned(),
                                    ambiguous: false,
                                },
                                now,
                            ),
                        }
                    } else {
                        self.store.cancel_context(request);
                        rune_state::Applied::Failed {
                            failure: rune_provider::Failure::Context { request, channel },
                            reason: rune_model::LocalReadError::Unavailable.label().to_owned(),
                            ambiguous: false,
                        }
                    };
                    let intents = rune_ui::on_applied(&mut self.ui, &mut self.store, &applied, now);
                    self.run(ctx, intents);
                }
            }
        }
        if let Some(receiver) = &self.devices {
            match receiver.try_recv() {
                Ok(found) => {
                    self.ui.audio.devices = Some(found);
                    self.devices = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.ui.audio.devices =
                        Some(Err(t("No se pudieron consultar los dispositivos.").into()));
                    self.devices = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        for download in self.client.poll_downloads() {
            // A copy says what it put on the clipboard (contenido-de-mensajes).
            if let Some(copied) = download.copied {
                match download.result {
                    Ok(_) => self.ui.notify(
                        match copied {
                            rune_core::Copied::Image => t("Imagen copiada"),
                            rune_core::Copied::File => t("Video copiado"),
                        },
                        Some(download.filename),
                        false,
                        now,
                    ),
                    Err(reason) => self
                        .ui
                        .notify(t("No se pudo copiar"), Some(reason), true, now),
                }
                continue;
            }
            match download.result {
                Ok(path) => self.ui.notify(
                    t("Archivo guardado"),
                    Some(path.display().to_string()),
                    false,
                    now,
                ),
                Err(reason) => self
                    .ui
                    .notify(t("No se pudo descargar"), Some(reason), true, now),
            }
        }
        for update in self.client.poll_saves() {
            match update {
                rune_core::SaveUpdate::Saving { request } => {
                    rune_ui::save_saving(&mut self.ui, request)
                }
                rune_core::SaveUpdate::Progress {
                    request,
                    received,
                    total,
                } => rune_ui::save_progress(&mut self.ui, request, received, total),
                rune_core::SaveUpdate::Saved { request, filename } => {
                    rune_ui::save_finished(&mut self.ui, request, Ok(filename))
                }
                rune_core::SaveUpdate::Cancelled { request } => {
                    rune_ui::save_finished(&mut self.ui, request, Err(None))
                }
                rune_core::SaveUpdate::Failed { request, reason } => {
                    rune_ui::save_finished(&mut self.ui, request, Err(Some(reason.into())))
                }
                // Linux desktop (escritorio-linux).
                rune_core::SaveUpdate::Exists { request, filename } => {
                    rune_ui::save_exists(&mut self.ui, request, filename)
                }
            }
        }
        // Linux desktop (escritorio-linux).
        for chosen in self.client.poll_folders() {
            let intents = rune_ui::sound_folder_chosen(
                &mut self.ui,
                chosen.request,
                chosen.result.map_err(str::to_owned),
            );
            self.run(ctx, intents);
        }
        // Own themes (temas-propios).
        for file in self.client.poll_theme_files() {
            let result = file.result.map_err(str::to_owned);
            if file.exported {
                rune_ui::themes::exported(&mut self.ui, file.request, result, now);
            } else {
                let intents = rune_ui::themes::imported(&mut self.ui, file.request, result, now);
                self.run(ctx, intents);
            }
        }
        self.drain_uploads(ctx);
        self.restore_draft_files(ctx);
        for picked in self.client.poll_profile_images() {
            let result = match picked.result {
                Ok(Some((image, data))) => preview(&data, t("Imagen elegida"))
                    .map(|preview| Some((image, preview)))
                    .ok_or_else(|| t("No se pudo mostrar la imagen.").to_owned()),
                Ok(None) => Ok(None),
                Err(reason) => Err(reason.to_owned()),
            };
            let source = picked.source.and_then(|source| picture_source(&source));
            let intents =
                rune_ui::profile_image_picked(&mut self.ui, picked.request, result, source, now);
            self.run(ctx, intents);
        }
        for picked in self.client.poll_expression_images() {
            let result = match picked.result {
                Ok(Some((image, data))) => preview(&data, t("Imagen elegida"))
                    .map(|preview| Some((image, preview)))
                    .ok_or_else(|| t("No se pudo mostrar la imagen.").to_owned()),
                Ok(None) => Ok(None),
                Err(reason) => Err(reason.to_owned()),
            };
            let intents =
                rune_ui::expression_image_picked(&mut self.ui, picked.request, result, now);
            self.run(ctx, intents);
        }
        if let Some(result) = self.client.poll_invitation() {
            let intents = rune_ui::invitation_result(&mut self.ui, &self.store, result);
            self.run(ctx, intents);
        }
        while let Some(result) = self.client.poll_invitation_card() {
            rune_ui::invitation_card_result(&mut self.ui, &self.store, result, Instant::now());
        }
        self.ui.messages.gifs.available = self.client.gifs_available();
        let intents = rune_ui::sync_gif_search(&mut self.ui, &self.store);
        self.run(ctx, intents);
        if let Some(result) = self.client.poll_gifs() {
            let intents = rune_ui::gif_result(&mut self.ui, &self.store, result);
            self.run(ctx, intents);
        }
        if let Some(result) = self.client.poll_document() {
            let image = result
                .result
                .map_err(|error| error.label().to_owned())
                .and_then(|data| {
                    limn::RasterImage::new(
                        data.sha256.clone(),
                        limn::RasterImageFormat::Png,
                        data.size_bytes,
                        data.base64.clone(),
                        tr!("Página {page} del documento", page = result.page),
                    )
                    .ok_or_else(|| t("No se pudo mostrar la vista previa.").to_owned())
                });
            let intents = rune_ui::document_result(
                &mut self.ui,
                &self.store,
                result.request,
                result.page,
                result.pages,
                result.marks,
                image,
            );
            self.run(ctx, intents);
        }
        if let Some(read) = self.client.poll_document_text() {
            let result = read.result.map_err(|error| error.label().to_owned());
            rune_ui::document_text(&mut self.ui, read.request, result);
        }
        if let Some(fetched) = self.client.poll_voice() {
            let loaded = fetched
                .result
                .map(|ogg| self.audio.load_voice(fetched.request, ogg))
                .map_err(str::to_owned);
            rune_ui::voice_loaded(&mut self.ui, fetched.request, loaded);
        }
        // Captures and the benchmark never ask the desktop for a shortcut.
        let open = !self.pending_storage.closing && self.capture.is_none() && self.bench.is_none();
        self.global_ptt
            .sync(&mut self.ui, self.store.me(), open, ptt::portal(ctx));
        let window_held = !rune_ui::recording_key(&self.ui)
            && self.ui.prefs.push_to_talk
            && self
                .ui
                .prefs
                .push_to_talk_key
                .as_deref()
                .is_some_and(|key| ptt::window_held(&self.tree, key));
        self.audio.held = window_held || self.global_ptt.held();
        // Desktop (P6): mute and deafen by their shortcuts, and push to mute;
        // in the window only what the desktop did not bind.
        let recording = rune_ui::recording_key(&self.ui);
        let mut toggles = self.global_ptt.take_toggles();
        let window_presses = std::mem::take(&mut self.pressed);
        for shortcut in [VoiceKey::Mute, VoiceKey::Deafen] {
            let pressed = !recording
                && !self.global_ptt.bound(shortcut)
                && shortcut
                    .binding(&self.ui.prefs)
                    .filter(|key| ptt::global_allowed(key))
                    .is_some_and(|key| ptt::window_pressed(&window_presses, key));
            if pressed {
                toggles.push(shortcut);
            }
        }
        for toggle in toggles {
            let id = if toggle == VoiceKey::Mute {
                "me:mute"
            } else {
                "me:deafen"
            };
            let intents = rune_ui::update(
                &mut self.ui,
                &mut self.store,
                &limn::UiAction::Clicked { id: id.into() },
                now,
            );
            self.run(ctx, intents);
        }
        let window_muting = !recording
            && VoiceKey::PushToMute
                .binding(&self.ui.prefs)
                .is_some_and(|key| ptt::window_held(&self.tree, key));
        self.audio.muted_held = window_muting || self.global_ptt.muting();
        self.audio.step(&mut self.ui, &mut self.store, now);
        self.present_clip(ctx);
        self.ui.expire(now);
        // A status chosen for a while, or a custom status set to clear, is due.
        let due = rune_ui::tick(&mut self.ui, &self.store, wall_clock());
        self.run(ctx, due);
    }

    /// The newest picture of the video playing, on its surface. A video no
    /// longer played lets go of its surface and the texture behind it.
    fn present_clip(&mut self, ctx: &egui::Context) {
        let playing = self
            .ui
            .voice_player
            .as_ref()
            .map(|player| player.attachment);
        if let Some(shown) = self.clip_surface.filter(|shown| Some(*shown) != playing) {
            limn_egui::forget_surface(&rune_components::message::media_surface(shown));
            self.clip_surface = None;
        }
        // Videos show their pictures on Linux only for now (windows-base).
        #[cfg(not(target_os = "linux"))]
        let _ = ctx;
        #[cfg(target_os = "linux")]
        {
            let (Some(frame), Some(attachment)) = (self.audio.take_picture(), playing) else {
                return;
            };
            let presented = limn_egui::SurfaceFrame::from_rgba(
                [frame.width as usize, frame.height as usize],
                &frame.rgba,
                1.0,
            )
            .is_some_and(|surface| {
                let id = rune_components::message::media_surface(attachment);
                limn_egui::present_surface(ctx, &id, surface)
            });
            if presented {
                self.clip_surface = Some(attachment);
                if let Some(player) = self.ui.voice_player.as_mut() {
                    player.picture = true;
                }
                ctx.request_repaint();
            }
        }
    }

    /// Cancel the native close now, but retain the input already delivered
    /// with it. The request becomes active after that frame's input is applied.
    fn receive_close_request(&mut self, ctx: &egui::Context) -> bool {
        if ctx.input(|input| input.viewport().close_requested()) && !self.pending_storage.closed {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            // Desktop (P6): closing leaves RUNE in the tray, when asked.
            if !self.pending_storage.closing && self.desktop.closes_to_tray(&self.ui) {
                self.desktop.hide(ctx);
                return false;
            }
            return !self.pending_storage.closing;
        }
        false
    }

    /// The desktop work's step (P6): what the tray, later starts and the
    /// local services reported, and the font. Quitting from the tray saves
    /// first, as closing does.
    fn desktop_step(&mut self, ctx: &egui::Context, now: Instant) {
        let (intents, fonts) =
            self.desktop
                .step(ctx, &mut self.ui, &mut self.store, now, wall_clock());
        if fonts {
            self.fonts_attached = false;
            ctx.request_repaint();
        }
        self.run(ctx, intents);
        // Once: Esc cancels this close as any other.
        if self.desktop.quit && !self.desktop.hidden && !self.pending_storage.closing {
            self.desktop.quit = false;
            self.request_close(ctx, now);
        }
    }

    fn handle_input(
        &mut self,
        ctx: &egui::Context,
        area: egui::Rect,
        now: Instant,
        close_requested: bool,
        cancelled_close: bool,
    ) -> Option<Instant> {
        let window = Size::new(area.width(), area.height());
        // Native file metadata has no ordering relative to keyboard input.
        // Even a rejected batch requires a later editor submission, rather
        // than sending text or an older file set before this drop is handled.
        let native_drop =
            self.drop_rejection.is_some() || ctx.input(|input| !input.raw.dropped_files.is_empty());
        let mut view_dirty = false;
        if let Some(error) = self.drop_rejection.take() {
            if !self.pending_storage.closing && !close_requested {
                self.reject_native_drop(error, now);
                view_dirty = true;
            }
        }
        // Release Context's read guard before any host effect re-enters it.
        // Unknown events retain their position without copying their payloads.
        let (native_events, window_focused) = ctx.input(|input| {
            let events = input
                .events
                .iter()
                .map(|event| match event {
                    egui::Event::WindowFocused(focused) => {
                        Some(NativeHostEvent::WindowFocused(*focused))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            (events, input.focused)
        });
        let mut events = limn_egui::events_indexed(ctx, area.min);
        // An initial or eventless frame still supplies native focus. When
        // callbacks exist, preserve their cuts at their own source positions.
        if !native_events
            .iter()
            .any(|event| matches!(event, Some(NativeHostEvent::WindowFocused(_))))
        {
            self.tree.set_window_focused(window_focused);
        }
        if cancelled_close {
            events.retain(|event| {
                !matches!(
                    &event.event,
                    UiEvent::Key {
                        key: Key::Escape,
                        ..
                    }
                )
            });
        }
        let input_started = (!self.pending_storage.closing && !events.is_empty()).then_some(now);
        if !self.pending_storage.closing
            && (!events.is_empty() || view_dirty || self.focus.is_some())
        {
            self.prepare_input_view(window, now);
            view_dirty = false;
        }
        let mut events = events.into_iter().peekable();
        // Frame-derived legacy files have no native source index. They keep
        // their explicit rejection policy rather than inventing chronology.
        while let Some(event) = events.next_if(|event| event.source_index.is_none()) {
            if !self.pending_storage.closing {
                if view_dirty || self.focus.is_some() {
                    self.prepare_input_view(window, now);
                    view_dirty = false;
                }
                view_dirty |=
                    self.handle_ui_event(ctx, event.event, now, native_drop, close_requested);
            }
        }
        for (index, native_event) in native_events.into_iter().enumerate() {
            // Even an untranslated source event is a boundary: a preceding
            // action must finish its view and focus before the next input.
            if !self.pending_storage.closing && (view_dirty || self.focus.is_some()) {
                self.prepare_input_view(window, now);
                view_dirty = false;
            }
            if let Some(NativeHostEvent::WindowFocused(focused)) = native_event {
                // limn lets go of the keys held, and of Shift.
                self.tree.set_window_focused(focused);
                // A key being recorded is not waited for from another window.
                if !focused {
                    let intents = rune_ui::window_unfocused(&mut self.ui);
                    self.run(ctx, intents);
                }
            }
            while let Some(event) = events.next_if(|event| event.source_index == Some(index)) {
                if !self.pending_storage.closing {
                    view_dirty |=
                        self.handle_ui_event(ctx, event.event, now, native_drop, close_requested);
                } else {
                    // A release while closing still lets go of its key.
                    self.tree.observe_keys(&event.event);
                }
            }
        }
        if !self.pending_storage.closing && (view_dirty || self.focus.is_some()) {
            self.prepare_input_view(window, now);
        }
        let access_actions = limn_egui::apply_access_requests(ctx, &mut self.tree);
        if !self.pending_storage.closing {
            for action in access_actions {
                if native_drop && self.is_editor_submission(&action) {
                    continue;
                }
                let intents = rune_ui::update(&mut self.ui, &mut self.store, &action, now);
                self.run(ctx, intents);
            }
        }
        if close_requested {
            self.request_close(ctx, now);
        }
        input_started
    }

    /// Apply an event to the current view; a reducer may change it without intents.
    fn handle_ui_event(
        &mut self,
        ctx: &egui::Context,
        event: UiEvent,
        now: Instant,
        native_drop: bool,
        close_requested: bool,
    ) -> bool {
        if matches!(
            &event,
            UiEvent::FilesHovered { .. } | UiEvent::FilesDropped { .. }
        ) && (close_requested || self.pending_storage.closing)
        {
            return false;
        }
        if self.host_keys(ctx, &event) {
            return true;
        }
        if self.host_shortcut(ctx, &event) {
            return true;
        }
        // The wheel's and the trackpad's speed (imagenes-y-lectura).
        let event = scaled_scroll(event, self.ui.prefs.scroll_factor());
        rune_ui::capture_composer_selection(&mut self.ui, &self.tree);
        let focused = self.tree.focused_id().map(str::to_owned);
        if let UiEvent::Paste(text) = &event {
            if self.defer_file_paste(ctx, text, focused.as_deref(), now) {
                return true;
            }
        }
        // A secondary click on the channel list opens its own menus.
        if let UiEvent::ContextMenu { position } = &event {
            let target = self.tree.frame().hit_test(*position).map(str::to_owned);
            if let Some(intents) = target.and_then(|target| {
                rune_ui::secondary_click(&mut self.ui, &self.store, &target, *position)
            }) {
                self.run(ctx, intents);
                return true;
            }
        }
        // A double click on a stream shows it full screen (transmisiones).
        if let UiEvent::PointerDownEditor {
            position,
            click_count: 2,
            ..
        } = &event
        {
            let target = self.tree.frame().hit_test(*position).map(str::to_owned);
            if let Some(intents) =
                rune_ui::double_click(&mut self.ui, &self.store, target.as_deref())
            {
                self.run(ctx, intents);
                return true;
            }
        }
        if let Some(intents) =
            rune_ui::intercept(&mut self.ui, &mut self.store, &event, focused.as_deref())
        {
            self.run(ctx, intents);
            return true;
        }
        let mut view_dirty = false;
        for action in self.tree.handle_event(event) {
            if native_drop && self.is_editor_submission(&action) {
                continue;
            }
            view_dirty = true;
            match action {
                limn::UiAction::FilesDropped { .. } => {
                    // Even bypassing the raw hook cannot turn an unbound
                    // pointer position into authorization to open files.
                    self.reject_native_drop(uploads::NativeDropRejection::Unpositioned, now);
                }
                action => {
                    if let limn::UiAction::CopyRequested { text } = &action {
                        ctx.copy_text(text.clone());
                    }
                    let intents = rune_ui::update(&mut self.ui, &mut self.store, &action, now);
                    self.run(ctx, intents);
                }
            }
        }
        view_dirty
    }

    fn prepare_input_view(&mut self, window: Size, now: Instant) {
        let context = self.context(window, now);
        self.tree
            .set_root(rune_ui::view(&self.ui, &self.store, &context));
        self.tree.layout(window);
        if self.apply_pending_focus() {
            self.tree.layout(window);
        }
    }

    /// A failed request must not leave the previous editor accepting input.
    fn apply_pending_focus(&mut self) -> bool {
        let Some((id, cursor)) = self.focus.take() else {
            return false;
        };
        if self.tree.focus(&id) {
            if let Some(cursor) = cursor {
                self.tree.set_input_cursor(&id, cursor);
            }
            true
        } else {
            self.tree.clear_focus()
        }
    }

    fn is_editor_submission(&self, action: &limn::UiAction) -> bool {
        matches!(action, limn::UiAction::Submitted { id, .. }
            if self.ui.channel().is_some_and(|channel| *id == rune_ui::editor_id(channel)))
    }

    /// Keep a setting in the map the account's are taken back from, and
    /// send it on to the store; an account's waits for one signed in.
    fn keep_setting(&mut self, key: &str, value: Option<String>) {
        if let Some(stored) = self.kept.put(key, value.clone()) {
            self.pending_storage.set_setting(stored, value);
        }
    }

    fn request_close(&mut self, ctx: &egui::Context, now: Instant) {
        let settings: Vec<_> = self
            .ui
            .settings_to_keep()
            .into_iter()
            .filter_map(|(key, value)| Some((self.kept.put(key, value.clone())?, value)))
            .collect();
        if self.pending_storage.begin_close(settings) {
            self.ui.notify(
                t("Guardando antes de cerrar…"),
                Some(t("Esperando la confirmación local. Esc cancela el cierre.").into()),
                false,
                now,
            );
        }
        ctx.request_repaint();
    }

    /// Queue acceptance retains no second copy of a draft. Only the worker's
    /// committed barrier permits a successful close or forget notification.
    fn keep_local_data(&mut self, ctx: &egui::Context, now: Instant) {
        if self.pending_storage.closing
            && (self.ui.dirty_drafts().next().is_some()
                || !self.store.dirty_read_marks().is_empty()
                || rune_ui::draft_files_dirty(&self.ui))
        {
            self.pending_storage.ticket = None;
        }
        let due = self.pending_storage.closing
            || now.duration_since(self.drafts_kept) >= Duration::from_secs(1);
        let result = (|| {
            let forgetting = self.pending_storage.forget;
            self.pending_storage
                .submit_forget(|| self.client.forget_local_data())?;
            if forgetting {
                self.store.discard_dirty_read_marks();
                self.read_marks_to_restore = None;
            }
            if due {
                keep_drafts(&mut self.ui, |channel, text| {
                    self.client.set_draft(channel, text)
                })?;
                // Where their files are, never the files (envios-y-subidas).
                for (channel, tokens) in rune_ui::dirty_draft_files(&self.ui, &self.store) {
                    self.client.set_draft_files(channel, &tokens)?;
                    rune_ui::draft_files_kept(&mut self.ui, channel);
                }
                keep_read_marks(&mut self.store, |account, channel, mark| {
                    self.client.set_read_mark(account, channel, mark)
                })?;
                self.drafts_kept = now;
            }
            self.pending_storage
                .submit_settings(|key, value| self.client.set_setting(key, value))?;
            let committed = self
                .pending_storage
                .ticket
                .is_some_and(|ticket| self.client.storage_committed(ticket));
            self.pending_storage.advance_barrier(
                self.ui.dirty_drafts().next().is_some()
                    || !self.store.dirty_read_marks().is_empty()
                    || rune_ui::draft_files_dirty(&self.ui),
                || self.client.flush_storage(),
                committed,
            )
        })();
        let local_error = match &result {
            Err(StorageWriteError::Closed) => Some(
                t("El almacenamiento local dejó de responder. Tus cambios siguen pendientes.").into(),
            ),
            Err(StorageWriteError::TooLarge) => Some(
                t("El cambio supera el límite del almacenamiento local. Esc permite volver a editarlo.")
                    .into(),
            ),
            _ => None,
        };
        let error = self
            .client
            .storage_error()
            .map(str::to_owned)
            .or(local_error);
        if error != self.pending_storage.error {
            if let Some(error) = &error {
                // Do not put SQLite errors or row values in the log.
                eprintln!("rune: local storage notice: persistence unavailable");
                self.ui.notify(
                    t("No se pudo guardar en este equipo"),
                    Some(error.clone()),
                    true,
                    now,
                );
                ctx.request_repaint();
            }
            self.pending_storage.error = error;
        }
        if let Ok(completed) = result {
            if completed.forgot {
                self.ui.notify(
                    t("Datos locales borrados"),
                    Some(
                        t("Se confirmó el borrado de la caché y los borradores de esta cuenta.")
                            .into(),
                    ),
                    false,
                    now,
                );
                ctx.request_repaint();
            }
            if completed.close {
                self.video.clear();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if self.pending_storage.waiting() {
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if self.ui.dirty_drafts().next().is_some()
            || !self.store.dirty_read_marks().is_empty()
            || rune_ui::draft_files_dirty(&self.ui)
        {
            let retry = (self.drafts_kept + Duration::from_secs(1)).saturating_duration_since(now);
            ctx.request_repaint_after(retry.max(Duration::from_millis(100)));
        }
    }

    /// The keys as limn reports them, before anything answers them
    /// (limn-teclado-y-superficies): push to talk reads the keys the tree
    /// holds, also those taken before it; the voice toggles, the first
    /// presses (not a key repeating); and the key being recorded takes the
    /// next key pressed, whatever it is (Escape cancels; a modifier alone is
    /// not a key and waits for one). True when the recorder took the key.
    fn host_keys(&mut self, ctx: &egui::Context, event: &UiEvent) -> bool {
        let UiEvent::Key {
            key,
            shift,
            ctrl,
            alt,
        } = *event
        else {
            self.tree.observe_keys(event);
            return false;
        };
        let held = limn::Modifiers { shift, ctrl, alt };
        if !self.tree.key_held(key) {
            self.pressed.push((key, held));
            // The toggles read it in the next frame's `drain`.
            ctx.request_repaint();
        }
        self.tree.observe_keys(event);
        if !rune_ui::recording_key(&self.ui) {
            return false;
        }
        let recorded = (key != Key::Escape).then(|| ptt::binding(key, held));
        let intents = rune_ui::record_key(&mut self.ui, recorded);
        self.run(ctx, intents);
        true
    }

    /// The window's own shortcuts, read at the translated event's source
    /// position. True when the event is the host's and goes no further.
    fn host_shortcut(&mut self, ctx: &egui::Context, event: &UiEvent) -> bool {
        let Some(intents) = shortcuts::dispatch(&mut self.ui, &mut self.store, &self.tree, event)
        else {
            return false;
        };
        self.run(ctx, intents);
        true
    }

    /// Drive the benchmark: open the longest conversation, then scroll it up.
    fn bench_step(&mut self, ctx: &egui::Context, window: Size) {
        let Some(bench) = self.bench.as_mut() else {
            return;
        };
        if self.store.servers().is_empty() {
            return;
        }
        if !bench.opened {
            bench.opened = true;
            let channel = self
                .store
                .servers()
                .iter()
                .flat_map(|server| self.store.channel_tree(server.id))
                .flat_map(|(_, channels)| channels)
                .find(|channel| channel.name == "rendimiento")
                .map(|channel| (channel.id, channel.server));
            if let Some((channel, server)) = channel {
                let intents = rune_ui::open(&mut self.ui, &mut self.store, server, channel);
                self.run(ctx, intents);
            }
            return;
        }
        let x = rune_components::rail::WIDTH + rune_components::sidebar::WIDTH + 200.0;
        self.tree.handle_event(UiEvent::Scroll {
            position: Point::new(x, window.height / 2.0),
            delta_y: 90.0,
        });
        ctx.request_repaint();
    }

    /// A deferred capture action: `app:{custom_id}`, `app-pick:{n}:{custom_id}`
    /// and `app-close:{custom_id}` act on the newest message of the open
    /// conversation with that part of an app; `click:{id}` clicks; `{field}={text}`
    /// writes; `reveal:{id}` scrolls to a control; `press:{id}` and
    /// `move:{id}` drag; anything else clicks its id.
    fn capture_later(&mut self, ctx: &egui::Context, id: &str, now: Instant) {
        if let Some(target) = id.strip_prefix("reveal:") {
            self.tree.reveal_at_start(target, 16.0);
            return;
        }
        // `key:{name}` or `key:ctrl+{name}` presses a key as the window
        // would deliver it (visor-de-medios: the viewer's zoom keys).
        if let Some(name) = id.strip_prefix("key:") {
            let (ctrl, name) = name
                .strip_prefix("ctrl+")
                .map_or((false, name), |name| (true, name));
            let Some(key) = limn::Key::from_name(name) else {
                eprintln!("rune: capture key {name} is not a key");
                return;
            };
            let event = UiEvent::Key {
                key,
                shift: false,
                ctrl,
                alt: false,
            };
            let focused = self.tree.focused_id().map(str::to_owned);
            let intents =
                match rune_ui::intercept(&mut self.ui, &mut self.store, &event, focused.as_deref())
                {
                    Some(intents) => intents,
                    None => {
                        let actions = self.tree.handle_event(event);
                        actions
                            .iter()
                            .flat_map(|action| {
                                rune_ui::update(&mut self.ui, &mut self.store, action, now)
                            })
                            .collect()
                    }
                };
            self.run(ctx, intents);
            return;
        }
        // `press:{id}` picks up the control `id` as the pointer would, and
        // `move:{id}` carries it over `id` (voz-filas-y-arrastre: a drag).
        let pointer = [("press:", 24.0), ("move:", 40.0)]
            .into_iter()
            .find_map(|(prefix, x)| Some((prefix, x, id.strip_prefix(prefix)?)));
        if let Some((prefix, x, target)) = pointer {
            let Some(rect) = self
                .tree
                .frame()
                .regions
                .iter()
                .rev()
                .find(|region| region.id == target)
                .map(|region| region.rect)
            else {
                eprintln!("rune: capture action {id} has nothing to act on");
                return;
            };
            let at = Point::new(rect.x + x, rect.y + rect.height / 2.0);
            let mut actions = Vec::new();
            if prefix == "press:" {
                actions.extend(self.tree.handle_event(UiEvent::PointerMove(at)));
                actions.extend(self.tree.handle_event(UiEvent::PointerDown(at)));
                // Past the click's tolerance: the drag starts.
                let away = Point::new(at.x + 6.0, at.y + 16.0);
                actions.extend(self.tree.handle_event(UiEvent::PointerMove(away)));
            } else {
                actions.extend(self.tree.handle_event(UiEvent::PointerMove(at)));
            }
            let intents = actions
                .iter()
                .flat_map(|action| rune_ui::update(&mut self.ui, &mut self.store, action, now))
                .collect();
            self.run(ctx, intents);
            return;
        }
        let mut part = |custom_id: &str| {
            let channel = self.ui.channel()?;
            let messages = self.store.timeline(channel)?.messages();
            let index = messages.iter().rposition(|message| {
                rune_model::components::control(&message.components.parts, custom_id).is_some()
            })?;
            // Its row in view, a little below the top.
            self.tree
                .scroll_to_index(&rune_ui::timeline_id(channel), index.saturating_sub(1));
            Some(messages[index].id)
        };
        use rune_components::message::app_ids;
        let resolved = if let Some(custom_id) = id.strip_prefix("app:") {
            part(custom_id).map(|message| app_ids::part(message, custom_id))
        } else if let Some(rest) = id.strip_prefix("app-pick:") {
            rest.split_once(':').and_then(|(index, custom_id)| {
                Some(app_ids::pick(
                    part(custom_id)?,
                    index.parse().ok()?,
                    custom_id,
                ))
            })
        } else if let Some(custom_id) = id.strip_prefix("app-close:") {
            part(custom_id).map(|message| app_ids::close(message, custom_id))
        } else {
            // `{open}` names the conversation open (private calls).
            let open = self.ui.channel().map_or(0, |channel| channel.0);
            Some(
                id.strip_prefix("click:")
                    .unwrap_or(id)
                    .replace("{open}", &open.to_string()),
            )
        };
        let Some(id) = resolved else {
            eprintln!("rune: capture action {id} has nothing to act on");
            return;
        };
        let action = match id.split_once('=').filter(|_| {
            !id.contains(':') || id.starts_with("cmd:opt:") || id.starts_with("modal:text:")
        }) {
            Some((field, value)) => {
                self.tree.set_input_value(field, value);
                limn::UiAction::InputChanged {
                    id: field.into(),
                    value: value.into(),
                }
            }
            None => limn::UiAction::Clicked { id },
        };
        let intents = rune_ui::update(&mut self.ui, &mut self.store, &action, now);
        self.run(ctx, intents);
    }

    /// Set the state asked for once the provider is ready, then save the
    /// window a moment later and close.
    fn capture_step(&mut self, ctx: &egui::Context, now: Instant) {
        let Some(capture) = self.capture.as_mut() else {
            return;
        };
        if !capture_ready(&self.store) && capture.splash_at.is_none() {
            ctx.request_repaint();
            return;
        }
        if rune_ui::attachment_pick_pending(&self.ui) {
            // Capturing while a native modal is active would close the host
            // before the descriptor preparation can return to its draft.
            capture.ready = Some(now);
            ctx.request_repaint();
            return;
        }
        if capture.ready.is_none() {
            capture.ready = Some(now);
            let found = capture.channel.as_ref().and_then(|name| {
                self.store
                    .servers()
                    .iter()
                    .flat_map(|server| self.store.channel_tree(server.id))
                    .flat_map(|(_, channels)| channels)
                    // A thread or a forum post is found by its name too.
                    .flat_map(|channel| {
                        std::iter::once(channel).chain(self.store.threads(channel.id, true))
                    })
                    // A group outside servers, by its name too; a direct
                    // conversation by its title (private calls).
                    .chain(self.store.directs())
                    .find(|channel| {
                        &channel.name == name
                            || (channel.server.is_none()
                                && self.store.channel_title(channel.id) == *name)
                    })
                    .map(|channel| (channel.id, channel.server))
            });
            let actions = if capture.attachments
                || capture.viewer
                || capture.invitation
                || capture.rich
                || capture.media
                || capture.search.is_some()
                || capture.context
            {
                Vec::new()
            } else {
                std::mem::take(&mut capture.actions)
            };
            let (later, actions): (Vec<String>, Vec<String>) =
                actions.into_iter().partition(|id| id.starts_with("after:"));
            capture.later = later
                .into_iter()
                .filter_map(|id| id.strip_prefix("after:").map(str::to_owned))
                .collect();
            let draft = capture.draft.take();
            if let Some((channel, server)) = found {
                let intents = rune_ui::open(&mut self.ui, &mut self.store, server, channel);
                self.run(ctx, intents);
            }
            if let (Some(draft), Some(channel)) = (draft, self.ui.channel()) {
                let id = rune_ui::editor_id(channel);
                self.tree.set_input_value(&id, &draft);
                let intents = rune_ui::update(
                    &mut self.ui,
                    &mut self.store,
                    &limn::UiAction::InputChanged { id, value: draft },
                    now,
                );
                self.run(ctx, intents);
            }
            let mut deferred = Vec::new();
            for id in actions {
                // `{open}` names the conversation open, as in `after:`.
                let open = self.ui.channel().map_or(0, |channel| channel.0);
                let id = id.replace("{open}", &open.to_string());
                // `shift` is held in every frame (`Capture::hold_shift`).
                if id == "shift" {
                    continue;
                }
                // `file:{path}` prepares a file for the open conversation or
                // forum, from its path, as the selector would.
                if let (Some(path), Some(channel)) = (id.strip_prefix("file:"), self.ui.channel()) {
                    if let Ok(request) = rune_ui::begin_attachment_import(
                        &mut self.ui,
                        &self.store,
                        channel,
                        rune_ui::AttachmentImportSource::Picker,
                    ) {
                        let _ = self.client.prepare_attachment_paths(
                            request,
                            channel,
                            vec![path.into()],
                        );
                    }
                    continue;
                }
                // `context:{id}@{x},{y}` opens the menu of a control, as a
                // secondary click there would; nothing is laid out yet.
                if let Some(target) = id.strip_prefix("context:") {
                    let (target, at) = target.split_once('@').unwrap_or((target, ""));
                    let center = at.split_once(',').and_then(|(x, y)| {
                        Some(limn::Point::new(x.parse().ok()?, y.parse().ok()?))
                    });
                    if let Some(intents) = center.and_then(|at| {
                        rune_ui::secondary_click(&mut self.ui, &self.store, target, at)
                    }) {
                        self.run(ctx, intents);
                    }
                    continue;
                }
                // Message actions (P4): `last:{what}` and `voice-menu` wait
                // for the conversation's messages, and so does what follows.
                if id.starts_with("last:")
                    || id == "voice-menu"
                    || id == "send"
                    || !deferred.is_empty()
                {
                    deferred.push(id);
                    continue;
                }
                // `click:{id}` clicks an id that has a `=` of its own.
                if let Some(id) = id.strip_prefix("click:") {
                    let action = limn::UiAction::Clicked { id: id.into() };
                    let intents = rune_ui::update(&mut self.ui, &mut self.store, &action, now);
                    self.run(ctx, intents);
                    continue;
                }
                // `field=text` writes into a field, as typing would.
                let action = match id.split_once('=') {
                    Some((field, value)) => {
                        self.tree.set_input_value(field, value);
                        limn::UiAction::InputChanged {
                            id: field.into(),
                            value: value.into(),
                        }
                    }
                    None => limn::UiAction::Clicked { id },
                };
                let intents = rune_ui::update(&mut self.ui, &mut self.store, &action, now);
                self.run(ctx, intents);
            }
            if let Some(capture) = self.capture.as_mut() {
                capture.deferred = deferred;
            }
        }
        // Message actions (P4): what waited for the conversation's messages,
        // once they are here; the picture waits again after it.
        let due = self.capture.as_ref().is_some_and(|capture| {
            !capture.deferred.is_empty()
                && self.ui.channel().is_some_and(|channel| {
                    self.store
                        .timeline(channel)
                        .is_some_and(|timeline| timeline.latest && !timeline.is_empty())
                })
        });
        if due {
            let capture = self.capture.as_mut().expect("due only with a capture");
            let deferred = std::mem::take(&mut capture.deferred);
            capture.ready = Some(now);
            for id in deferred {
                self.capture_deferred(ctx, id, now);
            }
        }
        if self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.attachments)
        {
            if let Some((event, source, image)) = capture_fixture::attachment(&self.store, &self.ui)
            {
                let capture = self.capture.as_mut().unwrap();
                capture.attachments = false;
                capture.fixture = Some((source, image));
                capture.ready = Some(now);
                let actions = std::mem::take(&mut capture.actions);
                let applied = self.store.apply(event, now);
                let intents = rune_ui::on_applied(&mut self.ui, &mut self.store, &applied, now);
                self.run(ctx, intents);
                if let Some(channel) = self.ui.channel() {
                    self.tree.scroll_to_end(&rune_ui::timeline_id(channel));
                }
                for id in actions {
                    // Linux desktop (escritorio-linux): Core's answer when a
                    // file has the name chosen for the save under way.
                    if id == "capture:save-exists" {
                        if let Some(save) = self.ui.save_as.as_ref() {
                            let (request, name) = (save.request, save.filename.clone());
                            rune_ui::save_exists(&mut self.ui, request, name);
                        }
                        continue;
                    }
                    // `input:{id}={text}` types into a field instead of clicking.
                    let action = match id.strip_prefix("input:").and_then(|it| it.split_once('=')) {
                        Some((field, value)) => {
                            self.tree.set_input_value(field, value);
                            limn::UiAction::InputChanged {
                                id: field.to_owned(),
                                value: value.to_owned(),
                            }
                        }
                        None => limn::UiAction::Clicked { id },
                    };
                    let intents = rune_ui::update(&mut self.ui, &mut self.store, &action, now);
                    self.run(ctx, intents);
                }
            }
        }
        let viewer = self
            .capture
            .as_ref()
            .filter(|capture| capture.viewer)
            .and_then(|_| capture_fixture::viewer(&self.store, &self.ui));
        if let Some(event) = viewer {
            let capture = self.capture.as_mut().unwrap();
            capture.viewer = false;
            capture.ready = Some(now);
            // `after:` ones wait for the viewer to be laid out.
            let (later, actions): (Vec<String>, Vec<String>) = std::mem::take(&mut capture.actions)
                .into_iter()
                .partition(|id| id.starts_with("after:"));
            capture.later = later
                .iter()
                .filter_map(|id| id.strip_prefix("after:"))
                .map(str::to_owned)
                .collect();
            let applied = self.store.apply(event, now);
            let intents = rune_ui::on_applied(&mut self.ui, &mut self.store, &applied, now);
            self.run(ctx, intents);
            if let Some(channel) = self.ui.channel() {
                self.tree.scroll_to_end(&rune_ui::timeline_id(channel));
            }
            for id in actions {
                let intents = rune_ui::update(
                    &mut self.ui,
                    &mut self.store,
                    &limn::UiAction::Clicked { id },
                    now,
                );
                self.run(ctx, intents);
            }
        }
        let rich = self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.rich)
            .then(|| capture_fixture::rich(&mut self.store, &mut self.ui))
            .flatten();
        // Plays for real on the chosen output, once the conversation has
        // settled where it opens: the capture shows it moving.
        let voiced = self
            .capture
            .as_ref()
            .is_some_and(|capture| {
                capture.voice
                    && capture
                        .ready
                        .is_some_and(|ready| now.duration_since(ready) > Duration::from_millis(800))
            })
            .then(|| self.ui.channel())
            .flatten()
            .and_then(|channel| {
                let messages = self.store.timeline(channel)?.messages();
                let index = messages.iter().position(|m| m.is_voice())?;
                // The row after it, so it shows above the jump to the latest.
                let below = (index + 1).min(messages.len() - 1);
                Some((channel, below, messages[index].clone()))
            });
        if let Some((channel, below, found)) = voiced {
            self.capture.as_mut().unwrap().voice = false;
            self.capture.as_mut().unwrap().ready = Some(now);
            self.tree
                .scroll_to_index(&rune_ui::timeline_id(channel), below);
            // At 1,5×: the speed's stretch runs on the real output too.
            for verb in ["speed", "voice"] {
                let id = rune_components::message::attachment_ids::action(
                    found.id,
                    found.attachments[0].id,
                    verb,
                );
                let intents = rune_ui::update(
                    &mut self.ui,
                    &mut self.store,
                    &limn::UiAction::Clicked { id },
                    now,
                );
                self.run(ctx, intents);
            }
        }
        if let Some(fixture) = rich {
            let capture = self.capture.as_mut().unwrap();
            capture.rich = false;
            capture.fixture = Some(fixture);
            capture.ready = Some(now);
            let actions = std::mem::take(&mut capture.actions);
            if let Some(channel) = self.ui.channel() {
                self.tree.scroll_to_end(&rune_ui::timeline_id(channel));
            }
            for id in actions {
                let intents = rune_ui::update(
                    &mut self.ui,
                    &mut self.store,
                    &limn::UiAction::Clicked { id },
                    now,
                );
                self.run(ctx, intents);
            }
        }
        let media = self
            .capture
            .as_ref()
            .filter(|capture| capture.media)
            .map(|capture| capture.players)
            .and_then(|players| capture_fixture::media(&mut self.store, &mut self.ui, players));
        if let Some(fixture) = media {
            let capture = self.capture.as_mut().unwrap();
            capture.media = false;
            capture.fixture = Some(fixture);
            capture.ready = Some(now);
            let actions = std::mem::take(&mut capture.actions);
            if let Some(channel) = self.ui.channel() {
                self.tree.scroll_to_end(&rune_ui::timeline_id(channel));
            }
            for id in actions {
                let intents = rune_ui::update(
                    &mut self.ui,
                    &mut self.store,
                    &limn::UiAction::Clicked { id },
                    now,
                );
                self.run(ctx, intents);
            }
        }
        // Its video plays for real once the rows settled: the capture shows
        // a picture of it on its surface.
        let play = self.capture.as_ref().is_some_and(|capture| {
            capture.media_play
                && !capture.media
                && capture
                    .ready
                    .is_some_and(|ready| now.duration_since(ready) > Duration::from_millis(800))
        });
        if play {
            let capture = self.capture.as_mut().unwrap();
            capture.media_play = false;
            capture.ready = Some(now);
            let id = rune_components::message::attachment_ids::action(
                capture_fixture::MEDIA_VIDEO,
                rune_model::AttachmentId(205),
                "play",
            );
            let intents = rune_ui::update(
                &mut self.ui,
                &mut self.store,
                &limn::UiAction::Clicked { id },
                now,
            );
            self.run(ctx, intents);
        }
        if self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.invitation)
            && capture_fixture::invitation(&mut self.store, &mut self.ui)
        {
            let capture = self.capture.as_mut().unwrap();
            capture.invitation = false;
            capture.ready = Some(now);
            let actions = std::mem::take(&mut capture.actions);
            if let Some(channel) = self.ui.channel() {
                self.tree.scroll_to_end(&rune_ui::timeline_id(channel));
            }
            for id in actions {
                let intents = rune_ui::update(
                    &mut self.ui,
                    &mut self.store,
                    &limn::UiAction::Clicked { id },
                    now,
                );
                self.run(ctx, intents);
            }
        }
        // Rules and onboarding (2026-10-18): the simulated club is joined
        // as its invitation would, then what it asks is opened.
        let club = rune_model::ServerId(8_000_000_001);
        match self.capture.as_ref().and_then(|capture| capture.onboarding) {
            Some(false) if self.ui.channel().is_some() => {
                if let Some(account) = self.store.me() {
                    let command = rune_provider::Command::AcceptInvitation {
                        request: self.ui.fresh_request(),
                        target: rune_provider::InvitationTarget {
                            key: "club".into(),
                            url: "https://simulado.invalid/invite/club".into(),
                        },
                        destination: rune_provider::InvitationDestination::Server(club),
                        account,
                        expires_at: now + Duration::from_secs(60),
                    };
                    self.capture.as_mut().unwrap().onboarding = Some(true);
                    self.run(ctx, vec![Intent::Command(command)]);
                }
            }
            Some(true) if self.store.join_steps(club).any() => {
                let mut intents = rune_ui::open_server(&mut self.ui, &mut self.store, Some(club));
                intents.extend(rune_ui::update(
                    &mut self.ui,
                    &mut self.store,
                    &limn::UiAction::Clicked {
                        id: format!("onb:open:{}", club.0),
                    },
                    now,
                ));
                let capture = self.capture.as_mut().unwrap();
                capture.onboarding = None;
                capture.ready = Some(now);
                self.run(ctx, intents);
            }
            _ => {}
        }
        let history_ready = self.ui.channel().is_some_and(|channel| {
            self.store
                .timeline(channel)
                .is_some_and(|timeline| timeline.latest)
        });
        if history_ready
            && self
                .capture
                .as_ref()
                .is_some_and(|capture| capture.search.is_some())
        {
            let capture = self.capture.as_mut().unwrap();
            let query = capture.search.take().unwrap();
            capture.ready = Some(now);
            // With `--data` it searches what is kept on this device; without
            // it, the loaded messages, or the scope an `--action` chooses.
            let mut actions = vec!["message-search:open".to_owned()];
            if self.local_data {
                actions.push("message-search:scope".to_owned());
                actions.push("message-search:scope:device".to_owned());
            }
            actions.extend(std::mem::take(&mut capture.actions));
            let remote = actions.iter().any(|id| id == "message-search:scope:remote");
            for id in actions {
                let intents = rune_ui::update(
                    &mut self.ui,
                    &mut self.store,
                    &limn::UiAction::Clicked { id },
                    now,
                );
                self.run(ctx, intents);
            }
            let id = "message-search:query".to_owned();
            self.tree.set_input_value(&id, &query);
            let intents = rune_ui::update(
                &mut self.ui,
                &mut self.store,
                &limn::UiAction::InputChanged {
                    id: id.clone(),
                    value: query.clone(),
                },
                now,
            );
            self.run(ctx, intents);
            // The whole history is asked with Enter.
            if remote {
                let intents = rune_ui::update(
                    &mut self.ui,
                    &mut self.store,
                    &limn::UiAction::Submitted { id, value: query },
                    now,
                );
                self.run(ctx, intents);
            }
        }
        if history_ready && self.capture.as_ref().is_some_and(|capture| capture.context) {
            let capture = self.capture.as_mut().unwrap();
            capture.context = false;
            capture.ready = Some(now);
            let actions = std::mem::take(&mut capture.actions);
            if let Some(channel) = self.ui.channel() {
                if let Some(anchor) = self
                    .store
                    .timeline(channel)
                    .and_then(|timeline| timeline.oldest())
                    .and_then(|id| id.0.checked_sub(100))
                    .map(rune_model::MessageId)
                {
                    let intents = rune_ui::open_message(
                        &mut self.ui,
                        &self.store,
                        channel,
                        anchor,
                        rune_model::HistorySource::Provider,
                    );
                    self.run(ctx, intents);
                }
            }
            for id in actions {
                let intents = rune_ui::update(
                    &mut self.ui,
                    &mut self.store,
                    &limn::UiAction::Clicked { id },
                    now,
                );
                self.run(ctx, intents);
            }
        }
        let Some(capture) = self.capture.as_mut() else {
            return;
        };
        // `after:{id}` once the history and the app answered (P3).
        if !capture.later.is_empty()
            && capture
                .ready
                .is_some_and(|ready| now.duration_since(ready) > Duration::from_millis(1_200))
        {
            let id = capture.later.remove(0);
            capture.ready = Some(now);
            self.capture_later(ctx, &id, now);
            ctx.request_repaint();
            return;
        }
        let Some(capture) = self.capture.as_mut() else {
            return;
        };
        if !capture.requested
            && capture.file_drop != Some(DropCapture::Waiting)
            && !capture.attachments
            && !capture.viewer
            && !capture.invitation
            && capture.onboarding.is_none()
            && !capture.rich
            && !capture.media
            && !capture.media_play
            && !capture.voice
            && capture.search.is_none()
            && !capture.context
            && match capture.splash_at {
                Some(at) => now >= at,
                None => capture
                    .ready
                    .is_some_and(|ready| now.duration_since(ready) > Duration::from_millis(1_500)),
            }
        {
            capture.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            let [width, height] = image.size;
            match image::save_buffer(
                &capture.path,
                image.as_raw(),
                width as u32,
                height as u32,
                image::ColorType::Rgba8,
            ) {
                Ok(()) => println!("rune: saved {}", capture.path.display()),
                Err(error) => eprintln!("rune: could not save {}: {error}", capture.path.display()),
            }
            self.request_close(ctx, now);
        }
        ctx.request_repaint();
    }

    /// Carry out a capture's action that waited for the conversation's
    /// messages: `last:{what}`, `voice-menu`, or a click or a field's text.
    fn capture_deferred(&mut self, ctx: &egui::Context, id: String, now: Instant) {
        // Message actions (P4): `last:{what}` reports `msg:{id}:{what}`
        // for the newest message of the open conversation it fits;
        // `voice-menu` opens the menu of the first person in voice in
        // the open server.
        if let Some(what) = id.strip_prefix("last:") {
            // `last:emoji-card` also opens the card of its first emoji (2026-10-18).
            let card = what == "emoji-card";
            let what = if card { "emoji" } else { what };
            if let Some((channel, found)) = self.ui.channel().and_then(|channel| {
                last_message(&self.store, channel, what).map(|found| (channel, found))
            }) {
                // `last:emoji` brings the newest message naming a server's
                // emoji into view (2026-10-17); `last:forward`, the newest
                // forward (contenido-de-mensajes).
                let intents = if matches!(what, "emoji" | "forward" | "event-invitation") {
                    let mut intents = rune_ui::open_message(
                        &mut self.ui,
                        &self.store,
                        channel,
                        found,
                        rune_model::HistorySource::Provider,
                    );
                    let first = self
                        .store
                        .timeline(channel)
                        .and_then(|timeline| timeline.get(found))
                        .and_then(|message| {
                            message.text_references.iter().find(|reference| {
                                matches!(reference.target, rune_model::TextTarget::Emoji { .. })
                            })
                        })
                        .map(|reference| reference.source.clone());
                    if let Some(token) = first.filter(|_| card) {
                        // Its number may be another: the card stands under
                        // the first of the message that is this emoji.
                        let action = limn::UiAction::Clicked {
                            id: format!("msg:{found}:emoji:0:{token}"),
                        };
                        intents.extend(rune_ui::update(
                            &mut self.ui,
                            &mut self.store,
                            &action,
                            now,
                        ));
                    }
                    intents
                } else {
                    let action = limn::UiAction::Clicked {
                        id: format!("msg:{found}:{what}"),
                    };
                    rune_ui::update(&mut self.ui, &mut self.store, &action, now)
                };
                self.run(ctx, intents);
            }
            return;
        }
        if id == "voice-menu" {
            let me = self.store.me();
            // The simulation may seat someone in two servers at once: the
            // menu is of where they are first.
            let person = self.ui.server.and_then(|server| {
                self.store
                    .channel_tree(server)
                    .into_iter()
                    .flat_map(|(_, channels)| channels)
                    .flat_map(|channel| self.store.voice_in(channel.id))
                    .find(|state| {
                        Some(state.user) != me
                            && self
                                .store
                                .voice_state_of(state.user)
                                .is_some_and(|first| first.channel == state.channel)
                    })
                    .map(|state| state.user)
            });
            if let Some(intents) = person.and_then(|user| {
                rune_ui::secondary_click(
                    &mut self.ui,
                    &self.store,
                    &format!("vz:user:{user}"),
                    limn::Point::new(420.0, 60.0),
                )
            }) {
                self.run(ctx, intents);
            }
            return;
        }
        // `send` sends the open conversation's draft, as Enter would.
        if let (true, Some(channel)) = (id == "send", self.ui.channel()) {
            let value = self.ui.drafts.get(&channel).cloned().unwrap_or_default();
            let action = limn::UiAction::Submitted {
                id: rune_ui::editor_id(channel),
                value,
            };
            let intents = rune_ui::update(&mut self.ui, &mut self.store, &action, now);
            self.run(ctx, intents);
            return;
        }
        let action = match (id.strip_prefix("click:"), id.split_once('=')) {
            (Some(clicked), _) => limn::UiAction::Clicked {
                id: clicked.to_owned(),
            },
            (None, Some((field, value))) => {
                self.tree.set_input_value(field, value);
                limn::UiAction::InputChanged {
                    id: field.into(),
                    value: value.into(),
                }
            }
            (None, None) => limn::UiAction::Clicked { id },
        };
        let intents = rune_ui::update(&mut self.ui, &mut self.store, &action, now);
        self.run(ctx, intents);
    }

    fn finish_bench(&mut self, ctx: &egui::Context) {
        let Some(bench) = &self.bench else { return };
        if bench.frames < 600 {
            return;
        }
        let times = &bench.times;
        let percentile = |part: &dyn Fn(&FrameTimes) -> Duration, q: f64| {
            let mut values: Vec<f64> = times
                .iter()
                .map(|times| part(times).as_secs_f64() * 1_000.0)
                .collect();
            values.sort_by(f64::total_cmp);
            values[((values.len() - 1) as f64 * q) as usize]
        };
        println!(
            "rune bench: {} frames scrolling #rendimiento, {} messages loaded",
            times.len(),
            self.store.loaded_messages()
        );
        for (name, part) in [
            (
                "view",
                &(|times: &FrameTimes| times.view) as &dyn Fn(&FrameTimes) -> Duration,
            ),
            ("layout", &|times: &FrameTimes| times.layout),
            ("paint", &|times: &FrameTimes| times.paint),
        ] {
            println!(
                "  {name:<7} p50 {:.2} ms  p95 {:.2} ms  max {:.2} ms",
                percentile(part, 0.5),
                percentile(part, 0.95),
                percentile(part, 1.0)
            );
        }
        for line in self.metrics.lines(self.store.loaded_messages(), None) {
            println!("  {line}");
        }
        self.request_close(ctx, Instant::now());
    }
}

/// Keep egui's zoom and the setting together: the setting's changes go to
/// egui, and Ctrl +, Ctrl − and Ctrl 0, which egui answers itself, come back
/// to the setting, held to its range. `applied` is the zoom last given to
/// egui. Returns the setting to keep when the keyboard changed it.
fn follow_zoom(ctx: &egui::Context, applied: &mut f32, prefs: &mut rune_ui::Prefs) -> Option<u16> {
    let current = ctx.zoom_factor();
    let mut changed = None;
    if (current - *applied).abs() > 0.001 {
        *applied = current;
        prefs.zoom = (current * 100.0).round().clamp(50.0, 200.0) as u16;
        changed = Some(prefs.zoom);
    }
    let wanted = prefs.zoom_factor();
    if (wanted - *applied).abs() > 0.001 {
        ctx.set_zoom_factor(wanted);
        *applied = wanted;
    }
    changed
}

impl eframe::App for App {
    /// While hidden in the tray only this runs (P6): what arrives is still
    /// applied and may notify, and the tray and later starts still answer.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.desktop.hidden {
            let now = Instant::now();
            self.drain(ctx, now);
            self.desktop_step(ctx, now);
        }
    }

    /// Own themes (temas-propios): a window the desktop may show through
    /// starts clear, and limn paints its translucent ground over it; an
    /// opaque one is covered by limn whole.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.ui.themes.see_through {
            [0.0; 4]
        } else {
            visuals.panel_fill.to_normalized_gamma_f32()
        }
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        if let Some(error) = uploads::reject_legacy_drop(input) {
            self.drop_rejection = Some(error);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // The language chosen, for what other threads write too (the core,
        // the tray): a change shows in this frame.
        rune_i18n::set(self.ui.language());
        // limn's own texts (a text field's menu, a picker's buttons) follow it.
        limn::set_labels(match self.ui.language() {
            rune_i18n::Language::Spanish => limn::Labels::SPANISH,
            rune_i18n::Language::English => limn::Labels::ENGLISH,
        });
        if !self.fonts_attached {
            limn_egui::attach_fonts(&mut self.tree, &ctx);
            self.fonts_attached = true;
        }
        limn_egui::normalize_keyboard_input(&ctx);
        let now = Instant::now();
        let close_requested = self.receive_close_request(&ctx);
        let cancelled_close =
            self.pending_storage.closing && ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if cancelled_close {
            self.pending_storage.cancel_close();
            self.ui.notify(t("Cierre cancelado"), None, false, now);
        }
        if !self.pending_storage.closing {
            if let Some(zoom) = follow_zoom(&ctx, &mut self.zoom, &mut self.ui.prefs) {
                self.keep_setting("zoom", Some(zoom.to_string()));
            }
        }
        self.metrics.cpu_frame = frame.info().cpu_usage;
        // The window's own frame (barra-e-icono): its edges, and the app inside.
        let rim = self.ui.look().rail;
        let area = self.frame.content(ui, &mut self.ui.desktop, rim);
        let window = Size::new(area.width(), area.height());

        self.drain(&ctx, now);
        self.desktop_step(&ctx, now);
        // The splash grows into the usual window once the opening screen
        // says why it waits, or gives way to the login or the interface.
        if self.frame.compact && !rune_ui::splash::compact(&self.ui, &self.store, now) {
            self.frame.grow(&ctx);
        }
        // RUNE's own updates (actualizaciones): installed, RUNE closes
        // (saving first, as any close) to start the new version.
        if self.updates.step(&mut self.ui) {
            self.desktop.quit = true;
            ctx.request_repaint();
        }
        if !self.pending_storage.closing {
            self.bench_step(&ctx, window);
            self.capture_step(&ctx, now);
        }

        let input_started = self.handle_input(&ctx, area, now, close_requested, cancelled_close);
        // The window's own title bar (barra-e-icono): its free stretches move it.
        let tree = &self.tree;
        self.frame.caption(&ctx, area, |point| {
            crate::frame::on_control(tree.frame(), point)
        });
        // Input can close the picker without scheduling another frame. Release
        // its page and cancel media work before this frame goes idle.
        let intents = rune_ui::sync_gif_search(&mut self.ui, &self.store);
        self.run(&ctx, intents);

        // Repeat the synthetic hover after OS pointer events so the capture
        // does not depend on where the user's real pointer happens to be.
        if self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.hover_last_message)
        {
            if let Some(id) = self.ui.channel().map(rune_ui::timeline_id) {
                if let Some(region) = self
                    .tree
                    .frame()
                    .regions
                    .iter()
                    .find(|region| region.id == id && region.kind == limn::HitKind::Scroll)
                {
                    let rect = region.rect;
                    self.tree.handle_event(UiEvent::PointerMove(Point::new(
                        rect.x + rect.width / 2.0,
                        rect.y + (rect.height - 8.0).max(0.0),
                    )));
                }
            }
        }
        if let Some(id) = self
            .capture
            .as_ref()
            .and_then(|capture| capture.hover_id.as_ref())
        {
            if let Some(region) = self
                .tree
                .frame()
                .regions
                .iter()
                .find(|region| &region.id == id)
            {
                let rect = region.rect;
                self.tree.handle_event(UiEvent::PointerMove(Point::new(
                    rect.x + rect.width / 2.0,
                    rect.y + rect.height / 2.0,
                )));
            }
        }

        // Shift held after the OS's own changes of the modifiers, as the
        // hover above.
        if self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.hold_shift)
        {
            let held = UiEvent::ModifiersChanged(limn::Modifiers {
                shift: true,
                ..Default::default()
            });
            self.handle_ui_event(&ctx, held, now, false, false);
        }

        // Describe the new state, lay it out, and paint it.
        let view_started = Instant::now();
        let context = self.context(window, now);
        let root = rune_ui::view(&self.ui, &self.store, &context);
        let view = view_started.elapsed();
        self.tree.set_root(root);
        let layout_started = Instant::now();
        let mut laid_out = self.tree.layout(window);
        if self.apply_pending_focus() {
            laid_out = self.tree.layout(window);
        }
        let layout = layout_started.elapsed();
        // A character none of RUNE's fonts draws: a system font, once found.
        if self.desktop.text_step(&ctx, &laid_out) {
            self.fonts_attached = false;
            ctx.request_repaint();
        }
        if !self.pending_storage.closing {
            let intents = rune_ui::after_layout(
                &mut self.ui,
                &mut self.store,
                &self.tree,
                ctx.input(|input| input.focused),
            );
            self.run(&ctx, intents);
        }
        if self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.invitation_cards)
        {
            let cards = rune_ui::wanted_invitation_cards(
                &mut self.ui,
                &self.store,
                &self.tree,
                Instant::now(),
                capture_fixture::recognize,
            );
            for (request, target) in cards {
                let answer = capture_fixture::card_answer(&self.store, &self.ui, request, target);
                rune_ui::invitation_card_result(&mut self.ui, &self.store, answer, Instant::now());
                ctx.request_repaint();
            }
        } else if self.capture.is_none()
            // A capture of the simulation looks its own links up too
            // (eventos-programados): nothing leaves the machine.
            || self
                .store
                .provider
                .as_ref()
                .is_some_and(|info| info.kind == rune_provider::ProviderKind::Simulated)
        {
            let client = &self.client;
            let cards = rune_ui::wanted_invitation_cards(
                &mut self.ui,
                &self.store,
                &self.tree,
                Instant::now(),
                |url| client.invitation_target(url),
            );
            for (request, target) in cards {
                self.client.preview_invitation_card(request, target);
            }
        }
        rune_ui::follow_viewer(&mut self.ui, &self.store, &self.tree);
        let mut wanted = rune_ui::wanted_images(&self.ui, &self.store, &self.tree);
        let fixture = self
            .capture
            .as_ref()
            .and_then(|capture| capture.fixture.as_ref());
        if let Some((source, _)) = fixture {
            wanted.retain(|image| image != source);
        }
        // Pictures on disk (imagenes-y-lectura): the person's budgets as they
        // change, and what it takes for the settings.
        self.client
            .set_image_cache_limits(image_cache_limits(&self.ui.prefs));
        let usage = self
            .client
            .image_cache_usage()
            .map(|usage| rune_ui::reading::CacheUsage {
                media_bytes: usage.media_bytes,
                artwork_bytes: usage.artwork_bytes,
            });
        if usage != self.ui.image_cache {
            self.ui.image_cache = usage;
            ctx.request_repaint();
        }
        let ready = self.client.load_images(&wanted);
        let failures = self.client.image_failures(&wanted);
        if failures != self.ui.image_failures {
            self.ui.image_failures = failures;
            ctx.request_repaint();
        }
        let mut retained: std::collections::HashSet<_> =
            ready.iter().map(|(source, _)| source.clone()).collect();
        if let Some((source, image)) = fixture {
            retained.insert(source.clone());
            if self
                .ui
                .images
                .insert(source.clone(), image.clone())
                .is_none()
            {
                ctx.request_repaint();
            }
        }
        // A capture draws the simulation's pictures, which nobody hosts (P5).
        if self.capture.is_some() {
            for source in wanted
                .iter()
                .filter(|source| source.url.contains("simulacion.invalid"))
            {
                retained.insert(source.clone());
                if !self.ui.images.contains_key(source) {
                    if let Some(image) = capture_fixture::simulated(source, ctx.pixels_per_point())
                    {
                        self.ui.images.insert(source.clone(), image);
                        ctx.request_repaint();
                    }
                }
            }
        }
        // Paused by reduced motion, a viewer, a minimized window or rows
        // scrolled away: those surfaces are forgotten until they play again.
        let still = self.ui.reduced_motion
            || limn_egui::prefers_reduced_motion() == Some(true)
            || ctx.input(|input| input.viewport().minimized == Some(true));
        let playable = rune_ui::playable_images(&self.ui, &self.store, &self.tree, still);
        let mut playing: Vec<_> = ready
            .iter()
            .filter(|(source, _)| playable.contains(source))
            .filter_map(|(_, data)| Some((data.sha256.clone(), data.animation.clone()?)))
            .collect();
        if let Some(capture) = &self.capture {
            if let (Some((source, image)), Some(animation)) = (&capture.fixture, &capture.animation)
            {
                if playable.contains(source) {
                    playing.push((image.sha256().to_owned(), animation.clone()));
                }
            }
        }
        // «Animar GIF» off (imagenes-y-lectura): only what the pointer is
        // over plays; the rest stands on its first frame.
        let pointer = ctx
            .input(|input| input.pointer.hover_pos())
            .map(|at| Point::new(at.x - area.min.x, at.y - area.min.y));
        animations::now_playing(self.ui.prefs.animate_gifs, &mut playing, &laid_out, pointer);
        let next_frame = self.animations.step(
            &ctx,
            playing,
            &animations::laid_out(&laid_out),
            Instant::now(),
        );
        if let Some(due) = next_frame {
            ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
        }
        let shown = self.animations.shown();
        if shown != self.ui.animated {
            self.ui.animated = shown;
            ctx.request_repaint();
        }
        let parent = &mut self.parent;
        let (changed, intents) = self.video.step(
            &ctx,
            &mut self.ui,
            &mut self.store,
            &laid_out,
            Instant::now(),
            &mut || parent.get(),
        );
        if changed {
            ctx.request_repaint();
        }
        // A shared screen whose source ended stops its stream.
        self.run(&ctx, intents);
        if let Some(poll) = self.video.next_poll() {
            ctx.request_repaint_after(poll);
        }
        let old_count = self.ui.images.len();
        self.ui.images.retain(|source, _| retained.contains(source));
        let mut images_changed = self.ui.images.len() != old_count;
        for (source, data) in ready {
            if self
                .ui
                .images
                .get(&source)
                .is_some_and(|image| image.sha256() == data.sha256)
            {
                continue;
            }
            let format = match data.format {
                rune_core::Encoding::Png => limn::RasterImageFormat::Png,
                rune_core::Encoding::Jpeg => limn::RasterImageFormat::Jpeg,
                rune_core::Encoding::Webp => limn::RasterImageFormat::Webp,
                rune_core::Encoding::Gif => limn::RasterImageFormat::Gif,
            };
            if let Some(image) = limn::RasterImage::new(
                data.sha256.clone(),
                format,
                data.size_bytes,
                data.base64.clone(),
                t("Imagen"),
            ) {
                self.ui.images.insert(source, image);
                images_changed = true;
            }
        }
        if images_changed {
            ctx.request_repaint();
        }
        limn_egui::set_cursor(&ctx, self.tree.cursor());
        let paint_started = Instant::now();
        limn_egui::paint_at(ui, &laid_out, area.min);
        let paint = paint_started.elapsed();
        // A stream shown alone, or in a window of its own.
        self.video.show_windows(&ctx, &mut self.ui, &self.store);
        let times = FrameTimes {
            view,
            layout,
            paint,
            input_to_paint: input_started.map(|started| started.elapsed()),
        };
        self.metrics.frame(times);
        if let Some(bench) = self.bench.as_mut().filter(|bench| bench.opened) {
            bench.frames += 1;
            bench.times.push(times);
        }
        if !self.pending_storage.closing {
            self.finish_bench(&ctx);
        }
        self.keep_local_data(&ctx, now);

        let title = match self.ui.channel() {
            Some(channel) => format!("RUNE — {}", self.store.channel_title(channel)),
            None => "RUNE".into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        // Paint again only when something will change.
        let mut next = self.tree.animation_repaint_after();
        let mut sooner = |after: Option<Duration>| {
            if let Some(after) = after {
                next = Some(next.map_or(after, |next| next.min(after)));
            }
        };
        sooner(self.store.typing_expires(now));
        // A timeout lifts by itself, and its remaining time counts down.
        sooner(self.store.restriction_expires());
        sooner(self.ui.next_expiry(now));
        sooner(rune_ui::next_tick(&self.ui, wall_clock()));
        // A moment written relative to now reads differently as time goes (P4).
        sooner(rune_ui::moments_tick(&self.ui, &self.store));
        // The call's clock moves each second.
        sooner(rune_ui::call_tick(&self.store, now));
        // And a voice room's time in the column each minute (voz-protocolo).
        sooner(rune_ui::voice_since_tick(&self.store, wall_clock()));
        // A ring plays again while it rings (private calls).
        sooner(rune_ui::ring_tick(&self.store));
        // An app that does not answer stops being waited for (P3).
        sooner(rune_ui::interaction_tick(&self.store, now));
        // The time of an activity shown moves each second (P5).
        sooner(rune_ui::activity_tick(&self.ui, &self.store));
        // A command waiting for its answer fails at its deadline.
        sooner(self.client.next_deadline(now));
        if self.ui.show_metrics {
            sooner(Some(Duration::from_millis(80)));
        }
        if self.audio.active() {
            sooner(Some(Duration::from_millis(50)));
        }
        // A video playing shows its pictures as they come, up to 30 a second.
        if self.ui.voice_player.as_ref().is_some_and(|player| {
            player.kind == rune_ui::Played::Video && player.state == rune_ui::VoiceState::Playing
        }) {
            sooner(Some(Duration::from_millis(33)));
        }
        if let Some(after) = next {
            ctx.request_repaint_after(after);
        }
    }
}

#[cfg(test)]
mod input_tests;

#[cfg(test)]
mod read_tests;

#[cfg(test)]
mod image_cache_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_choices_survive_busy_storage_and_block_close_until_the_commit_barrier() {
        let provider = MockProvider::new(MockConfig::default());
        let snapshot = provider.snapshot();
        let channel = snapshot
            .channels
            .iter()
            .find(|channel| channel.name == "general")
            .unwrap()
            .id;
        let message = snapshot
            .last_messages
            .iter()
            .find(|(id, _)| *id == channel)
            .unwrap()
            .1;
        let account = snapshot.me.as_ref().unwrap().id;
        let mut store = Store::new();
        store.provider = Some(provider.info());
        store.apply(
            rune_provider::Event::Ready(Box::new(snapshot)),
            Instant::now(),
        );
        // A reminder kept here: the simulation takes them too (P2), but what
        // matters is the local choice waiting for storage.
        store.mark_unread(channel, message);
        let before = store.dirty_read_marks();
        assert!(!before.is_empty());
        assert_eq!(
            keep_read_marks(&mut store, |_, _, _| Err(StorageWriteError::Busy)),
            Err(StorageWriteError::Busy)
        );
        assert_eq!(store.dirty_read_marks(), before);
        let mut pending = PendingStorage::default();
        pending.begin_close([]);
        assert_eq!(
            pending
                .advance_barrier(
                    !store.dirty_read_marks().is_empty(),
                    || panic!("dirty reads cannot be flushed past"),
                    false
                )
                .unwrap(),
            StorageCompletion::default()
        );
        let mut written = Vec::new();
        keep_read_marks(&mut store, |owner, id, mark| {
            written.push((owner, id, mark));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            written,
            before
                .into_iter()
                .map(|(id, mark)| (account, id, mark))
                .collect::<Vec<_>>()
        );
        assert!(store.dirty_read_marks().is_empty());
        assert_eq!(
            pending
                .advance_barrier(false, || Ok(Some(StorageTicket(7))), false)
                .unwrap(),
            StorageCompletion::default()
        );
        assert_eq!(
            pending
                .advance_barrier(false, || panic!("barrier already queued"), false)
                .unwrap(),
            StorageCompletion::default()
        );
        assert!(
            pending
                .advance_barrier(false, || panic!("barrier already committed"), true)
                .unwrap()
                .close
        );
    }

    fn pins_host() -> (egui::Context, App, ChannelId, rune_model::Message) {
        let ctx = egui::Context::default();
        let now = Instant::now();
        let provider = Arc::new(MockProvider::new(MockConfig {
            latency: Duration::ZERO,
            ambient: false,
            replies: false,
            ..MockConfig::default()
        }));
        let snapshot = provider.snapshot();
        let channel = snapshot
            .channels
            .iter()
            .find(|channel| channel.name == "general")
            .unwrap()
            .clone();
        let mut message = provider.history(channel.id, None, 1).0.pop().unwrap();
        message.pinned = true;
        let mut app = App::new(
            &ctx,
            now,
            Arguments {
                screenshot: Some("unused-headless-pins.png".into()),
                ..Arguments::default()
            },
            provider,
            None,
        );
        app.store
            .apply(rune_provider::Event::Ready(Box::new(snapshot)), now);
        let intents = rune_ui::open(&mut app.ui, &mut app.store, channel.server, channel.id);
        app.run(&ctx, intents);
        (ctx, app, channel.id, message)
    }

    #[test]
    fn pins_host_registers_a_read_before_dispatch_and_accepts_only_its_response() {
        use rune_provider::{Command, Event};
        let (ctx, mut app, channel, message) = pins_host();
        app.run(
            &ctx,
            vec![Intent::Command(Command::LoadPins {
                request: 401,
                channel,
            })],
        );
        assert!(matches!(
            app.store.apply(
                Event::PinsLoaded {
                    request: 400,
                    channel,
                    messages: vec![message.clone()]
                },
                Instant::now()
            ),
            rune_state::Applied::Nothing
        ));
        assert!(app.store.pins(channel).is_none());
        assert!(matches!(app.store.apply(Event::PinsLoaded {
            request: 401, channel, messages: vec![message]
        }, Instant::now()), rune_state::Applied::Pins { channel: loaded, request: 401 } if loaded == channel));
        assert_eq!(app.store.pins(channel).unwrap().len(), 1);
    }

    #[test]
    fn pins_host_cancels_the_local_read_before_a_late_response() {
        use rune_provider::{Command, Event};
        let (ctx, mut app, channel, message) = pins_host();
        assert!(app.store.begin_pins(channel, 402).allowed());
        app.run(&ctx, vec![Intent::Command(Command::Cancel(402))]);
        assert!(matches!(
            app.store.apply(
                Event::PinsLoaded {
                    request: 402,
                    channel,
                    messages: vec![message]
                },
                Instant::now()
            ),
            rune_state::Applied::Nothing
        ));
        assert!(app.store.pins(channel).is_none());
    }

    #[test]
    fn pins_host_revalidates_admission_and_replaces_loading_with_the_exact_failure() {
        let (ctx, mut app, channel, message) = pins_host();
        let now = Instant::now();
        let intents = rune_ui::update(
            &mut app.ui,
            &mut app.store,
            &limn::UiAction::Clicked {
                id: rune_components::channel_menu::ids::PINS.into(),
            },
            now,
        );
        let request = intents
            .iter()
            .find_map(|intent| match intent {
                Intent::Command(rune_provider::Command::LoadPins {
                    request,
                    channel: loaded,
                }) if *loaded == channel => Some(*request),
                _ => None,
            })
            .expect("the popout started a read before the session changed");
        app.store.connection = rune_provider::ConnectionState::Disconnected {
            reason: "synthetic disconnect".into(),
        };
        app.run(&ctx, intents);
        app.tree
            .set_text_measurer(|text, size, _| text.chars().count() as f32 * size * 0.5);
        let window = Size::new(1280.0, 800.0);
        app.tree.set_root(rune_ui::view(
            &app.ui,
            &app.store,
            &app.context(window, now),
        ));
        let frame = app.tree.layout(window);
        assert!(
            frame.regions.iter().any(|region| region.id == "pins:retry"),
            "denial must end the matching Loading phase"
        );
        assert!(frame.commands.iter().any(|command| matches!(command,
            limn::DrawCommand::Text { text, .. } if text.contains("Esperá a que la sesión esté disponible"))));
        assert!(
            app.ui.notices.is_empty(),
            "the popout owns the exact request failure"
        );
        assert!(matches!(
            app.store.apply(
                rune_provider::Event::PinsLoaded {
                    request,
                    channel,
                    messages: vec![message]
                },
                now
            ),
            rune_state::Applied::Nothing
        ));
    }

    #[test]
    fn context_host_denial_ends_loading_without_trusting_ui_availability() {
        for source in [
            rune_model::HistorySource::Provider,
            rune_model::HistorySource::Cached,
        ] {
            let (ctx, mut app, channel, mut message) = pins_host();
            let now = Instant::now();
            let _ = rune_ui::local_history_changed(&mut app.ui, &app.store, true);
            let anchor = rune_model::MessageId(u64::MAX - 100);
            let intents = rune_ui::open_message(&mut app.ui, &app.store, channel, anchor, source);
            let request = intents
                .iter()
                .find_map(|intent| match intent {
                    Intent::LoadContext { request, .. } => Some(*request),
                    _ => None,
                })
                .expect("the UI starts an exact context request");
            if source == rune_model::HistorySource::Provider {
                app.store.connection = rune_provider::ConnectionState::Disconnected {
                    reason: "synthetic disconnect".into(),
                };
            }
            app.run(&ctx, intents);
            app.tree
                .set_text_measurer(|text, size, _| text.chars().count() as f32 * size * 0.5);
            let window = Size::new(1280.0, 800.0);
            app.tree.set_root(rune_ui::view(
                &app.ui,
                &app.store,
                &app.context(window, now),
            ));
            let frame = app.tree.layout(window);
            assert!(
                frame
                    .regions
                    .iter()
                    .any(|region| region.id == "context:retry"),
                "host denial must terminate Loading for {source:?}"
            );
            assert!(
                app.ui.notices.is_empty(),
                "context status owns the exact failure"
            );
            message.id = anchor;
            assert!(matches!(
                app.store.apply(
                    rune_provider::Event::ContextLoaded {
                        request,
                        channel,
                        anchor,
                        messages: vec![message]
                    },
                    now
                ),
                rune_state::Applied::Nothing
            ));
        }
    }

    #[test]
    fn pending_settings_retry_the_latest_value_after_backpressure() {
        let mut pending = PendingStorage::default();
        pending.set_setting("theme".into(), Some("old".into()));
        assert_eq!(
            pending.submit_settings(|_, _| Err(StorageWriteError::Busy)),
            Err(StorageWriteError::Busy)
        );
        pending.set_setting("theme".into(), Some("new".into()));
        let mut saved = Vec::new();
        pending
            .submit_settings(|key, value| {
                saved.push((key.to_owned(), value));
                Ok(())
            })
            .unwrap();
        assert_eq!(saved, vec![("theme".into(), Some("new".into()))]);
        assert!(pending.settings.is_empty());
    }

    #[test]
    fn closing_waits_for_dirty_values_backpressure_and_the_commit_watermark() {
        let mut pending = PendingStorage::default();
        pending.begin_close([("zoom".into(), Some("125".into()))]);
        let mut flushes = 0;
        let mut flush = || {
            flushes += 1;
            Ok(Some(StorageTicket(7)))
        };
        assert!(
            !pending
                .advance_barrier(true, &mut flush, false)
                .unwrap()
                .close
        );
        pending.submit_settings(|_, _| Ok(())).unwrap();
        assert_eq!(
            pending.advance_barrier(false, || Err(StorageWriteError::Busy), false),
            Err(StorageWriteError::Busy)
        );
        assert!(
            !pending
                .advance_barrier(false, &mut flush, false)
                .unwrap()
                .close
        );
        assert!(
            !pending
                .advance_barrier(false, || panic!("already queued"), false)
                .unwrap()
                .close
        );
        assert!(
            pending
                .advance_barrier(false, || panic!("already queued"), true)
                .unwrap()
                .close
        );
        assert_eq!(flushes, 1);
        assert!(pending.closed);
    }

    #[test]
    fn forgetting_is_confirmed_after_commit_and_cancelled_close_keeps_pending_values() {
        let mut pending = PendingStorage {
            forget: true,
            ..PendingStorage::default()
        };
        assert_eq!(
            pending.submit_forget(|| Err(StorageWriteError::Busy)),
            Err(StorageWriteError::Busy)
        );
        assert!(pending.forget);
        pending.submit_forget(|| Ok(())).unwrap();
        assert!(
            !pending
                .advance_barrier(false, || Ok(Some(StorageTicket(9))), false)
                .unwrap()
                .forgot
        );
        pending.begin_close([("theme".into(), Some("onyx".into()))]);
        pending.cancel_close();
        assert!(!pending.closing);
        assert!(pending.settings.contains_key("theme"));
        pending.submit_settings(|_, _| Ok(())).unwrap();
        assert!(
            !pending
                .advance_barrier(false, || Ok(Some(StorageTicket(10))), false)
                .unwrap()
                .forgot
        );
        let completed = pending
            .advance_barrier(false, || panic!("already queued"), true)
            .unwrap();
        assert!(completed.forgot);
        assert!(!completed.close);
    }

    #[test]
    fn closing_without_storage_does_not_wait_for_a_nonexistent_ticket() {
        let mut pending = PendingStorage::default();
        pending.begin_close([]);
        assert!(
            pending
                .advance_barrier(false, || Ok(None), false)
                .unwrap()
                .close
        );
    }

    #[test]
    fn a_preference_changed_while_closing_requires_a_new_barrier() {
        let mut pending = PendingStorage::default();
        pending.begin_close([]);
        assert!(
            !pending
                .advance_barrier(false, || Ok(Some(StorageTicket(1))), false)
                .unwrap()
                .close
        );
        pending.set_setting("custom_status_until".into(), None);
        pending.submit_settings(|_, _| Ok(())).unwrap();
        assert!(
            !pending
                .advance_barrier(false, || Ok(Some(StorageTicket(2))), true)
                .unwrap()
                .close
        );
        assert!(
            !pending
                .advance_barrier(false, || panic!("already queued"), false)
                .unwrap()
                .close
        );
        assert!(
            pending
                .advance_barrier(false, || panic!("already queued"), true)
                .unwrap()
                .close
        );
    }

    #[test]
    fn text_and_paste_received_with_a_native_close_are_kept_before_input_freezes() {
        for event in [
            egui::Event::Text("última tecla".into()),
            egui::Event::Paste("último pegado".into()),
        ] {
            let expected = match &event {
                egui::Event::Text(text) | egui::Event::Paste(text) => text.clone(),
                _ => unreachable!(),
            };
            let ctx = egui::Context::default();
            let now = Instant::now();
            let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
            let window = Size::new(area.width(), area.height());
            let provider = Arc::new(MockProvider::new(MockConfig {
                ambient: false,
                replies: false,
                ..MockConfig::default()
            }));
            let snapshot = provider.snapshot();
            let channel = snapshot
                .channels
                .iter()
                .find(|channel| channel.name == "general")
                .unwrap()
                .clone();
            let mut app = App::new(
                &ctx,
                now,
                Arguments {
                    screenshot: Some("unused-headless-close.png".into()),
                    ..Arguments::default()
                },
                provider,
                None,
            );
            app.store
                .apply(rune_provider::Event::Ready(Box::new(snapshot)), now);
            let intents = rune_ui::open(&mut app.ui, &mut app.store, channel.server, channel.id);
            app.run(&ctx, intents);
            let context = app.context(window, now);
            app.tree
                .set_root(rune_ui::view(&app.ui, &app.store, &context));
            app.tree.layout(window);
            assert!(app.tree.focus(&rune_ui::editor_id(channel.id)));

            let mut input = egui::RawInput {
                events: vec![event],
                screen_rect: Some(area),
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
            let mut output = ctx.run_ui(input, |_| {
                limn_egui::normalize_keyboard_input(&ctx);
                let requested = app.receive_close_request(&ctx);
                assert!(requested);
                app.handle_input(&ctx, area, now, requested, false);
            });
            assert!(output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::CancelClose));
            output.textures_delta.clear();
            assert!(app.pending_storage.closing);
            assert_eq!(app.ui.drafts.get(&channel.id), Some(&expected));
            let mut kept = Vec::new();
            keep_drafts(&mut app.ui, |channel, text| {
                kept.push((channel, text));
                Ok(())
            })
            .unwrap();
            assert_eq!(kept, vec![(channel.id, Some(expected.clone()))]);

            ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Text("ignorado".into())],
                    ..Default::default()
                },
                |_| {
                    app.handle_input(&ctx, area, now, false, false);
                },
            )
            .textures_delta
            .clear();
            assert_eq!(
                app.ui.drafts.get(&channel.id),
                Some(&expected),
                "later frames stay frozen"
            );
        }
    }

    #[test]
    fn captures_wait_for_mock_data_but_not_for_servers_during_sign_in() {
        let mut store = Store::new();
        assert!(!capture_ready(&store));
        store.apply(
            rune_provider::Event::Authentication(AuthenticationState::SignedOut),
            Instant::now(),
        );
        assert!(capture_ready(&store));
        assert!(store.servers().is_empty());
    }

    fn args(values: &[&str], id: Option<&str>) -> Result<Arguments, String> {
        parse_arguments(
            values.iter().map(|value| (*value).to_owned()),
            id.map(str::to_owned),
        )
    }

    #[test]
    fn provider_selection_defaults_to_mock_and_real_auth_requires_an_application() {
        assert_eq!(args(&[], None).unwrap().provider, ProviderChoice::Mock);
        assert!(args(&["--provider", "discord"], None).is_err());
        assert!(args(&["--provider", "discord", "--discord-client-id"], None).is_err());
        assert!(args(&["--provider", "unknown"], None).is_err());
        assert!(args(&["--provider"], None).is_err());
        let configured = args(&["--provider", "discord"], Some("123456789012345678")).unwrap();
        assert_eq!(
            configured.discord_client_id.as_deref(),
            Some("123456789012345678")
        );
        let configured = args(
            &[
                "--provider",
                "discord",
                "--discord-client-id",
                "234567890123456789",
            ],
            Some("123456789012345678"),
        )
        .unwrap();
        assert_eq!(
            configured.discord_client_id.as_deref(),
            Some("234567890123456789")
        );
    }

    #[test]
    fn the_provider_is_named_by_its_key_and_says_why_it_cannot_start() {
        for choice in ProviderChoice::ALL {
            assert_eq!(ProviderChoice::from_key(choice.key()), Some(choice));
            assert_eq!(choice.option(Some("123")).key, choice.key());
        }
        assert!(args(&["--provider", "mock"], None).unwrap().provider_given);
        assert!(!args(&[], None).unwrap().provider_given);
        // Nothing remembered: the login in a build that has it, never the
        // simulation by itself; the simulation's own options keep it.
        let login = cfg!(all(
            feature = "session-login",
            any(target_os = "linux", windows)
        ));
        let first = unremembered_provider(&args(&[], None).unwrap());
        assert_eq!(first == ProviderChoice::DiscordSession, login);
        assert_eq!(
            unremembered_provider(&args(&["--data"], None).unwrap()),
            ProviderChoice::Mock
        );
        assert!(ProviderChoice::Discord.unavailable(None).is_some());
        assert!(ProviderChoice::Discord.unavailable(Some(" ")).is_some());
        assert!(ProviderChoice::Discord.unavailable(Some("123")).is_none());
        assert_eq!(
            ProviderChoice::DiscordSession.unavailable(None).is_none(),
            cfg!(all(
                feature = "session-login",
                any(target_os = "linux", windows)
            ))
        );
        // Each one says where its data comes from, never a credential.
        let kinds: Vec<_> = ProviderChoice::ALL
            .into_iter()
            .map(|choice| choice.option(None).kind)
            .collect();
        assert_eq!(
            kinds,
            [
                rune_provider::ProviderKind::Simulated,
                rune_provider::ProviderKind::Official,
                rune_provider::ProviderKind::Unofficial
            ]
        );
    }

    #[test]
    fn the_login_window_closing_done_or_cancelled_asks_for_the_window_back() {
        use rune_provider::AuthenticationState as State;
        assert!(login_closed(&State::AwaitingExternal, &State::Exchanging));
        assert!(login_closed(&State::AwaitingExternal, &State::SignedOut));
        assert!(login_closed(
            &State::AwaitingExternal,
            &State::Failed { reason: "x".into() }
        ));
        assert!(!login_closed(&State::SignedOut, &State::AwaitingExternal));
        assert!(!login_closed(
            &State::Exchanging,
            &State::SignedIn { scopes: vec![] }
        ));
        assert!(!login_closed(
            &State::AwaitingExternal,
            &State::AwaitingExternal
        ));
    }

    #[test]
    fn what_was_kept_of_a_real_account_shows_before_ready_and_never_as_confirmed() {
        let ctx = egui::Context::default();
        let now = Instant::now();
        let provider = Arc::new(MockProvider::new(MockConfig {
            latency: Duration::ZERO,
            ambient: false,
            replies: false,
            ..MockConfig::default()
        }));
        let snapshot = provider.snapshot();
        let account = snapshot.me.as_ref().unwrap().id;
        let channel = snapshot
            .channels
            .iter()
            .find(|channel| channel.name == "general")
            .unwrap()
            .id;
        let mark = rune_model::ReadMark {
            through: Some(rune_model::MessageId(1)),
            unread_from: None,
            revision: 3,
        };
        let restored = |snapshot| rune_core::Restored {
            account,
            snapshot,
            drafts: vec![(channel, "a medio escribir".into())],
            read_marks: Ok(vec![(channel, mark)]),
            draft_files: Vec::new(),
        };
        let mut app = App::new(
            &ctx,
            now,
            Arguments {
                screenshot: Some("unused-headless-restored.png".into()),
                ..Arguments::default()
            },
            provider.clone(),
            None,
        );
        assert!(rune_ui::splash::visible(&app.store));
        let applied = app.restored(restored(Some(snapshot.clone())));
        assert!(matches!(applied, rune_state::Applied::Ready));
        // The interface instead of the opening screen, as the cache's.
        assert!(!rune_ui::splash::visible(&app.store));
        assert!(app.store.from_cache);
        assert_eq!(app.store.me(), Some(account));
        assert_ne!(
            app.store.connection,
            rune_provider::ConnectionState::Connected
        );
        assert_eq!(
            app.ui.drafts.get(&channel).map(String::as_str),
            Some("a medio escribir")
        );
        // READY confirms it.
        app.store
            .apply(rune_provider::Event::Ready(Box::new(snapshot.clone())), now);
        assert!(!app.store.from_cache);
        // Kept after READY (a new sign-in): no snapshot replaces the session's,
        // and a draft written meanwhile stays.
        app.ui.drafts.insert(channel, "escrito ahora".into());
        let applied = app.restored(restored(Some(snapshot)));
        assert!(matches!(applied, rune_state::Applied::Nothing));
        assert!(!app.store.from_cache);
        assert_eq!(
            app.ui.drafts.get(&channel).map(String::as_str),
            Some("escrito ahora")
        );
        assert!(app.read_marks_to_restore.is_none(), "taken at once");
    }

    // Linux desktop (escritorio-linux).
    #[test]
    fn a_replace_nothing_waits_for_ends_the_save_instead_of_saving_forever() {
        let (ctx, mut app, _, _) = pins_host();
        // As `--action capture:save-exists` leaves it: Core never started it.
        app.ui.save_as = Some(rune_ui::SaveAs {
            request: 7,
            account: app.store.me(),
            filename: "foto.png".into(),
            phase: rune_ui::SavePhase::Exists,
        });
        app.run(&ctx, vec![Intent::ReplaceSavedFile { request: 7 }]);
        assert_eq!(app.ui.save_as, None);
        let notice = app.ui.notices.last().unwrap();
        assert!(notice.error && notice.title == "No se pudo guardar el archivo");
    }

    #[test]
    fn switching_the_provider_clears_the_account_and_keeps_the_window() {
        let (ctx, mut app, channel, _) = pins_host();
        app.ui.drafts.insert(channel, "a medio escribir".into());
        app.ui.prefs.developer = true;
        // Linux desktop (escritorio-linux): read once, before the window opened.
        let graphics = rune_ui::desktop::Graphics {
            gpus: vec![("nvidia".into(), "NVIDIA".into())],
            chosen: Some("nvidia".into()),
            ..Default::default()
        };
        app.attach_window(
            window::Parent::default(),
            graphics.clone(),
            rune_ui::title_bar::TitleBar::Rune,
        );
        // Without its application ID, OAuth2 does not start: nothing changes.
        app.switch_provider(&ctx, "discord");
        assert_eq!(app.ui.providers.current, Some("mock"));
        assert!(!app.store.servers().is_empty());
        app.run(&ctx, vec![Intent::UseProvider("mock")]);
        assert!(
            app.store.servers().is_empty(),
            "a new store, until it is ready"
        );
        assert!(app.ui.drafts.is_empty(), "the drafts were of the other one");
        assert!(app.ui.prefs.developer, "the window's own settings stay");
        assert_eq!(
            app.ui.desktop.title_bar,
            rune_ui::title_bar::TitleBar::Rune,
            "and what the host read of it"
        );
        assert_eq!(app.ui.desktop.graphics, graphics);
        assert_eq!(app.ui.providers.options.len(), 3);
        assert_eq!(app.ui.providers.current, Some("mock"));
        assert_eq!(
            app.store.provider.as_ref().map(|info| info.kind),
            Some(rune_provider::ProviderKind::Simulated)
        );
        assert!(
            !app.local_data,
            "a capture keeps nothing, the choice neither"
        );
    }

    #[test]
    fn simulation_options_cannot_claim_to_benchmark_a_real_service() {
        assert!(args(
            &["--provider", "discord", "--bench"],
            Some("123456789012345678")
        )
        .is_err());
        assert!(args(
            &["--provider", "discord", "--latency", "25"],
            Some("123456789012345678")
        )
        .is_err());
        assert!(args(&["--bench"], None).is_ok());
        assert!(args(&["--provider", "discord-session", "--bench"], None).is_err());
        assert!(args(&["--provider", "discord-session", "--latency", "25"], None).is_err());
    }

    #[test]
    fn a_run_may_choose_the_interface_s_language() {
        assert_eq!(
            args(&["--language", "en"], None).unwrap().language,
            Some(rune_i18n::Language::English)
        );
        assert_eq!(args(&[], None).unwrap().language, None);
        assert!(args(&["--language", "pt"], None).is_err());
        assert!(args(&["--language"], None).is_err());
    }

    #[test]
    fn synthetic_capture_input_is_limited_to_mock_screenshots() {
        for option in [
            vec!["--capture-hover"],
            vec!["--capture-drop"],
            vec!["--capture-attachments"],
            vec!["--capture-viewer"],
            vec!["--capture-animation"],
            vec!["--capture-invitation"],
            vec!["--capture-onboarding"],
            vec!["--capture-rich"],
            vec!["--capture-voice"],
            vec!["--capture-context"],
            vec!["--capture-draft", "Una línea"],
        ] {
            assert!(args(&option, None).is_err());
            let mut capture = vec!["--screenshot", "mock.png"];
            capture.extend_from_slice(&option);
            assert!(args(&capture, None).is_ok());
            capture.extend_from_slice(&["--provider", "discord-session"]);
            assert!(args(&capture, None).is_err());
        }
        assert!(args(&["--screenshot", "mock.png", "--capture-draft"], None).is_err());
        assert!(args(&["--screenshot", "mock.png", "--capture-search", "a"], None).is_ok());
        let search = [
            "--screenshot",
            "mock.png",
            "--data",
            "--capture-search",
            "a",
        ];
        assert!(args(&search, None).is_ok());
        let mut real = search.to_vec();
        real.extend_from_slice(&["--provider", "discord-session"]);
        assert!(args(&real, None).is_err());
        let mut incompatible = search.to_vec();
        incompatible.push("--capture-context");
        assert!(args(&incompatible, None).is_err());
    }

    #[test]
    fn unofficial_login_is_explicit_and_does_not_use_an_oauth_application_id() {
        let configured = args(&["--provider", "discord-session"], None).unwrap();
        assert_eq!(configured.provider, ProviderChoice::DiscordSession);
        assert!(configured.discord_client_id.is_none());
        #[cfg(not(all(feature = "session-login", any(target_os = "linux", windows))))]
        assert!(selected_provider(&configured).is_err());
    }

    fn pass(ctx: &egui::Context, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        // Headless: no renderer takes the font atlas.
        ctx.run_ui(input, |_| {}).textures_delta.clear();
    }

    #[test]
    fn the_zoom_setting_reaches_egui_and_ctrl_plus_comes_back_to_it() {
        let ctx = egui::Context::default();
        let mut applied = 1.0;
        let mut prefs = rune_ui::Prefs {
            zoom: 125,
            ..Default::default()
        };
        pass(&ctx, Vec::new());
        assert_eq!(follow_zoom(&ctx, &mut applied, &mut prefs), None);
        pass(&ctx, Vec::new());
        assert!(
            (ctx.zoom_factor() - 1.25).abs() < 0.001,
            "the setting is applied"
        );
        let plus = egui::Event::Key {
            key: egui::Key::Equals,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        pass(&ctx, vec![plus.clone()]);
        pass(&ctx, Vec::new());
        let zoomed = follow_zoom(&ctx, &mut applied, &mut prefs);
        assert!(zoomed.is_some_and(|zoom| zoom > 125), "{zoomed:?}");
        assert_eq!(Some(prefs.zoom), zoomed);
        // Past the setting's range, egui is held back to it.
        prefs.zoom = 200;
        follow_zoom(&ctx, &mut applied, &mut prefs);
        pass(&ctx, Vec::new());
        for _ in 0..3 {
            pass(&ctx, vec![plus.clone()]);
        }
        pass(&ctx, Vec::new());
        assert_eq!(follow_zoom(&ctx, &mut applied, &mut prefs), Some(200));
        pass(&ctx, Vec::new());
        assert!((ctx.zoom_factor() - 2.0).abs() < 0.001);
    }
}
