//! RUNE's own updates on the desktop's side (actualizaciones, 2026-10-19).
//!
//! Before anything opens, [`startup`] looks for a newer RUNE, waiting at
//! most [`STARTUP_WAIT`], when this copy updates by itself (RUNE's
//! installer put it in place, or `RUNE_UPDATE_FEED` names a test feed) and
//! the person did not turn it off. A mandatory release runs [`mandatory`]:
//! a window of its own that downloads, installs and starts the new RUNE,
//! or says why not with «Reintentar» and «Salir»; the main window never
//! opens with a version the service no longer accepts. An optional one is
//! handed to the interface ([`Updates`]), which offers it without
//! blocking. A check that fails or takes too long opens RUNE as it is.
//! Network and disk run on threads of their own (`rune_update`).

use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use limn::{Element, Size, UiAction, UiTree};
use rune_components::updates::{ids, Stage, UpdateView};
use rune_i18n::t;
use rune_model::Timestamp;
use rune_update::install::Install;
use rune_update::{Error, Feed, Release, Version};

/// How long a start waits for the check before opening anyway.
const STARTUP_WAIT: Duration = Duration::from_secs(4);
/// Kept before the window opens: `off` once the person turned checks at
/// start off.
const AT_START: &str = "updates";
/// Kept: when the last check ended, in milliseconds since the epoch.
const CHECKED: &str = "update-checked";

/// The installation this copy updates: the one it runs from, or with a
/// test feed (`RUNE_UPDATE_FEED`) the one `RUNE_UPDATE_ROOT` or the
/// default names. `None` for a build from source.
pub fn installation() -> Option<Install> {
    let install = Install::locate()?;
    let running = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .is_ok_and(|program| install.runs(&program));
    (running || Feed::chosen()).then_some(install)
}

/// The program a launcher or the session's autostart should start: the
/// installation's, which stays the same across updates, or this one.
pub fn program() -> std::io::Result<PathBuf> {
    match Install::locate() {
        Some(install)
            if std::env::current_exe()
                .and_then(std::fs::canonicalize)
                .is_ok_and(|program| install.runs(&program)) =>
        {
            Ok(install.program())
        }
        _ => std::env::current_exe(),
    }
}

/// Look when RUNE starts, or no longer, from the next start.
pub fn set_at_start(on: bool) {
    crate::window::remember(AT_START, (!on).then(|| "off".to_owned()));
}

fn at_start() -> bool {
    crate::window::remembered(AT_START).as_deref() != Some("off")
}

fn now() -> Timestamp {
    rune_provider::mock::wall_clock()
}

/// Look for a newer RUNE, here and now: call it off the interface's
/// thread. `keep`: remember when, as an interactive run does.
fn check(keep: bool) -> Result<Option<Release>, Error> {
    let client = rune_update::client()?;
    let checked = rune_update::block_on(rune_update::check(
        &client,
        &Feed::from_env(),
        rune_update::public_key().as_ref(),
    ));
    if keep {
        crate::window::remember(CHECKED, Some(now().0.to_string()));
    }
    let installed = Version::parse(rune_update::VERSION).expect("the workspace's version");
    let offered = rune_update::offer(checked?, &installed, &rune_update::platform());
    // On Windows the installer that put this version in place is still in
    // the downloads (it cannot delete itself): up to date, nothing there is
    // needed any more.
    #[cfg(windows)]
    if let (None, Some(install)) = (&offered, installation()) {
        let _ = std::fs::remove_dir_all(install.downloads());
    }
    Ok(offered)
}

/// What a check found before the window opened.
#[derive(Default)]
pub struct Startup {
    pub found: Option<Result<Option<Release>, Error>>,
}

impl Startup {
    fn mandatory(&self) -> Option<&Release> {
        match &self.found {
            Some(Ok(Some(release))) if release.mandatory => Some(release),
            _ => None,
        }
    }
}

/// Before anything opens: look for a newer RUNE, if this copy does. A
/// capture looks only at a test feed; the benchmark never does.
pub fn startup(capture: bool, bench: bool) -> Startup {
    let looks = if capture {
        Feed::chosen()
    } else {
        !bench && at_start() && installation().is_some()
    };
    if !looks {
        return Startup::default();
    }
    let (sender, found) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(check(!capture));
    });
    let found = found.recv_timeout(STARTUP_WAIT).ok();
    if found.is_none() {
        eprintln!("rune: update: no answer at start; opening as it is");
    }
    if let Some(Err(error)) = &found {
        eprintln!("rune: update: {error:?} at start; opening as it is");
    }
    Startup { found }
}

/// What a job reports to the window that started it.
enum Report {
    Checked(Result<Option<Release>, Error>),
    Progress(u64, u64),
    Installing,
    /// Installed: start this program.
    Installed(PathBuf),
    Failed(Error),
}

/// Download `release` into `install` and, when `apply`, install it,
/// reporting as it goes; on a thread of its own.
fn install_job(
    release: &Release,
    install: &Install,
    apply: bool,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> mpsc::Receiver<Report> {
    let (sender, reports) = mpsc::channel();
    let (artifact, version, install) = (
        release.artifact.clone(),
        release.version.clone(),
        install.clone(),
    );
    std::thread::spawn(move || {
        let tell = |report| {
            let _ = sender.send(report);
            wake();
        };
        let result = (|| {
            let artifact = artifact.ok_or(Error::NoArtifact)?;
            let client = rune_update::client()?;
            let mut said = Instant::now();
            let file = rune_update::block_on(rune_update::download(
                &client,
                &artifact,
                &install.downloads(),
                |have, whole| {
                    // A frame every tenth of a second is enough to show it.
                    if have == whole || said.elapsed() >= Duration::from_millis(100) {
                        said = Instant::now();
                        tell(Report::Progress(have, whole));
                    }
                },
            ))?;
            // A capture only downloads; its window ignores this one.
            if !apply {
                return Err(Error::Incomplete);
            }
            tell(Report::Installing);
            apply_release(&install, &file, &version)
        })();
        tell(match result {
            Ok(program) => Report::Installed(program),
            Err(error) => Report::Failed(error),
        });
    });
    reports
}

/// Put the downloaded release in place, returning what [`restart`] runs.
/// On Linux, the installation's folders, all or nothing, and then the new
/// program. On Windows nothing changes yet: the installer runs once RUNE
/// has closed, so it never replaces files RUNE still uses.
fn apply_release(install: &Install, file: &Path, version: &Version) -> Result<PathBuf, Error> {
    #[cfg(unix)]
    {
        let program = install.apply(file, version).map_err(|error| {
            eprintln!("rune: update: could not install: {error}");
            Error::Disk
        })?;
        let _ = std::fs::remove_dir_all(install.downloads());
        Ok(program)
    }
    #[cfg(windows)]
    {
        let _ = (install, version);
        Ok(file.to_owned())
    }
}

/// The program to start once this one has closed, and its arguments.
static RESTART: Mutex<Option<(PathBuf, Vec<std::ffi::OsString>)>> = Mutex::new(None);

/// Start `program` with `arguments` once the window has closed.
fn restart_after_exit(program: PathBuf, arguments: Vec<std::ffi::OsString>) {
    *RESTART.lock().unwrap() = Some((program, arguments));
}

/// The program an update installed and its arguments, once the window
/// has closed.
pub fn installed() -> Option<(PathBuf, Vec<std::ffi::OsString>)> {
    RESTART.lock().unwrap().take()
}

/// Become the program an update installed. On Linux the process is
/// replaced, so the claim on being the only RUNE passes on whole (stderr
/// must be the terminal's again: the log's thread does not survive). On
/// Windows `program` is the installer: it runs silently and starts the new
/// RUNE once it is done.
pub fn restart((program, arguments): (PathBuf, Vec<std::ffi::OsString>)) -> ! {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = std::process::Command::new(&program).args(arguments).exec();
        eprintln!(
            "rune: update: installed, but could not start {}: {error}",
            program.display()
        );
        std::process::exit(1);
    }
    #[cfg(windows)]
    {
        let _ = arguments;
        if let Err(error) = rune_update::install::run_installer(&program) {
            eprintln!("rune: update: could not start the installer: {error}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }
}

fn view_of(release: &Release, stage: Stage) -> UpdateView {
    UpdateView {
        version: release.version.to_string(),
        installed: rune_update::VERSION.to_owned(),
        notes: release.notes.clone(),
        stage,
    }
}

/// Updates while the main window is open: checks asked for in the
/// settings, and installing the one offered.
pub struct Updates {
    /// Checks are remembered: not in a capture.
    keep: bool,
    install: Option<Install>,
    release: Option<Release>,
    job: Option<mpsc::Receiver<Report>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Updates {
    /// Hand the interface what the start found, and what is kept.
    pub fn new(
        startup: Startup,
        keep: bool,
        ui: &mut rune_ui::Ui,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let install = installation();
        let state = &mut ui.updates;
        state.installation = install
            .as_ref()
            .map(|install| install.root().display().to_string());
        state.at_start = at_start();
        state.checked = crate::window::remembered(CHECKED)
            .and_then(|millis| millis.parse().ok())
            .map(Timestamp);
        let mut updates = Self {
            keep,
            install,
            release: None,
            job: None,
            wake,
        };
        if let Some(found) = startup.found {
            updates.found(found, ui);
        }
        updates
    }

    fn found(&mut self, found: Result<Option<Release>, Error>, ui: &mut rune_ui::Ui) {
        let state = &mut ui.updates;
        state.checking = false;
        state.checked = Some(now());
        match found {
            Ok(release) => {
                state.error = None;
                state.offered = release
                    .as_ref()
                    .map(|release| view_of(release, Stage::Offered));
                self.release = release;
            }
            Err(error) => state.error = Some(error.label()),
        }
    }

    /// «Buscar ahora».
    pub fn check(&mut self, ui: &mut rune_ui::Ui) {
        if self.job.is_some() {
            return;
        }
        ui.updates.checking = true;
        let (sender, reports) = mpsc::channel();
        let (wake, keep) = (self.wake.clone(), self.keep);
        std::thread::spawn(move || {
            let _ = sender.send(Report::Checked(check(keep)));
            wake();
        });
        self.job = Some(reports);
    }

    /// «Actualizar y reiniciar».
    pub fn install(&mut self, ui: &mut rune_ui::Ui) {
        let (Some(release), Some(install)) = (&self.release, &self.install) else {
            return;
        };
        if self.job.is_some() {
            return;
        }
        self.job = Some(install_job(release, install, true, self.wake.clone()));
        ui.updates.offered = Some(view_of(release, Stage::Downloading { have: 0, whole: 0 }));
    }

    /// What the job reported since the last frame. Returns `true` once the
    /// update is installed and RUNE should close to start it.
    pub fn step(&mut self, ui: &mut rune_ui::Ui) -> bool {
        let Some(job) = &self.job else {
            return false;
        };
        let reports: Vec<Report> = job.try_iter().collect();
        for report in reports {
            let stage = match report {
                Report::Checked(found) => {
                    self.job = None;
                    self.found(found, ui);
                    continue;
                }
                Report::Progress(have, whole) => Stage::Downloading { have, whole },
                Report::Installing => Stage::Installing,
                Report::Installed(program) => {
                    self.job = None;
                    restart_after_exit(program, Vec::new());
                    return true;
                }
                Report::Failed(error) => {
                    self.job = None;
                    Stage::Failed(error.label())
                }
            };
            if let Some(release) = &self.release {
                ui.updates.offered = Some(view_of(release, stage));
            }
        }
        false
    }
}

/// The window of a mandatory update, instead of RUNE's: it downloads and
/// installs at once, and starts the new RUNE when it is done. With
/// `capture`, it saves a picture of itself a moment in and closes,
/// installing nothing.
pub fn mandatory(startup: &Startup, capture: Option<PathBuf>) -> Option<eframe::Result> {
    let release = startup.mandatory()?.clone();
    let install = installation()?;
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("RUNE")
            .with_app_id("rune")
            .with_icon(crate::frame::icon())
            .with_inner_size([720.0, 560.0])
            .with_min_inner_size([480.0, 420.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "RUNE",
        options,
        Box::new(move |creation| {
            limn_egui::install_fonts(&creation.egui_ctx);
            limn_egui::set_icon_pack(rune_components::glyphs::pack);
            let wake_ctx = creation.egui_ctx.clone();
            let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || wake_ctx.request_repaint());
            let job = install_job(&release, &install, capture.is_none(), wake.clone());
            Ok(Box::new(MandatoryWindow {
                tree: UiTree::new(Element::spacer()),
                fonts_attached: false,
                view: view_of(&release, Stage::Offered),
                release,
                install,
                job: Some(job),
                wake,
                capture: capture.map(|path| (path, Instant::now(), false)),
            }))
        }),
    );
    if let Some(installed) = installed() {
        restart(installed);
    }
    Some(result)
}

struct MandatoryWindow {
    tree: UiTree,
    fonts_attached: bool,
    view: UpdateView,
    release: Release,
    install: Install,
    job: Option<mpsc::Receiver<Report>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    /// Where to save the picture, since when, and whether it was asked.
    capture: Option<(PathBuf, Instant, bool)>,
}

impl MandatoryWindow {
    fn drain(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.job else { return };
        let reports: Vec<Report> = job.try_iter().collect();
        for report in reports {
            self.view.stage = match report {
                Report::Progress(have, whole) => Stage::Downloading { have, whole },
                Report::Installing => Stage::Installing,
                Report::Installed(program) => {
                    self.job = None;
                    // The links and options this start was given go on.
                    restart_after_exit(program, std::env::args_os().skip(1).collect());
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                // A capture installs nothing: it stops once downloaded.
                Report::Failed(Error::Incomplete) if self.capture.is_some() => continue,
                Report::Failed(error) => {
                    self.job = None;
                    Stage::Failed(error.label())
                }
                Report::Checked(_) => continue,
            };
        }
    }

    fn clicked(&mut self, ctx: &egui::Context, action: &UiAction) {
        let UiAction::Clicked { id } = action else {
            return;
        };
        match id.as_str() {
            ids::RETRY if self.job.is_none() => {
                self.view.stage = Stage::Offered;
                self.job = Some(install_job(
                    &self.release,
                    &self.install,
                    self.capture.is_none(),
                    self.wake.clone(),
                ));
            }
            ids::QUIT => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            _ => {}
        }
    }

    fn capture_step(&mut self, ctx: &egui::Context) {
        let Some((path, since, requested)) = &mut self.capture else {
            return;
        };
        if !*requested && since.elapsed() >= Duration::from_millis(1_500) {
            *requested = true;
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
                &*path,
                image.as_raw(),
                width as u32,
                height as u32,
                image::ColorType::Rgba8,
            ) {
                Ok(()) => println!("rune: saved {}", path.display()),
                Err(error) => eprintln!("rune: could not save {}: {error}", path.display()),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

impl eframe::App for MandatoryWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // The language of the session: the settings are not read yet.
        if !self.fonts_attached {
            limn_egui::attach_fonts(&mut self.tree, &ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                t("Actualización obligatoria").into(),
            ));
            self.fonts_attached = true;
        }
        limn_egui::normalize_keyboard_input(&ctx);
        self.drain(&ctx);
        let area = ui.max_rect();
        let window = Size::new(area.width(), area.height());
        let look = rune_components::Look::ONYX;
        self.tree
            .set_root(rune_components::updates::screen(&self.view, window, &look));
        self.tree.layout(window);
        for event in limn_egui::events(&ctx, area.min) {
            for action in self.tree.handle_event(event) {
                self.clicked(&ctx, &action);
            }
        }
        for action in limn_egui::apply_access_requests(&ctx, &mut self.tree) {
            self.clicked(&ctx, &action);
        }
        self.tree
            .set_root(rune_components::updates::screen(&self.view, window, &look));
        let frame = self.tree.layout(window);
        limn_egui::paint_at(ui, &frame, area.min);
        if let Some(after) = self.tree.animation_repaint_after() {
            ctx.request_repaint_after(after);
        }
        self.capture_step(&ctx);
    }
}

/// RUNE's icon as `rune --install` puts it in the theme of icons.
#[cfg(unix)]
const ICONS: [(u32, &[u8]); 8] = [
    (
        16,
        include_bytes!("../../../assets/icons/hicolor/16x16/apps/rune.png"),
    ),
    (
        24,
        include_bytes!("../../../assets/icons/hicolor/24x24/apps/rune.png"),
    ),
    (
        32,
        include_bytes!("../../../assets/icons/hicolor/32x32/apps/rune.png"),
    ),
    (
        48,
        include_bytes!("../../../assets/icons/hicolor/48x48/apps/rune.png"),
    ),
    (
        64,
        include_bytes!("../../../assets/icons/hicolor/64x64/apps/rune.png"),
    ),
    (
        128,
        include_bytes!("../../../assets/icons/hicolor/128x128/apps/rune.png"),
    ),
    (
        256,
        include_bytes!("../../../assets/icons/hicolor/256x256/apps/rune.png"),
    ),
    (
        512,
        include_bytes!("../../../assets/icons/hicolor/512x512/apps/rune.png"),
    ),
];

/// `rune --install` (Linux): this program becomes the installation's
/// first version, with a launcher in `~/.local/bin`, an entry in the
/// applications menu and its icon. Returns the exit code.
#[cfg(unix)]
pub fn install_self() -> i32 {
    let result = (|| -> std::io::Result<PathBuf> {
        let not_found = || std::io::Error::new(std::io::ErrorKind::NotFound, "no home folder");
        let install = Install::locate().ok_or_else(not_found)?;
        let version = Version::parse(rune_update::VERSION).expect("the workspace's version");
        let program = install.adopt(&std::env::current_exe()?, &version)?;
        let home = PathBuf::from(std::env::var_os("HOME").ok_or_else(not_found)?);
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|data| data.is_absolute())
            .unwrap_or_else(|| home.join(".local/share"));
        // A launcher, unless something else is there already.
        let launcher = home.join(".local/bin/rune");
        if std::fs::symlink_metadata(&launcher).is_err() {
            std::fs::create_dir_all(home.join(".local/bin"))?;
            std::os::unix::fs::symlink(&program, &launcher)?;
        }
        rune_platform::autostart::set_menu_entry(
            &data.join("applications/rune.desktop"),
            &program,
        )?;
        // RUNE's icon (barra-e-icono), drawn for each size the theme asks
        // for; the mark an older install left as scalable would win over it.
        let icons = data.join("icons/hicolor");
        for (size, png) in ICONS {
            let folder = icons.join(format!("{size}x{size}/apps"));
            std::fs::create_dir_all(&folder)?;
            std::fs::write(folder.join("rune.png"), png)?;
        }
        match std::fs::remove_file(icons.join("scalable/apps/rune.svg")) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        Ok(program)
    })();
    match result {
        Ok(program) => {
            println!(
                "rune: installed {} at {}",
                rune_update::VERSION,
                program.display()
            );
            0
        }
        Err(error) => {
            eprintln!("rune: could not install: {error}");
            1
        }
    }
}
