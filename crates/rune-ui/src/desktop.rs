//! The desktop work (P6) as the interface sees it: the tray and closing to
//! it, starting with the session, the desktop's theme, the interface's
//! font, the panels' widths, favorite channels and pinned conversations,
//! the channels the account cannot see, the global shortcuts of the
//! microphone and the sound, the activity this device publishes, and the
//! call's encryption codes. What touches the system the host does, and
//! tells this module what it found.

use std::ops::RangeInclusive;
use std::time::Instant;

use limn::{Component, Element, ResizeHandle, UiAction};
use rune_components::desktop::{self as parts, Verifying};
use rune_components::voice::details_tabs;
use rune_components::Look;
use rune_i18n::t;
use rune_model::{Activity, Channel, ChannelId, ChannelKind, LocalActivity, ServerId, Timestamp};
use rune_provider::Command;
use rune_state::{Store, Verification};

use crate::settings::{valid_key, Prefs};
use crate::{Intent, Theme, Ui};

pub(crate) const CHANNELS_HANDLE: &str = "panel:channels";
pub(crate) const MEMBERS_HANDLE: &str = "panel:members";
/// How wide the column of channels and the member list may be.
pub const SIDEBAR_WIDTHS: RangeInclusive<f32> = 220.0..=440.0;
pub const MEMBERS_WIDTHS: RangeInclusive<f32> = 220.0..=400.0;
/// The handle's width, over the boundary it moves.
const HANDLE: f32 = 6.0;
/// Favorites and pinned conversations kept, as Serein bounds them.
const KEPT: usize = 64;
/// People of the call listed to verify.
const VERIFIABLE: usize = 8;

/// A shortcut the settings record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shortcut {
    #[default]
    PushToTalk,
    Mute,
    Deafen,
    PushToMute,
}

impl Shortcut {
    pub const GLOBAL: [Self; 3] = [Self::Mute, Self::Deafen, Self::PushToMute];

    pub fn key(self) -> &'static str {
        match self {
            Self::PushToTalk => "ptt",
            Self::Mute => "mute",
            Self::Deafen => "deafen",
            Self::PushToMute => "ptm",
        }
    }

    /// What it does, as the settings and the desktop's dialog say it.
    pub fn label(self) -> &'static str {
        match self {
            Self::PushToTalk => t("Pulsar para hablar"),
            Self::Mute => t("Silenciar o activar el micrófono"),
            Self::Deafen => t("Ensordecer o volver a oír"),
            Self::PushToMute => t("Pulsar para silenciar"),
        }
    }

    /// The key recorded for it.
    pub fn binding(self, prefs: &Prefs) -> Option<&str> {
        match self {
            Self::PushToTalk => prefs.push_to_talk_key.as_deref(),
            Self::Mute => prefs.mute_key.as_deref(),
            Self::Deafen => prefs.deafen_key.as_deref(),
            Self::PushToMute => prefs.push_to_mute_key.as_deref(),
        }
    }

    pub(crate) fn slot(self, prefs: &mut Prefs) -> (&'static str, &mut Option<String>) {
        match self {
            Self::PushToTalk => ("push_to_talk_key", &mut prefs.push_to_talk_key),
            Self::Mute => ("mute_key", &mut prefs.mute_key),
            Self::Deafen => ("deafen_key", &mut prefs.deafen_key),
            Self::PushToMute => ("push_to_mute_key", &mut prefs.push_to_mute_key),
        }
    }
}

/// What the host found about the desktop, and the panels being resized.
#[derive(Debug, Default)]
pub struct Desktop {
    /// The desktop shows RUNE's icon in its tray; `None` until it says.
    pub tray: Option<bool>,
    /// RUNE starts with the session; `None` until read, or while changing.
    pub autostart: Option<bool>,
    /// Why starting with the session could not change, if it could not.
    pub autostart_problem: Option<String>,
    /// The system's font families, once listed.
    pub fonts: Option<Vec<String>>,
    /// Why the chosen font is not the one shown, if it is not.
    pub font_problem: Option<String>,
    /// The desktop prefers dark (`Some(true)`) or light; `None` unknown.
    pub system_dark: Option<bool>,
    /// What this device says the person is doing now, published or not.
    pub activity: Option<LocalActivity>,
    /// Why the local Rich Presence socket is not open, while on.
    pub rich_presence_problem: Option<String>,
    resizing: Option<(Panel, f32, f32)>,
    /// The voice details show their «Privacidad» tab.
    privacy_tab: bool,
    // Interface languages (i18n).
    /// The session's language, for the «Sistema» choice; Spanish until the
    /// host reads it, and in tests.
    pub system_language: rune_i18n::Language,
    // Linux desktop (escritorio-linux).
    /// Who draws the window's title bar, as the person chose (barra-e-icono).
    pub title_bar: crate::title_bar::TitleBar,
    /// The GPUs the window can be drawn with.
    pub graphics: Graphics,
    // Desktop session and games (escritorio-sesion-y-juegos).
    /// The session starts RUNE in the tray, as its entry says.
    pub autostart_minimized: bool,
    /// How the providers' REST requests go from the next start, as kept.
    pub proxy: rune_provider::ApiProxy,
    /// The proxy's address being written, and why it was refused.
    pub proxy_draft: Option<String>,
    pub proxy_problem: Option<&'static str>,
    /// The person's list of games: what runs, and the programs to add.
    pub games: crate::games::Games,
    // Windows (windows-base).
    /// The desktop's accent color, as it says it: the main color while the
    /// theme follows the system's and the person chose none.
    pub system_accent: Option<limn::Color>,
    // The window's own title bar (barra-e-icono): the host says, every frame.
    /// RUNE draws the window's frame and its buttons show at the end of the
    /// top bar: not in full screen.
    pub window_buttons: bool,
    /// The window is maximized: its middle button restores it.
    pub maximized: bool,
}

// Desktop session and games (escritorio-sesion-y-juegos).
/// The field the proxy's address is written in.
pub(crate) const PROXY: &str = "us:proxy-url";

// Linux desktop (escritorio-linux).
/// The GPUs RUNE can draw its window with, as the host found them before
/// opening it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graphics {
    /// Each one that can be chosen: its key and its name.
    pub gpus: Vec<(String, String)>,
    /// The one chosen for the next start; `None`: as RUNE chooses.
    pub chosen: Option<String>,
    /// The one drawing the window now, as its driver names it.
    pub drawing: Option<String>,
    /// The variable of the environment that decides the GPU instead, if one
    /// does.
    pub overridden: Option<String>,
    /// The GPU this start was limited to could not open the window: it
    /// draws with any.
    pub fallback: bool,
    /// That GPU was the person's choice; else RUNE's own (Automático).
    pub fallback_chosen: bool,
}

impl Desktop {
    /// What stays for another account or provider: what the host found of
    /// the device and the window, which it says only once. A resize, the
    /// voice details' tab and the activity published for the account go.
    pub(crate) fn kept(&mut self) -> Self {
        Self {
            activity: None,
            resizing: None,
            privacy_tab: false,
            ..std::mem::take(self)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Channels,
    Members,
}

fn save(key: &str, value: impl ToString) -> Intent {
    Intent::SaveSetting {
        key: key.into(),
        value: Some(value.to_string()),
    }
}

fn ids(list: &[ChannelId]) -> Option<String> {
    (!list.is_empty()).then(|| {
        list.iter()
            .map(|channel| channel.0.to_string())
            .collect::<Vec<_>>()
            .join(",")
    })
}

/// The preferences of this module, as settings to keep.
pub(crate) fn kept(prefs: &Prefs) -> Vec<(&'static str, Option<String>)> {
    let flag = |on: bool| Some(on.to_string());
    vec![
        ("close_to_tray", flag(prefs.close_to_tray)),
        ("ui_font", prefs.ui_font.clone()),
        ("sidebar_width", Some(prefs.sidebar_width.to_string())),
        ("members_width", Some(prefs.members_width.to_string())),
        ("favorite_channels", ids(&prefs.favorites)),
        ("pinned_directs", ids(&prefs.pinned)),
        ("show_hidden_channels", flag(prefs.show_hidden)),
        ("mute_key", prefs.mute_key.clone()),
        ("deafen_key", prefs.deafen_key.clone()),
        ("push_to_mute_key", prefs.push_to_mute_key.clone()),
        ("rich_presence", flag(prefs.rich_presence)),
        ("game_activity", flag(prefs.game_activity)),
        // Desktop session and games (escritorio-sesion-y-juegos).
        (
            crate::games::SETTING,
            crate::games::kept(&prefs.registered_games),
        ),
    ]
}

/// Take back what [`kept`] kept; what is missing or wrong stays as it starts.
pub(crate) fn restore(prefs: &mut Prefs, setting: &impl Fn(&str) -> Option<String>) {
    let usual = Prefs::default();
    let flag = |key: &str, default: bool| match setting(key).as_deref() {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    };
    let width = |key: &str, range: RangeInclusive<f32>, default: f32| {
        setting(key)
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|width| range.contains(width))
            .unwrap_or(default)
    };
    let key = |key: &str| setting(key).filter(|value| valid_key(value));
    prefs.close_to_tray = flag("close_to_tray", usual.close_to_tray);
    prefs.system_theme = setting("theme").as_deref() == Some("system");
    prefs.ui_font = setting("ui_font").filter(|family| {
        !family.is_empty() && family.len() <= 128 && !family.chars().any(char::is_control)
    });
    prefs.sidebar_width = width("sidebar_width", SIDEBAR_WIDTHS, usual.sidebar_width);
    prefs.members_width = width("members_width", MEMBERS_WIDTHS, usual.members_width);
    restore_lists(prefs, setting);
    prefs.show_hidden = flag("show_hidden_channels", usual.show_hidden);
    prefs.mute_key = key("mute_key");
    prefs.deafen_key = key("deafen_key");
    prefs.push_to_mute_key = key("push_to_mute_key");
    prefs.rich_presence = flag("rich_presence", usual.rich_presence);
    prefs.game_activity = flag("game_activity", usual.game_activity);
    // Desktop session and games (escritorio-sesion-y-juegos).
    prefs.registered_games = crate::games::restore(setting(crate::games::SETTING));
}

/// Take back the favorites and pinned conversations: the account's.
pub(crate) fn restore_lists(prefs: &mut Prefs, setting: &impl Fn(&str) -> Option<String>) {
    let list = |key: &str| -> Vec<ChannelId> {
        setting(key)
            .unwrap_or_default()
            .split(',')
            .filter_map(|id| id.parse().ok().map(ChannelId))
            .take(KEPT)
            .collect()
    };
    prefs.favorites = list("favorite_channels");
    prefs.pinned = list("pinned_directs");
}

/// Follow the desktop's light or dark preference from now on.
pub(crate) fn follow_system_theme(ui: &mut Ui) -> Vec<Intent> {
    ui.prefs.system_theme = true;
    apply_scheme(ui);
    vec![save("theme", "system")]
}

fn apply_scheme(ui: &mut Ui) {
    if ui.prefs.system_theme {
        // Dark, or no preference, is RUNE's own: Ónix.
        ui.theme = match ui.desktop.system_dark {
            Some(false) => Theme::Light,
            _ => Theme::Onyx,
        };
    }
}

/// The desktop prefers `dark` now (`None`: no preference, or no portal).
pub fn color_scheme(ui: &mut Ui, dark: Option<bool>) {
    ui.desktop.system_dark = dark;
    apply_scheme(ui);
}

// Windows (windows-base).
/// The desktop's accent color now; `None` when it says none.
pub fn system_accent(ui: &mut Ui, accent: Option<limn::Color>) {
    ui.desktop.system_accent = accent;
}

/// What the tray host said: whether the icon shows.
pub fn tray_shown(ui: &mut Ui, shown: bool) {
    ui.desktop.tray = Some(shown);
}

/// Whether RUNE starts with the session, and in the tray, as the host read
/// or left it; and why it could not change, if it could not.
pub fn autostart_known(ui: &mut Ui, on: bool, minimized: bool, problem: Option<String>) {
    ui.desktop.autostart = Some(on);
    ui.desktop.autostart_minimized = on && minimized;
    ui.desktop.autostart_problem = problem;
}

// Desktop session and games (escritorio-sesion-y-juegos).
/// How the providers' REST requests go, as the host kept it.
pub fn proxy_known(ui: &mut Ui, proxy: rune_provider::ApiProxy) {
    ui.desktop.proxy = proxy;
}

/// Keep the proxy address written, if it is only an address.
fn save_proxy(ui: &mut Ui) -> Vec<Intent> {
    let draft = ui.desktop.proxy_draft.clone().unwrap_or_default();
    match rune_provider::ApiProxy::url(&draft) {
        Ok(proxy) => {
            ui.desktop.proxy_draft = None;
            ui.desktop.proxy_problem = None;
            ui.desktop.proxy = proxy.clone();
            vec![Intent::SetProxy(proxy)]
        }
        Err(reason) => {
            ui.desktop.proxy_problem = Some(reason);
            Vec::new()
        }
    }
}

/// Choose how REST requests go: `automatic`, `direct`, or `url`, which
/// opens the field for the address.
fn choose_proxy(ui: &mut Ui, mode: &str) -> Option<Vec<Intent>> {
    use rune_provider::ApiProxy;
    ui.settings_state.select = None;
    ui.desktop.proxy_problem = None;
    let proxy = match mode {
        "automatic" => ApiProxy::Automatic,
        "direct" => ApiProxy::Direct,
        "url" => {
            let written = match &ui.desktop.proxy {
                ApiProxy::Url(url) => url.clone(),
                _ => String::new(),
            };
            ui.desktop.proxy_draft = Some(written.clone());
            return Some(vec![
                Intent::SetInput {
                    id: PROXY.into(),
                    value: written,
                },
                Intent::Focus(PROXY.into()),
            ]);
        }
        _ => return None,
    };
    ui.desktop.proxy_draft = None;
    ui.desktop.proxy = proxy.clone();
    Some(vec![Intent::SetProxy(proxy)])
}

// Linux desktop (escritorio-linux).
/// What the host read before opening the window: who draws its title bar,
/// and the GPUs it can draw with.
pub fn window_known(ui: &mut Ui, title_bar: crate::title_bar::TitleBar, graphics: Graphics) {
    ui.desktop.title_bar = title_bar;
    ui.desktop.graphics = graphics;
}

/// The system's font families, as the host listed them.
pub fn fonts_listed(ui: &mut Ui, families: Vec<String>) {
    ui.desktop.fonts = Some(families);
}

/// Why the chosen font is not shown, or `None` once it is.
pub fn font_problem(ui: &mut Ui, problem: Option<String>) {
    ui.desktop.font_problem = problem;
}

/// Why the local Rich Presence socket did not open, or `None`.
pub fn rich_presence_problem(ui: &mut Ui, problem: Option<String>) {
    ui.desktop.rich_presence_problem = problem;
}

/// What this device says the person is doing now: kept to show, and
/// published as the account's activity when the provider can and the
/// person allowed the source.
pub fn local_activity(
    ui: &mut Ui,
    store: &mut Store,
    activity: Option<LocalActivity>,
) -> Vec<Intent> {
    let allowed = |activity: &&LocalActivity| match activity.source {
        rune_model::ActivitySource::Program => ui.prefs.rich_presence,
        rune_model::ActivitySource::Game => ui.prefs.game_activity,
    };
    let activity = activity.filter(|activity| allowed(&activity));
    // Only to a provider that publishes it, with an account there.
    let publishes = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.local_activity)
        && store.me().is_some();
    let was = ui.desktop.activity.take();
    ui.desktop.activity = activity.clone();
    if !publishes {
        store.set_local_activity(None);
        return Vec::new();
    }
    store.set_local_activity(activity.as_ref().map(shown));
    if was == activity {
        return Vec::new();
    }
    vec![Intent::Command(Command::SetLocalActivity(activity))]
}

/// The publishing again of what this device does, for a new session.
pub fn republish(ui: &mut Ui, store: &mut Store) -> Vec<Intent> {
    let activity = ui.desktop.activity.take();
    local_activity(ui, store, activity)
}

/// How the interface shows an activity of this device.
fn shown(activity: &LocalActivity) -> Activity {
    activity.shown(
        activity
            .name
            .clone()
            .unwrap_or_else(|| t("Una aplicación").into()),
    )
}

/// Open a link a later start of RUNE handed over, as one clicked in a
/// message: a conversation of the provider's opens here; anything else is
/// asked about first unless trusted.
pub fn open_link(ui: &mut Ui, store: &mut Store, url: &str, now: Instant) -> Vec<Intent> {
    match place_of(store, url) {
        Some((channel, message)) => {
            crate::message_actions::links::open_place(ui, store, channel, message, now)
        }
        None => crate::message_actions::links::external(ui, store, url, false),
    }
}

/// What `https://{host}/channels/{server or @me}/{channel}[/{message}]` on
/// one of the provider's own hosts names.
fn place_of(store: &Store, url: &str) -> Option<(ChannelId, Option<rune_model::MessageId>)> {
    let (host, port) = crate::message_actions::links::host(url)?;
    let own = store.provider.as_ref()?.capabilities.trusted_links;
    let ours = own
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")));
    if port || !ours || !url.starts_with("https://") {
        return None;
    }
    let path = url["https://".len()..].split_once('/')?.1;
    let path = path.split(['?', '#']).next()?;
    let mut parts = path.split('/');
    (parts.next()? == "channels").then_some(())?;
    parts.next().filter(|scope| !scope.is_empty())?;
    let channel = ChannelId(parts.next()?.parse().ok()?);
    let message = match parts.next() {
        Some(message) if !message.is_empty() => Some(rune_model::MessageId(message.parse().ok()?)),
        _ => None,
    };
    parts.next().is_none().then_some((channel, message))
}

/// The mentions waiting and the places with something unread, for the tray.
pub fn tray_counts(ui: &Ui, store: &Store, wall: Timestamp) -> (u32, u32) {
    let (home_unread, home_mentions) = crate::conversations::directs_unread(ui, store, wall);
    let mut mentions = home_mentions;
    let mut unread = u32::from(home_unread);
    for server in store.servers() {
        let waiting = crate::channel_list::server_unread(ui, store, server.id, wall);
        let (any, count, _) = crate::server_menu::rail_marks(ui, store, server.id, waiting, wall);
        mentions = mentions.saturating_add(count);
        unread += u32::from(any || count > 0);
    }
    (mentions, unread)
}

/// Mark `channel` a favorite of its server, or no longer.
pub(crate) fn toggle_favorite(ui: &mut Ui, channel: ChannelId) -> Vec<Intent> {
    toggle(&mut ui.prefs.favorites, channel);
    vec![Intent::SaveSetting {
        key: "favorite_channels".into(),
        value: ids(&ui.prefs.favorites),
    }]
}

/// Pin the direct conversation `channel` on top of the list, or no longer.
pub(crate) fn toggle_pinned(ui: &mut Ui, channel: ChannelId) -> Vec<Intent> {
    toggle(&mut ui.prefs.pinned, channel);
    vec![Intent::SaveSetting {
        key: "pinned_directs".into(),
        value: ids(&ui.prefs.pinned),
    }]
}

/// New entries go first, as Serein does; the oldest leave past the bound.
fn toggle(list: &mut Vec<ChannelId>, channel: ChannelId) {
    if let Some(at) = list.iter().position(|kept| *kept == channel) {
        list.remove(at);
    } else {
        list.insert(0, channel);
        list.truncate(KEPT);
    }
}

/// The favorites of `server` the account sees, first; then its tree of
/// categories and channels without them, with the channels it cannot see
/// when asked to show them.
#[allow(clippy::type_complexity)]
pub(crate) fn channel_groups<'a>(
    ui: &Ui,
    store: &'a Store,
    server: ServerId,
) -> (
    Vec<&'a Channel>,
    Vec<(Option<&'a Channel>, Vec<&'a Channel>)>,
) {
    let favorites: Vec<&Channel> = ui
        .prefs
        .favorites
        .iter()
        .filter_map(|channel| store.channel(*channel))
        .filter(|channel| {
            channel.server == Some(server)
                && channel.kind != ChannelKind::Category
                && !channel.is_thread()
                && Store::visible(channel)
        })
        .collect();
    let mut tree = if ui.prefs.show_hidden {
        store.channel_tree_with_hidden(server)
    } else {
        store.channel_tree(server)
    };
    for (_, channels) in &mut tree {
        channels.retain(|channel| !favorites.iter().any(|favorite| favorite.id == channel.id));
    }
    (favorites, tree)
}

/// The direct conversations pinned, then the rest, in `directs`' order.
pub(crate) fn split_pinned<'a>(
    ui: &Ui,
    directs: Vec<&'a Channel>,
) -> (Vec<&'a Channel>, Vec<&'a Channel>) {
    let mut pinned: Vec<&Channel> = directs
        .iter()
        .copied()
        .filter(|channel| ui.prefs.pinned.contains(&channel.id))
        .collect();
    pinned.sort_by_key(|channel| ui.prefs.pinned.iter().position(|id| *id == channel.id));
    let rest = directs
        .into_iter()
        .filter(|channel| !ui.prefs.pinned.contains(&channel.id))
        .collect();
    (pinned, rest)
}

/// A channel the account cannot see, as the column lists it.
pub(crate) fn hidden_row(channel: &Channel, look: &Look) -> Element {
    let glyph = crate::channel_list::glyph(channel.kind).unwrap_or(if channel.kind.is_voice() {
        limn::Icon::Volume
    } else {
        limn::Icon::Hash
    });
    parts::hidden_channel(&channel.name, glyph, Store::private(channel), look)
}

/// The handle on a panel's edge that changes its width.
fn handle(id: &str, label: &str, look: &Look) -> Element {
    ResizeHandle::vertical(id, look.sidebar_palette())
        .thickness(HANDLE)
        .accessibility_label(label)
        .render()
}

/// Over the window: the handle on the boundary of the column of channels.
pub(crate) fn column_handle(ui: &Ui, look: &Look) -> Element {
    let at = rune_components::rail::WIDTH + ui.prefs.sidebar_width - HANDLE / 2.0;
    Element::row(vec![
        Element::spacer().size(Some(at), None),
        handle(
            CHANNELS_HANDLE,
            t("Cambiar el ancho de la lista de canales"),
            look,
        ),
        Element::spacer().flex(1.0),
    ])
}

/// Over the member list: the handle on its left edge.
pub(crate) fn members_handle(look: &Look) -> Element {
    Element::row(vec![
        handle(
            MEMBERS_HANDLE,
            t("Cambiar el ancho de la lista de miembros"),
            look,
        ),
        Element::spacer().flex(1.0),
    ])
}

/// Whether the voice details show the call's encryption.
pub(crate) fn privacy_tab(ui: &Ui) -> bool {
    ui.desktop.privacy_tab
}

/// The call's encryption in the voice details, under the connection.
pub(crate) fn privacy(ui: &Ui, store: &Store, look: &Look) -> Option<Element> {
    store
        .provider
        .as_ref()
        .filter(|info| info.capabilities.voice_privacy_code)?;
    let call = store.call()?;
    let me = store.me();
    let people: Vec<Verifying> = store
        .voice_in(call.channel)
        .into_iter()
        .filter(|state| Some(state.user) != me)
        .take(VERIFIABLE)
        .map(|state| {
            let verification = store.verification(state.user);
            Verifying {
                user: state.user.0,
                name: store.display_name(
                    state.user,
                    store.channel(call.channel).and_then(|c| c.server),
                ),
                code: verification.map(|verification| match verification {
                    Verification::Code(code) => Some(code.clone()),
                    Verification::Asking | Verification::Unknown => None,
                }),
                asking: verification == Some(&Verification::Asking),
            }
        })
        .collect();
    let _ = ui;
    Some(parts::privacy(
        store.privacy_code(),
        &people,
        crate::voice::simulated(store),
        look,
    ))
}

/// Answer an action of this module; `None` when it is not one.
pub(crate) fn update(ui: &mut Ui, store: &mut Store, action: &UiAction) -> Option<Vec<Intent>> {
    let panel = |id: &str| match id {
        CHANNELS_HANDLE => Some(Panel::Channels),
        MEMBERS_HANDLE => Some(Panel::Members),
        _ => None,
    };
    match action {
        UiAction::DragStarted { id, position } => {
            let panel = panel(id)?;
            let width = match panel {
                Panel::Channels => ui.prefs.sidebar_width,
                Panel::Members => ui.prefs.members_width,
            };
            ui.desktop.resizing = Some((panel, position.x, width));
            Some(Vec::new())
        }
        UiAction::DragMoved { id, position, .. } => {
            let panel = panel(id)?;
            if let Some((resizing, from, width)) = ui.desktop.resizing.filter(|(p, ..)| *p == panel)
            {
                let moved = position.x - from;
                match resizing {
                    Panel::Channels => {
                        ui.prefs.sidebar_width = clamp(width + moved, SIDEBAR_WIDTHS);
                    }
                    // Its handle is on its left: going left widens it.
                    Panel::Members => {
                        ui.prefs.members_width = clamp(width - moved, MEMBERS_WIDTHS);
                    }
                }
            }
            Some(Vec::new())
        }
        UiAction::Dropped { id, .. } => {
            let panel = panel(id)?;
            ui.desktop.resizing = None;
            Some(vec![match panel {
                Panel::Channels => save("sidebar_width", ui.prefs.sidebar_width),
                Panel::Members => save("members_width", ui.prefs.members_width),
            }])
        }
        UiAction::DragCancelled { id } => {
            let panel = panel(id)?;
            if let Some((_, _, width)) = ui.desktop.resizing.take() {
                match panel {
                    Panel::Channels => ui.prefs.sidebar_width = width,
                    Panel::Members => ui.prefs.members_width = width,
                }
            }
            Some(Vec::new())
        }
        // A press without moving, or Enter on the handle.
        UiAction::Clicked { id } if panel(id).is_some() => Some(Vec::new()),
        UiAction::Clicked { id } if id == "cl:show-hidden" => {
            ui.prefs.show_hidden = !ui.prefs.show_hidden;
            ui.channel_list.menu = None;
            Some(vec![save("show_hidden_channels", ui.prefs.show_hidden)])
        }
        UiAction::Clicked { id }
            if id == details_tabs::PRIVACY || id == details_tabs::CONNECTION =>
        {
            ui.desktop.privacy_tab = id == details_tabs::PRIVACY;
            Some(Vec::new())
        }
        UiAction::Clicked { id } if id == "e2ee:copy" => Some(
            store
                .privacy_code()
                .map(|code| vec![Intent::Copy(code.to_owned())])
                .unwrap_or_default(),
        ),
        // Desktop session and games (escritorio-sesion-y-juegos): the
        // fields of a game's name and of the proxy's address.
        UiAction::InputChanged { id, value } if id == crate::games::NAME => {
            crate::games::typed(ui, value);
            Some(Vec::new())
        }
        UiAction::Submitted { id, .. } if id == crate::games::NAME => {
            Some(crate::games::save_name(ui))
        }
        UiAction::InputChanged { id, value } if id == PROXY => {
            ui.desktop.proxy_draft = Some(value.clone());
            ui.desktop.proxy_problem = None;
            Some(Vec::new())
        }
        UiAction::Submitted { id, .. } if id == PROXY => Some(save_proxy(ui)),
        UiAction::Clicked { id } => {
            let user = rune_model::UserId(id.strip_prefix("e2ee:verify:")?.parse().ok()?);
            let channel = store.call()?.channel;
            if !store.begin_verification(user) {
                return Some(Vec::new());
            }
            Some(vec![Intent::Command(Command::VoiceVerificationCode {
                channel,
                user,
            })])
        }
        _ => None,
    }
}

fn clamp(width: f32, range: RangeInclusive<f32>) -> f32 {
    width.round().clamp(*range.start(), *range.end())
}

/// A click of the settings of this module, `rest` after `us:`.
pub(crate) fn settings_clicked(
    ui: &mut Ui,
    store: &mut Store,
    rest: &str,
    now: Instant,
) -> Option<Vec<Intent>> {
    // Desktop session and games (escritorio-sesion-y-juegos).
    if let Some(intents) = crate::games::clicked(ui, rest) {
        return Some(intents);
    }
    if let Some(mode) = rest.strip_prefix("proxy=") {
        return choose_proxy(ui, mode);
    }
    match rest {
        "autostart-minimized" => {
            if ui.desktop.autostart != Some(true) {
                return Some(Vec::new());
            }
            let minimized = !ui.desktop.autostart_minimized;
            ui.desktop.autostart = None;
            return Some(vec![Intent::SetAutostart {
                on: true,
                minimized,
            }]);
        }
        // Only on, by the person's click: turning it off is the account's.
        "activity-sharing-on" => {
            let able = store
                .provider
                .as_ref()
                .is_some_and(|info| info.capabilities.activity_sharing);
            if !able
                || store.activity_sharing() != Some(false)
                || store.activity_sharing_asked().is_some()
            {
                return Some(Vec::new());
            }
            store.ask_activity_sharing(true);
            return Some(vec![Intent::Command(Command::SetActivitySharing(true))]);
        }
        "select:proxy" => {
            let open = ui.settings_state.select == Some("proxy");
            ui.settings_state.select = (!open).then_some("proxy");
            return Some(Vec::new());
        }
        "proxy-save" => return Some(save_proxy(ui)),
        "proxy-cancel" => {
            ui.desktop.proxy_draft = None;
            ui.desktop.proxy_problem = None;
            return Some(Vec::new());
        }
        _ => {}
    }
    let prefs = &mut ui.prefs;
    let flip = |value: &mut bool, key: &str| {
        *value = !*value;
        vec![save(key, *value)]
    };
    // Any shortcut of the table (atajos-y-navegacion).
    if let Some(key) = rest.strip_prefix("key-record=") {
        let action = crate::keybinds::Action::from_key(key)?;
        let state = &mut ui.settings_state;
        state.recording = match state.recording {
            Some(_) if state.recording_for == action => None,
            _ => Some(now),
        };
        state.recording_for = action;
        state.key_notice = None;
        return Some(Vec::new());
    }
    if let Some(key) = rest.strip_prefix("key-reset=") {
        let action = crate::keybinds::Action::from_key(key)?;
        return Some(crate::keybinds::reset(ui, action));
    }
    if let Some(index) = rest.strip_prefix("font=") {
        let family = ui
            .desktop
            .fonts
            .as_ref()?
            .get(index.parse::<usize>().ok()?)?
            .clone();
        ui.settings_state.select = None;
        ui.desktop.font_problem = None;
        ui.prefs.ui_font = Some(family.clone());
        return Some(vec![save("ui_font", family)]);
    }
    // The window's own title bar (barra-e-icono).
    if let Some(key) = rest.strip_prefix("title-bar=") {
        use crate::title_bar::TitleBar;
        let choice = match key {
            "auto" => TitleBar::Automatic,
            "rune" => TitleBar::Rune,
            "system" => TitleBar::System,
            _ => return None,
        };
        ui.settings_state.select = None;
        ui.desktop.title_bar = choice;
        return Some(vec![Intent::TitleBar(choice)]);
    }
    // Linux desktop (escritorio-linux).
    if let Some(key) = rest.strip_prefix("gpu=") {
        let graphics = &mut ui.desktop.graphics;
        let chosen = match key {
            "auto" => None,
            key => Some(
                graphics
                    .gpus
                    .iter()
                    .find(|(known, _)| known == key)?
                    .0
                    .clone(),
            ),
        };
        ui.settings_state.select = None;
        graphics.chosen = chosen.clone();
        return Some(vec![Intent::DrawWith(chosen)]);
    }
    Some(match rest {
        // The window's own title bar (barra-e-icono).
        "select:title-bar" => {
            let open = ui.settings_state.select == Some("title-bar");
            ui.settings_state.select = (!open).then_some("title-bar");
            Vec::new()
        }
        "select:gpu" => {
            let open = ui.settings_state.select == Some("gpu");
            ui.settings_state.select = (!open).then_some("gpu");
            Vec::new()
        }
        "select:font" => {
            let open = ui.settings_state.select == Some("font");
            ui.settings_state.select = (!open).then_some("font");
            if !open && ui.desktop.fonts.is_none() {
                vec![Intent::ListFonts]
            } else {
                Vec::new()
            }
        }
        "font-default" => {
            ui.settings_state.select = None;
            ui.desktop.font_problem = None;
            prefs.ui_font = None;
            vec![Intent::SaveSetting {
                key: "ui_font".into(),
                value: None,
            }]
        }
        "tray-close" => flip(&mut prefs.close_to_tray, "close_to_tray"),
        "autostart" => {
            let on = !ui.desktop.autostart?;
            ui.desktop.autostart = None;
            vec![Intent::SetAutostart {
                on,
                minimized: on && ui.desktop.autostart_minimized,
            }]
        }
        "widths-reset" => {
            let usual = Prefs::default();
            prefs.sidebar_width = usual.sidebar_width;
            prefs.members_width = usual.members_width;
            vec![
                save("sidebar_width", prefs.sidebar_width),
                save("members_width", prefs.members_width),
            ]
        }
        "hidden-channels" => flip(&mut prefs.show_hidden, "show_hidden_channels"),
        "rich-presence" => flip(&mut prefs.rich_presence, "rich_presence"),
        "game-activity" => flip(&mut prefs.game_activity, "game_activity"),
        _ => return None,
    })
}
