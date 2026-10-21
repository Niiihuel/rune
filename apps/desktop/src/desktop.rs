//! The desktop work (P6) on the host's side: the tray and hiding in it,
//! later starts of RUNE, the desktop's theme, the interface's font,
//! starting with the session, and what this device says the person is
//! doing (other programs' Rich Presence, games by their processes). Each
//! service runs on a thread of its own in `rune_platform`; this module
//! starts and stops them as the preferences say and turns what they report
//! into the interface's calls. Captures and the benchmark touch none of it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{mpsc, Arc};
use std::time::Instant;

use eframe::egui;
use limn::UiAction;
use rune_i18n::{es, tr};
use rune_model::{ActivitySource, LocalActivity, LocalActivityKind, Status, UserId};
use rune_platform::appearance::{Appearance, ColorScheme};
use rune_platform::fonts::Faces;
use rune_platform::games::{Game, GameWatcher, Playing};
use rune_platform::rich_presence::{self, PresenceEvent, RichPresence};
use rune_platform::tray::{Tray, TrayEvent, TrayState, TrayStatus};
use rune_state::{Applied, Store};
use rune_ui::games::RegisteredGame;
use rune_ui::{Intent, Ui};

/// The claim on being the only RUNE, and what later starts ask of it.
#[cfg(any(target_os = "linux", windows))]
pub type Instance = rune_platform::single_instance::Instance;
#[cfg(not(any(target_os = "linux", windows)))]
pub type Instance = std::convert::Infallible;
#[cfg(any(target_os = "linux", windows))]
type Listener = rune_platform::single_instance::Listener;
#[cfg(not(any(target_os = "linux", windows)))]
type Listener = std::convert::Infallible;

/// Links handed over by later starts, kept until an account is there.
const MAX_LINKS: usize = 8;
/// Programs whose activity is remembered at once.
const MAX_PROGRAMS: usize = 8;

pub struct Desktop {
    enabled: bool,
    wake: Arc<dyn Fn() + Send + Sync>,
    tray: Option<Tray>,
    appearance: Option<Appearance>,
    instance: Option<Listener>,
    links: Vec<String>,
    presence: Option<RichPresence>,
    presence_account: Option<UserId>,
    /// Each program's latest activity, by connection, with when it came:
    /// the newest wins, as Serein chooses.
    programs: BTreeMap<u64, (u64, LocalActivity)>,
    sequence: u64,
    games: Option<GameWatcher>,
    /// This session's list was asked for.
    games_asked: bool,
    playing: Option<Playing>,
    /// The activity the interface was told last.
    told: Option<Option<LocalActivity>>,
    /// The font installed: `None` is RUNE's own.
    font: Option<String>,
    font_loading: Option<(Option<String>, mpsc::Receiver<Option<Faces>>)>,
    fonts_listing: Option<mpsc::Receiver<Vec<String>>>,
    /// The fonts installed before the system's fallbacks, and those
    /// fallbacks (2026-10-17).
    base_fonts: Option<egui::FontDefinitions>,
    fallbacks: crate::fallback_fonts::Fallbacks,
    autostart: Option<mpsc::Receiver<(bool, bool, Option<String>)>>,
    /// The window is hidden in the tray.
    pub hidden: bool,
    /// «Salir» was chosen in the tray: closing really quits.
    pub quit: bool,
    // Desktop session and games (escritorio-sesion-y-juegos).
    /// KDE Plasma under Wayland: its scripting hides the window.
    #[cfg(target_os = "linux")]
    kwin: Option<rune_platform::kwin::KWin>,
    /// Started by the session in the tray: the window hides once it shows.
    starting_hidden: bool,
    programs_listing: Option<mpsc::Receiver<Vec<rune_platform::games::Program>>>,
    /// What the watcher was given: whether with the provider's list, and
    /// the revision of the person's.
    watched: Option<(bool, u64)>,
}

impl Desktop {
    /// The services of an interactive run; `enabled` false for captures and
    /// the benchmark, which start none. `links` were given on the command
    /// line, to open once an account is there. `start_minimized`: the
    /// window was opened unseen, to stay in the tray if there is one.
    pub fn new(
        ctx: &egui::Context,
        enabled: bool,
        instance: Option<Instance>,
        links: Vec<String>,
        start_minimized: bool,
    ) -> Self {
        let wake: Arc<dyn Fn() + Send + Sync> = {
            let ctx = ctx.clone();
            Arc::new(move || ctx.request_repaint())
        };
        let mut desktop = Self {
            enabled,
            wake: wake.clone(),
            tray: None,
            appearance: None,
            instance: None,
            links: links.into_iter().take(MAX_LINKS).collect(),
            presence: None,
            presence_account: None,
            programs: BTreeMap::new(),
            sequence: 0,
            games: None,
            games_asked: false,
            playing: None,
            told: None,
            font: None,
            font_loading: None,
            base_fonts: None,
            fallbacks: Default::default(),
            fonts_listing: None,
            autostart: None,
            hidden: false,
            quit: false,
            #[cfg(target_os = "linux")]
            kwin: None,
            starting_hidden: false,
            programs_listing: None,
            watched: None,
        };
        if enabled {
            desktop.tray = Some(Tray::start(wake.clone()));
            desktop.appearance = Some(Appearance::start(wake.clone()));
            #[cfg(any(target_os = "linux", windows))]
            {
                desktop.instance = instance.map(|instance| instance.listen(wake.clone()));
            }
            #[cfg(target_os = "linux")]
            {
                desktop.kwin = rune_platform::kwin::KWin::detect();
            }
            #[cfg(not(any(target_os = "linux", windows)))]
            let _ = instance;
            // Unseen until the tray says whether it shows.
            desktop.starting_hidden = start_minimized;
            desktop.hidden = start_minimized;
            desktop.autostart = Some(background(wake, || {
                let (on, minimized) = rune_platform::autostart::state().unwrap_or_default();
                (on, minimized, None)
            }));
        }
        desktop
    }

    /// What an event changed that matters here: a new session publishes
    /// again and reads the list of games again.
    pub fn applied(&mut self, applied: &Applied) {
        match applied {
            Applied::Ready => {
                self.told = None;
                self.games_asked = false;
                self.watched = None;
            }
            Applied::DetectableGames => self.watched = None,
            _ => {}
        }
    }

    /// Closing the window hides it in the tray instead: asked for, and
    /// the tray shows.
    pub fn closes_to_tray(&self, ui: &Ui) -> bool {
        self.enabled && !self.quit && ui.prefs.close_to_tray && ui.desktop.tray == Some(true)
    }

    /// Start RUNE with the session, in the tray when `minimized`, or no
    /// longer, off this thread.
    pub fn set_autostart(&mut self, on: bool, minimized: bool) {
        // An installation's own path, which updates keep (actualizaciones).
        let program = crate::update::program();
        self.autostart = Some(background(self.wake.clone(), move || {
            let problem = match &program {
                Ok(program) => rune_platform::autostart::change(on, minimized, program).err(),
                Err(error) => {
                    eprintln!("rune: autostart: where RUNE runs from is unknown: {error}");
                    Some(es("No se pudo cambiar el inicio con la sesión."))
                }
            }
            .map(String::from);
            let (on, minimized) = rune_platform::autostart::state().unwrap_or_default();
            (on, minimized, problem)
        }));
    }

    // Desktop session and games (escritorio-sesion-y-juegos).
    /// List the programs open, to add one as a game, off this thread.
    pub fn list_programs(&mut self) {
        if self.programs_listing.is_none() {
            self.programs_listing = Some(background(
                self.wake.clone(),
                rune_platform::games::programs,
            ));
        }
    }

    /// List the system's fonts, off this thread.
    pub fn list_fonts(&mut self) {
        if self.fonts_listing.is_none() {
            self.fonts_listing = Some(background(
                self.wake.clone(),
                rune_platform::fonts::families,
            ));
        }
    }

    /// Hide the window in the tray. Wayland cannot hide a window: Hyprland
    /// moves it to a workspace of its own, KWin takes it off the taskbar
    /// with a script (escritorio-sesion-y-juegos), other compositors
    /// minimize it.
    pub fn hide(&mut self, ctx: &egui::Context) {
        self.hidden = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            #[cfg(target_os = "linux")]
            if let Some(socket) = rune_platform::hyprland::socket() {
                let pid = std::process::id();
                std::thread::spawn(move || {
                    if let Err(error) = rune_platform::hyprland::hide(&socket, pid) {
                        eprintln!("rune: Hyprland did not hide the window: {error}");
                    }
                });
                return;
            }
            #[cfg(target_os = "linux")]
            if let Some(kwin) = self.kwin.as_ref().filter(|kwin| kwin.ready()) {
                kwin.set(rune_platform::kwin::Place::Hidden);
                return;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    /// Bring the window to the front, from the tray or a later start.
    pub fn show(&mut self, ctx: &egui::Context) {
        self.starting_hidden = false;
        if self.hidden {
            self.hidden = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            #[cfg(target_os = "linux")]
            if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                if let Some(socket) = rune_platform::hyprland::socket() {
                    let pid = std::process::id();
                    std::thread::spawn(move || {
                        if let Err(error) = rune_platform::hyprland::show(&socket, pid) {
                            eprintln!("rune: Hyprland did not show the window: {error}");
                        }
                    });
                }
                if let Some(kwin) = &self.kwin {
                    kwin.set(rune_platform::kwin::Place::Shown);
                }
            }
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        // Wayland gives the focus only to a window asking for attention.
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
            egui::UserAttentionType::Informational,
        ));
        ctx.request_repaint();
    }

    /// One step: what the services reported, and what the preferences ask
    /// of them now. Returns what the interface asked for, and whether the
    /// fonts changed (the tree must measure with them again).
    pub fn step(
        &mut self,
        ctx: &egui::Context,
        ui: &mut Ui,
        store: &mut Store,
        now: Instant,
        wall: rune_model::Timestamp,
    ) -> (Vec<Intent>, bool) {
        if !self.enabled {
            return (Vec::new(), false);
        }
        let mut intents = Vec::new();
        if let Some(scheme) = self.appearance.as_ref().and_then(Appearance::poll) {
            rune_ui::desktop::color_scheme(
                ui,
                match scheme {
                    ColorScheme::Dark => Some(true),
                    ColorScheme::Light => Some(false),
                    ColorScheme::NoPreference => None,
                },
            );
            ctx.request_repaint();
        }
        // The accent color: only Windows says one yet (windows-base).
        if let Some(accent) = self.appearance.as_ref().and_then(Appearance::poll_accent) {
            rune_ui::desktop::system_accent(
                ui,
                accent.map(|[red, green, blue]| limn::Color::rgb(red, green, blue)),
            );
            ctx.request_repaint();
        }
        if let Some((on, minimized, problem)) = take(&mut self.autostart) {
            rune_ui::desktop::autostart_known(ui, on, minimized, problem);
            ctx.request_repaint();
        }
        if let Some(programs) = take(&mut self.programs_listing) {
            let listed = programs
                .into_iter()
                .map(|program| (program.key, program.path))
                .collect();
            rune_ui::games::programs_listed(ui, listed);
            ctx.request_repaint();
        }
        if let Some(families) = take(&mut self.fonts_listing) {
            rune_ui::desktop::fonts_listed(ui, families);
            ctx.request_repaint();
        }
        intents.extend(self.tray_step(ctx, ui, store, now, wall));
        // Before the links: a program's invitation opens in this frame.
        self.presence_step(ctx, ui, store);
        intents.extend(self.links_step(ctx, ui, store, now));
        intents.extend(self.games_step(ui, store, wall));
        let chosen = self.chosen();
        if self.told.as_ref() != Some(&chosen) {
            intents.extend(rune_ui::desktop::local_activity(ui, store, chosen.clone()));
            self.told = Some(chosen);
            ctx.request_repaint();
        }
        let fonts = self.font_step(ctx, ui);
        (intents, fonts)
    }

    fn tray_step(
        &mut self,
        ctx: &egui::Context,
        ui: &mut Ui,
        store: &mut Store,
        now: Instant,
        wall: rune_model::Timestamp,
    ) -> Vec<Intent> {
        let mut intents = Vec::new();
        while let Some(event) = self.tray.as_ref().and_then(Tray::poll) {
            let click = |ui: &mut Ui, store: &mut Store, id: String| {
                rune_ui::update(ui, store, &UiAction::Clicked { id }, now)
            };
            match event {
                TrayEvent::Shown => {
                    rune_ui::desktop::tray_shown(ui, true);
                    // Started in the tray: now there is one to stay in.
                    if std::mem::take(&mut self.starting_hidden) {
                        self.hide(ctx);
                    }
                }
                TrayEvent::Unavailable => {
                    rune_ui::desktop::tray_shown(ui, false);
                    // Gone with the tray, a hidden window comes back.
                    if self.hidden {
                        self.show(ctx);
                    }
                }
                TrayEvent::Open => self.show(ctx),
                TrayEvent::ToggleMute => intents.extend(click(ui, store, "me:mute".into())),
                TrayEvent::ToggleDeafen => intents.extend(click(ui, store, "me:deafen".into())),
                TrayEvent::Status(status) => {
                    let key = match status {
                        TrayStatus::Online => "online",
                        TrayStatus::Idle => "idle",
                        TrayStatus::DoNotDisturb => "dnd",
                        TrayStatus::Invisible => "invisible",
                    };
                    let id = format!("{}{key}", rune_components::profile::ids::STATUS);
                    intents.extend(click(ui, store, id));
                }
                TrayEvent::Quit => {
                    // Quitting saves first, which needs the window's frames.
                    self.quit = true;
                    self.show(ctx);
                }
            }
            ctx.request_repaint();
        }
        if let Some(tray) = &self.tray {
            let (mentions, unread) = rune_ui::desktop::tray_counts(ui, store, wall);
            tray.update(TrayState {
                mentions,
                unread,
                muted: ui.muted,
                deafened: ui.deafened,
                status: store.me().map(|me| match store.presence(me).status {
                    Status::Idle => TrayStatus::Idle,
                    Status::DoNotDisturb => TrayStatus::DoNotDisturb,
                    Status::Invisible | Status::Offline => TrayStatus::Invisible,
                    Status::Online => TrayStatus::Online,
                }),
                language: ui.language(),
            });
        }
        intents
    }

    fn links_step(
        &mut self,
        ctx: &egui::Context,
        ui: &mut Ui,
        store: &mut Store,
        now: Instant,
    ) -> Vec<Intent> {
        #[cfg(any(target_os = "linux", windows))]
        while let Some(request) = self.instance.as_ref().and_then(|instance| instance.poll()) {
            self.show(ctx);
            if let rune_platform::single_instance::Request::Open(link) = request {
                if self.links.len() < MAX_LINKS {
                    self.links.push(link);
                }
            }
        }
        let _ = ctx;
        if store.me().is_none() || self.links.is_empty() {
            return Vec::new();
        }
        let links = std::mem::take(&mut self.links);
        links
            .iter()
            .flat_map(|link| rune_ui::desktop::open_link(ui, store, link, now))
            .collect()
    }

    fn presence_step(&mut self, ctx: &egui::Context, ui: &mut Ui, store: &Store) {
        if !ui.prefs.rich_presence {
            if self.presence.take().is_some() {
                self.programs.clear();
                rune_ui::desktop::rich_presence_problem(ui, None);
            }
            return;
        }
        let presence = self
            .presence
            .get_or_insert_with(|| RichPresence::start(self.wake.clone()));
        if store.me() != self.presence_account {
            self.presence_account = store.me();
            presence.set_account(store.me_user().map(|me| rich_presence::Account {
                id: me.id.0,
                username: me.name.clone(),
            }));
        }
        let events: Vec<PresenceEvent> = std::iter::from_fn(|| presence.poll()).collect();
        for event in events {
            self.program_event(ctx, ui, event);
        }
    }

    /// What a program said through the local socket.
    fn program_event(&mut self, ctx: &egui::Context, ui: &mut Ui, event: PresenceEvent) {
        match event {
            PresenceEvent::Set { client, activity } => {
                let Some(activity) = program_activity(activity) else {
                    return;
                };
                self.sequence += 1;
                self.programs.insert(client, (self.sequence, activity));
                while self.programs.len() > MAX_PROGRAMS {
                    let oldest = self
                        .programs
                        .iter()
                        .min_by_key(|(_, (when, _))| *when)
                        .map(|(client, _)| *client);
                    self.programs.retain(|client, _| Some(*client) != oldest);
                }
            }
            PresenceEvent::Cleared { client } => {
                self.programs.remove(&client);
            }
            // As a link a later start hands over: once an account is there
            // it opens in RUNE's own invitation flow, where the person
            // accepts it or not; nothing joins by itself.
            PresenceEvent::Invite { code } => {
                if self.links.len() < MAX_LINKS {
                    self.links.push(format!("https://discord.gg/{code}"));
                }
                self.show(ctx);
            }
            PresenceEvent::Unavailable => rune_ui::desktop::rich_presence_problem(
                ui,
                Some(
                    es("No se pudo abrir el socket local: puede que otro cliente lo esté usando.")
                        .into(),
                ),
            ),
        }
    }

    fn games_step(
        &mut self,
        ui: &mut Ui,
        store: &Store,
        wall: rune_model::Timestamp,
    ) -> Vec<Intent> {
        let able = store.provider.as_ref().is_some_and(|info| {
            info.capabilities.detectable_games && info.capabilities.local_activity
        });
        if !ui.prefs.game_activity || !able || store.me().is_none() {
            self.games = None;
            self.watched = None;
            // A game that was running stops being played now.
            return match self.playing.take() {
                Some(_) => rune_ui::games::running(ui, None, wall.0),
                None => Vec::new(),
            };
        }
        let watcher = self
            .games
            .get_or_insert_with(|| GameWatcher::start(self.wake.clone()));
        let mut intents = Vec::new();
        if !self.games_asked {
            self.games_asked = true;
            intents.push(Intent::Command(rune_provider::Command::LoadDetectableGames));
        }
        // The person's games at once, the provider's once its list is read;
        // again after any change of either (escritorio-sesion-y-juegos).
        let listed = store.detectable_games();
        let wanted = (listed.is_some(), rune_ui::games::revision(ui));
        if self.watched != Some(wanted) {
            let listed = listed.as_deref().map_or(&[][..], Vec::as_slice);
            watcher.set_games(&watched(&ui.prefs.registered_games, listed));
            self.watched = Some(wanted);
            // A game renamed while it runs is published by its new name.
            if let Some(playing) = self.playing.as_mut() {
                if let Some(game) = ui
                    .prefs
                    .registered_games
                    .iter()
                    .find(|game| game.executable == playing.executable && !game.hidden)
                {
                    playing.name.clone_from(&game.name);
                }
            }
        }
        if let Some(playing) = watcher.poll() {
            self.playing = playing;
            let running = self
                .playing
                .as_ref()
                .map(|playing| rune_ui::games::Running {
                    executable: playing.executable.clone(),
                    application: playing.application,
                    name: playing.name.clone(),
                });
            intents.extend(rune_ui::games::running(ui, running, wall.0));
        }
        intents
    }

    /// What this device says now: the newest program's activity, else the
    /// game running.
    fn chosen(&self) -> Option<LocalActivity> {
        let program = self
            .programs
            .values()
            .max_by_key(|(when, _)| *when)
            .map(|(_, activity)| activity.clone());
        program.or_else(|| {
            self.playing.as_ref().map(|playing| LocalActivity {
                application: playing.application,
                name: Some(playing.name.clone()),
                start: Some(playing.since),
                source: ActivitySource::Game,
                ..LocalActivity::default()
            })
        })
    }

    /// Install `fonts` with the system's fallbacks found so far after them.
    fn install(&mut self, ctx: &egui::Context, fonts: egui::FontDefinitions) {
        ctx.set_fonts(self.fallbacks.with(&fonts));
        self.base_fonts = Some(fonts);
    }

    /// After a layout: find a system font for what `frame` paints that no
    /// installed face draws, off this thread. True when the fonts changed.
    pub fn text_step(&mut self, ctx: &egui::Context, frame: &limn::Frame) -> bool {
        if !self.enabled {
            return false;
        }
        let base = self
            .base_fonts
            .get_or_insert_with(limn_egui::font_definitions);
        self.fallbacks.scan(base, frame);
        let wake = self.wake.clone();
        match self.fallbacks.step(base, |work| background(wake, work)) {
            Some(fonts) => {
                ctx.set_fonts(fonts);
                true
            }
            None => false,
        }
    }

    /// Install the font the preferences name, loaded off this thread; RUNE's
    /// own when it is gone. True when the fonts changed.
    fn font_step(&mut self, ctx: &egui::Context, ui: &mut Ui) -> bool {
        let wanted = ui.prefs.ui_font.clone();
        let loading = self.font_loading.as_ref().map(|(asked, _)| asked.clone());
        if wanted != self.font && loading.as_ref() != Some(&wanted) {
            match &wanted {
                None => {
                    self.font_loading = None;
                    self.font = None;
                    self.install(ctx, limn_egui::font_definitions());
                    return true;
                }
                Some(family) => {
                    let family = family.clone();
                    self.font_loading = Some((
                        wanted.clone(),
                        background(self.wake.clone(), move || {
                            rune_platform::fonts::load(&family)
                        }),
                    ));
                }
            }
        }
        let Some((asked, receiver)) = &self.font_loading else {
            return false;
        };
        let Ok(faces) = receiver.try_recv() else {
            return false;
        };
        let asked = asked.clone();
        self.font_loading = None;
        self.font = asked.clone();
        match faces {
            Some(faces) => {
                let data = |face: rune_platform::fonts::Face| egui::FontData {
                    index: face.index,
                    ..egui::FontData::from_owned(face.bytes)
                };
                let fonts = limn_egui::font_definitions_with(limn_egui::UiFaces {
                    regular: data(faces.regular),
                    medium: faces.medium.map(data),
                    semibold: faces.semibold.map(data),
                    bold: faces.bold.map(data),
                });
                self.install(ctx, fonts);
                rune_ui::desktop::font_problem(ui, None);
            }
            None => {
                self.install(ctx, limn_egui::font_definitions());
                rune_ui::desktop::font_problem(
                    ui,
                    Some(tr!(
                        "No se encontró «{font}» en el sistema: se usa la fuente de RUNE.",
                        font = asked.unwrap_or_default()
                    )),
                );
            }
        }
        true
    }
}

// Desktop session and games (escritorio-sesion-y-juegos).
/// The games to look for: the person's own first, so their name wins, then
/// the provider's with the names the person gave them, and nothing of what
/// the person hid: a game by its application, a program by its name.
fn watched(registered: &[RegisteredGame], listed: &[rune_model::DetectableGame]) -> Vec<Game> {
    use rune_platform::games::normalize;
    let hidden_programs: HashSet<String> = registered
        .iter()
        .filter(|game| game.hidden)
        .map(|game| normalize(&game.executable))
        .collect();
    let hidden_games: HashSet<u64> = registered
        .iter()
        .filter(|game| game.hidden)
        .filter_map(|game| game.application)
        .collect();
    let names: HashMap<u64, &str> = registered
        .iter()
        .filter(|game| !game.hidden)
        .filter_map(|game| Some((game.application?, game.name.as_str())))
        .collect();
    let own = registered
        .iter()
        .filter(|game| !game.hidden && game.application.is_none())
        .map(|game| Game {
            application: None,
            name: game.name.clone(),
            executables: vec![game.executable.clone()],
        });
    let theirs = listed
        .iter()
        .filter(|game| {
            game.application
                .is_none_or(|application| !hidden_games.contains(&application))
        })
        .filter_map(|game| {
            let executables: Vec<String> = game
                .executables
                .iter()
                .filter(|program| !hidden_programs.contains(&normalize(program)))
                .cloned()
                .collect();
            let name = game
                .application
                .and_then(|application| names.get(&application))
                .map_or_else(|| game.name.clone(), |name| (*name).to_owned());
            (!executables.is_empty()).then_some(Game {
                application: game.application,
                name,
                executables,
            })
        });
    own.chain(theirs).collect()
}

/// A program's activity as RUNE publishes it; `None` for a kind it does not.
fn program_activity(activity: rich_presence::Activity) -> Option<LocalActivity> {
    Some(LocalActivity {
        application: Some(activity.application),
        kind: LocalActivityKind::from_number(activity.kind)?,
        name: None,
        details: activity.details,
        state: activity.state,
        start: activity.start,
        end: activity.end,
        large_image: activity.large_image,
        large_text: activity.large_text,
        small_image: activity.small_image,
        small_text: activity.small_text,
        source: ActivitySource::Program,
    })
}

/// Run `work` on a thread of its own; its answer comes on the receiver.
fn background<T: Send + 'static>(
    wake: Arc<dyn Fn() + Send + Sync>,
    work: impl FnOnce() -> T + Send + 'static,
) -> mpsc::Receiver<T> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        if sender.send(work()).is_ok() {
            wake();
        }
    });
    receiver
}

/// The answer of a background job, once, if it came.
fn take<T>(slot: &mut Option<mpsc::Receiver<T>>) -> Option<T> {
    let answer = slot.as_ref()?.try_recv();
    match answer {
        Ok(answer) => {
            *slot = None;
            Some(answer)
        }
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => {
            *slot = None;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_program_wins_over_a_game_and_leaving_hands_back() {
        let ctx = egui::Context::default();
        let mut desktop = Desktop::new(&ctx, false, None, Vec::new(), false);
        desktop.playing = Some(Playing {
            application: Some(1),
            name: "Juego".into(),
            since: 10,
            executable: "juego.exe".into(),
        });
        assert_eq!(
            desktop
                .chosen()
                .and_then(|activity| activity.name)
                .as_deref(),
            Some("Juego")
        );
        let program = |details: &str| {
            program_activity(rich_presence::Activity {
                application: 9,
                details: Some(details.into()),
                ..Default::default()
            })
            .unwrap()
        };
        desktop.programs.insert(1, (1, program("uno")));
        desktop.programs.insert(2, (2, program("dos")));
        assert_eq!(
            desktop
                .chosen()
                .and_then(|activity| activity.details)
                .as_deref(),
            Some("dos")
        );
        desktop.programs.remove(&2);
        assert_eq!(
            desktop
                .chosen()
                .and_then(|activity| activity.details)
                .as_deref(),
            Some("uno")
        );
        desktop.programs.clear();
        assert_eq!(
            desktop.chosen().map(|activity| activity.source),
            Some(ActivitySource::Game)
        );
        assert!(
            program_activity(rich_presence::Activity {
                kind: 1,
                ..Default::default()
            })
            .is_none(),
            "streaming is not a program's"
        );
    }

    #[test]
    fn a_program_s_invitation_waits_for_an_account_then_opens_as_the_provider_s_link() {
        use rune_provider::mock::{MockConfig, MockProvider};
        use rune_provider::Provider;
        let ctx = egui::Context::default();
        let mut desktop = Desktop::new(&ctx, false, None, Vec::new(), false);
        let mut ui = Ui::new();
        let mut store = Store::new();
        let now = Instant::now();
        desktop.program_event(
            &ctx,
            &mut ui,
            PresenceEvent::Invite {
                code: "abc-DEF_1".into(),
            },
        );
        assert!(
            desktop
                .links_step(&ctx, &mut ui, &mut store, now)
                .is_empty(),
            "no account yet"
        );
        let provider = MockProvider::new(MockConfig::default());
        store.apply(
            rune_provider::Event::Ready(Box::new(provider.snapshot())),
            now,
        );
        let mut info = provider.info();
        info.capabilities.trusted_links = &["discord.gg"];
        store.provider = Some(info);
        // The provider's own link: the host opens its invitation dialog,
        // where accepting is the person's click.
        assert_eq!(
            desktop.links_step(&ctx, &mut ui, &mut store, now),
            [Intent::OpenUrl("https://discord.gg/abc-DEF_1".into())]
        );
        assert!(desktop.links.is_empty());
    }

    #[test]
    fn captures_start_nothing_and_never_close_to_the_tray() {
        let ctx = egui::Context::default();
        let mut desktop = Desktop::new(&ctx, false, None, Vec::new(), false);
        let mut ui = Ui::new();
        ui.prefs.close_to_tray = true;
        ui.desktop.tray = Some(true);
        assert!(!desktop.closes_to_tray(&ui));
        let mut store = Store::new();
        let (intents, fonts) = desktop.step(
            &ctx,
            &mut ui,
            &mut store,
            Instant::now(),
            rune_model::Timestamp(0),
        );
        assert!(intents.is_empty() && !fonts);
        assert!(desktop.tray.is_none() && desktop.appearance.is_none());
        // Nor do they start unseen: that is the session's start alone.
        let desktop = Desktop::new(&ctx, false, None, Vec::new(), true);
        assert!(!desktop.hidden && !desktop.starting_hidden);
    }

    // Desktop session and games (escritorio-sesion-y-juegos).
    #[test]
    fn the_person_s_games_come_first_renamed_and_hidden_ones_are_not_looked_for() {
        let registered =
            |executable: &str, application: Option<u64>, hidden: bool| RegisteredGame {
                executable: executable.into(),
                name: format!("Mío {executable}"),
                application,
                hidden,
                last_played: None,
            };
        let listed = |application: u64, executables: &[&str]| rune_model::DetectableGame {
            application: Some(application),
            name: format!("Suyo {application}"),
            executables: executables.iter().map(|e| (*e).to_owned()).collect(),
        };
        let games = watched(
            &[
                registered("opt/juego/juego.x86_64", None, false),
                registered("uno.exe", Some(1), false),
                registered("dos.exe", Some(2), true),
                registered("Tres.EXE", None, true),
            ],
            &[
                listed(1, &["uno.exe"]),
                listed(2, &["dos.exe", "linux/dos"]),
                listed(3, &["tres.exe", "linux/tres"]),
            ],
        );
        let summary: Vec<(Option<u64>, &str, Vec<&str>)> = games
            .iter()
            .map(|game| {
                let executables = game.executables.iter().map(String::as_str).collect();
                (game.application, game.name.as_str(), executables)
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    None,
                    "Mío opt/juego/juego.x86_64",
                    vec!["opt/juego/juego.x86_64"]
                ),
                (Some(1), "Mío uno.exe", vec!["uno.exe"]),
                (Some(3), "Suyo 3", vec!["linux/tres"]),
            ]
        );
    }
}
