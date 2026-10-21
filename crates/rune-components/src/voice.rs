//! Voice, from plain props: the panel of the account's call above its own
//! panel, the popover of the connection's details, the people in a voice
//! channel under it in the column, and the call's stage where the
//! conversation goes: its tiles and the bar of controls under them, or the
//! invitation to join while the account is not in it.
//!
//! What a provider cannot do is shown disabled with its reason, never left to
//! fail: with no video, the camera and the screen say so.

use limn::{
    Button, ButtonSizing, ButtonVariant, ChartSeries, Color, Component, ComponentPalette,
    DropdownMenu, DropdownMenuEntry, DropdownMenuItem, Element, FontFamily, Icon, IconButton,
    LineChart, PopoverPlacement, RadiusRole, Size, Slider, Tooltip, TooltipPlacement, TooltipSpec,
};
use rune_i18n::{es, t, tr};
use rune_model::{CallPhase, CallQuality, Rgb, StreamQuality};

use crate::{centered, gap, icon, line, strong_line, text, Look};

/// Signal bars, for the connection.
pub const SIGNAL: &str = crate::glyphs::glyph!("signal");

/// The actions these pieces report. Those that name a person are built with
/// the functions here.
pub mod ids {
    /// The microphone and the sound of the account in its call.
    pub const MUTE: &str = "vz:mute";
    pub const DEAFEN: &str = "vz:deafen";
    /// Hang up, from the panel or from the stage.
    pub const LEAVE: &str = "vz:leave";
    /// Join the channel the stage shows, or try again after a failure there.
    pub const JOIN: &str = "vz:join";
    /// The panel's way to try again once the call failed.
    pub const RETRY: &str = "vz:retry";
    /// The connection's details: the signal of the panel, and its title.
    pub const DETAILS: &str = "vz:details";
    pub const DETAILS_TITLE: &str = "vz:details:title";
    pub const DETAILS_DISMISS: &str = "vz:details:dismiss";
    /// The bar's «Más opciones» and its menu.
    pub const MORE: &str = "vz:more";
    pub const MORE_DISMISS: &str = "vz:more:dismiss";
    pub const VOICE_SETTINGS: &str = "vz:voice-settings";
    /// The camera and the screen of the account, on and off.
    pub const CAMERA: &str = "vz:camera";
    pub const SCREEN: &str = "vz:screen";
    /// «Dejar de compartir» and «Apagar la cámara» on their previews.
    pub const STOP_SCREEN: &str = "vz:stop-screen";
    /// The quality picker that opens before sharing the screen: its
    /// sizes (`vz:quality:720`), rates (`vz:fps:30`), and going live.
    pub const QUALITY: &str = "vz:quality:";
    pub const FPS: &str = "vz:fps:";
    pub const SHARE: &str = "vz:share";
    /// «Con sonido» in the picker: the applications' sound goes along.
    pub const SOUND: &str = "vz:sound";
    /// «Elegir otra pantalla» in the picker: the system's asks again.
    pub const PICK_SCREEN: &str = "vz:pick-screen";
    /// A screen or a window in RUNE's own picker, where the system has
    /// none (Windows): `vz:source:{id}`, by the host's id of it.
    pub const SOURCE: &str = "vz:source:";
    pub const SHARE_DISMISS: &str = "vz:share:dismiss";
    pub const STOP_CAMERA: &str = "vz:stop-camera";
    /// The surfaces the host presents the previews' frames on.
    pub const SCREEN_SURFACE: &str = "video:screen";
    pub const CAMERA_SURFACE: &str = "video:camera";
    /// «Dejar de ver» on a stream watched: `vz:stop-watching:{persona}`.
    pub const STOP_WATCHING: &str = "vz:stop-watching:";
    /// «Pausar» and «Reanudar» on the preview of the shared screen.
    pub const PAUSE: &str = "vz:pause";
    pub const RESUME: &str = "vz:resume";
    /// «Salir de pantalla completa» on a stream shown alone.
    pub const FULLSCREEN_EXIT: &str = "vz:fullscreen-exit";
    /// The stage's header: the channel's chat.
    pub const CHAT: &str = "vz:chat";
    /// A person's menu, and their card.
    pub const MENU_DISMISS: &str = "vz:menu:dismiss";
    pub const CARD_DISMISS: &str = "vz:card:dismiss";
    /// Items of a person's menu, followed by their ID.
    pub const PROFILE: &str = "vz:menu:profile:";
    pub const MENTION: &str = "vz:menu:mention:";
    pub const COPY_ID: &str = "vz:menu:copy-id:";
    /// The account's own microphone and sound, from its menu.
    pub const MENU_MUTE: &str = "vz:menu:mute";
    pub const MENU_DEAFEN: &str = "vz:menu:deafen";
    /// The arrows beside the bar's microphone and sound: the quick voice
    /// panel of each, [`crate::quick_voice`].
    pub const INPUT_OPTIONS: &str = "vz:input-options";
    pub const OUTPUT_OPTIONS: &str = "vz:output-options";
    /// The arrow beside the bar's camera: the camera's panel
    /// (llamadas-y-panel-de-voz), [`crate::quick_voice::camera`].
    pub const CAMERA_OPTIONS: &str = "vz:camera-options";

    /// Someone's row under a voice channel: a click opens their card.
    pub fn row(user: u64) -> String {
        format!("vz:user:{user}")
    }

    /// Someone's tile on the stage.
    pub fn tile(user: u64) -> String {
        format!("vz:tile:{user}")
    }

    /// The «⋯» of someone's tile, shown while the pointer is on it.
    pub fn tile_more(user: u64) -> String {
        format!("vz:tile-more:{user}")
    }

    /// How loud someone plays for the account, from their menu.
    pub fn volume(user: u64) -> String {
        format!("vz:volume:{user}")
    }

    /// Silence someone for the account only, from their menu.
    pub fn local_mute(user: u64) -> String {
        format!("vz:mute:{user}")
    }

    /// «Ver transmisión» on the tile of someone streaming.
    pub fn watch(user: u64) -> String {
        format!("vz:watch:{user}")
    }

    /// The picture of someone's stream under their row in the column:
    /// clicked, it is watched.
    pub fn preview(user: u64) -> String {
        format!("vz:preview:{user}")
    }

    // Rows in the column (voz-filas-y-arrastre).
    /// The card of someone live beside their row, shown on hover.
    pub const LIVE_DISMISS: &str = "vz:live:dismiss";

    /// «Ver transmisión» on the card of someone live beside their row.
    pub fn live_watch(user: u64) -> String {
        format!("vz:live:watch:{user}")
    }

    /// «Dejar de ver» on `user`'s stream.
    pub fn stop_watching(user: u64) -> String {
        format!("{STOP_WATCHING}{user}")
    }

    /// «Pantalla completa» on `user`'s stream: alone, the window full screen.
    pub fn fullscreen(user: u64) -> String {
        format!("vz:fullscreen:{user}")
    }

    /// «Ventana aparte» on `user`'s stream: in a window of its own too.
    pub fn window(user: u64) -> String {
        format!("vz:window:{user}")
    }

    /// The surface the host presents `user`'s stream on in its own window.
    pub fn window_surface(user: u64) -> String {
        format!("video:window:{user}")
    }

    /// The surface the host presents someone's stream on.
    pub fn stream_surface(user: u64) -> String {
        format!("video:stream:{user}")
    }

    /// The surface the host presents someone's camera on, in their tile.
    pub fn camera_surface(user: u64) -> String {
        format!("video:camera:{user}")
    }

    // Streams (transmisiones).
    /// `user`'s stream watched on the stage: a click focuses it, or lets go
    /// of the focus, a double click shows it full screen, a secondary click
    /// opens its menu.
    pub fn stream_tile(user: u64) -> String {
        format!("vz:stream:{user}")
    }

    /// The «⋯» of `user`'s stream, shown while the pointer is on it: its
    /// menu, anchored there.
    pub fn stream_more(user: u64) -> String {
        format!("vz:stream-more:{user}")
    }

    /// «Enfocar» `user`'s stream, or «Volver a la grilla» while it is.
    pub fn spotlight(user: u64) -> String {
        format!("vz:spotlight:{user}")
    }
}

/// Why the camera and the screen cannot be used on this system.
pub const NO_VIDEO: &str =
    es("La vista previa de pantalla y cámara solo funciona en Linux por ahora");

/// The account's camera and screen, as the call's controls show them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VideoControls {
    pub screen: bool,
    pub camera: bool,
    /// Why they cannot be used here, if they cannot.
    pub denied: Option<String>,
    /// What their hints add: that what they show stays on this machine.
    pub note: String,
    /// What the screen's hint adds, when it is not `note`.
    pub screen_note: Option<String>,
    /// What the camera's hint adds, when it is not `note`.
    pub camera_note: Option<String>,
    /// The quality picker, open on the bar's screen button, and what is
    /// chosen in it.
    pub picker: Option<StreamQuality>,
    /// The applications' sound goes along with the screen.
    pub sound: bool,
    /// The system remembers the screen chosen: it can be chosen anew.
    pub pick_again: bool,
    // Windows' screen picker (windows-medios).
    /// What RUNE's own picker offers to share, where the system has no
    /// dialog of its own (Windows); empty elsewhere.
    pub sources: Vec<ScreenChoice>,
}

/// A screen or a window RUNE's own picker offers, the one chosen checked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScreenChoice {
    pub id: String,
    pub name: String,
    pub window: bool,
    pub chosen: bool,
}

/// The camera's and the screen's buttons, `height` high on `base`.
fn video_buttons(controls: &VideoControls, height: f32, base: Color, look: &Look) -> [Element; 2] {
    let button = |id: &str, on: bool, glyph: Icon, titles: (&str, &str)| {
        let (fill, ink) = if on {
            (Look::alpha(look.online, 40), look.online)
        } else {
            (base, look.text)
        };
        let button = IconButton::new(id, glyph, filled(fill, lighter(fill, 16), look))
            .variant(ButtonVariant::Raised)
            .sizing(ButtonSizing {
                height,
                font_size: 14.0,
                icon_size: 20.0,
                horizontal_padding: 8.0,
                inner_spacing: 6.0,
            })
            .radius(RadiusRole::Card)
            .active(on);
        match &controls.denied {
            Some(reason) => button
                .ink(look.faint)
                .tooltip_with(denied_hint(titles.1, t(reason)))
                .disabled()
                .render(),
            None => button
                .ink(ink)
                .hover_ink(ink)
                .tooltip_with(
                    TooltipSpec::new(if on { titles.0 } else { titles.1 }).description(
                        match (id, &controls.screen_note, &controls.camera_note) {
                            (ids::SCREEN, Some(note), _) | (ids::CAMERA, _, Some(note)) => {
                                note.clone()
                            }
                            _ => controls.note.clone(),
                        },
                    ),
                )
                .render(),
        }
    };
    [
        button(
            ids::CAMERA,
            controls.camera,
            if controls.camera {
                Icon::Video
            } else {
                Icon::VideoOff
            },
            (t("Apagar la cámara"), t("Encender la cámara")),
        ),
        button(
            ids::SCREEN,
            controls.screen,
            Icon::Screen,
            (t("Dejar de compartir"), t("Compartir pantalla")),
        ),
    ]
}

/// The account's screen or camera on the stage, shown only here.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    /// The surface the host presents its frames on.
    pub surface: String,
    /// «Tu pantalla», «Tu cámara», «Pantalla de Ana».
    pub title: String,
    /// The size of the frame on the surface; `None` while the system asks
    /// or the first frame is on its way.
    pub frame: Option<(u32, u32)>,
    /// What shows until then: «Esperando al sistema…».
    pub waiting: String,
    /// What stops it, and how it reads.
    pub stop: (String, &'static str),
    /// That it stays on this machine.
    pub note: String,
    /// What else it offers beside the way to stop it, and how each reads:
    /// «Pausar».
    pub actions: Vec<(String, &'static str)>,
    // Streams (transmisiones).
    /// A stream watched: what its tile offers in place of `stop` and
    /// `actions`.
    pub stream: Option<StreamTile>,
    /// It takes the stage, the rest in a strip under it.
    pub focused: bool,
}

/// A stream watched, as its tile offers it (transmisiones), as in the
/// official client: a click focuses it, a double click shows it full
/// screen, and under the pointer its sound and «⋯» show at its top right.
/// While it is focused, its window and full screen go at the bar's end and
/// hanging up gives way to «Dejar de ver».
#[derive(Clone, Debug, PartialEq)]
pub struct StreamTile {
    pub owner: u64,
    /// The key of its sound for the account's mixer, apart from the voice's.
    pub sound: u64,
    /// Its sound is silenced for the account.
    pub silenced: bool,
    /// It shows in a window of its own already.
    pub windowed: bool,
    /// Its menu, while open from its «⋯».
    pub menu: Option<Vec<MenuEntry>>,
}

/// RUNE's own glyphs for «Dejar de ver» and «Ventana aparte».
const SCREEN_STOP: &str = crate::glyphs::glyph!("screen-stop");
const POP_OUT: &str = crate::glyphs::glyph!("pop-out");

/// A preview's tile: the frame as large as fits, its note and the way to
/// stop it on top, its title in a chip at the bottom left. The camera's is
/// mirrored, as a webcam shows the person in front of it (as in Serein);
/// what goes to the call is not.
pub fn preview(view: &Preview, size: Size, look: &Look) -> Element {
    preview_tile(view, size, false, look)
}

/// [`preview`], or `compact` in the strip under a stream focused: its
/// picture and its title only.
fn preview_tile(view: &Preview, size: Size, compact: bool, look: &Look) -> Element {
    let picture = match view.frame {
        Some((width, height)) => {
            let scale = (size.width / width as f32).min(size.height / height as f32);
            centered(
                Element::surface(view.surface.clone(), view.title.clone())
                    .surface_mirrored(view.surface == ids::CAMERA_SURFACE)
                    .size(Some(width as f32 * scale), Some(height as f32 * scale)),
            )
        }
        None => centered(line(&view.waiting, 14.0, look.muted)),
    };
    let chip = |child: Element| {
        Element::panel(child.padding(8.0))
            .size(None, Some(34.0))
            .radius(8.0)
            .background(Look::alpha(look.rail, 200))
    };
    let stop = Button::new(view.stop.0.clone(), t(view.stop.1), look.palette())
        .variant(ButtonVariant::Destructive)
        .icon(Icon::StopFill)
        .render();
    // What else it offers goes at the top, so a narrow tile keeps its title.
    let mut top = vec![
        chip(line(&view.note, 13.0, look.text)).shrink_to_fit(),
        Element::spacer().flex(1.0),
    ];
    top.extend(view.actions.iter().map(|(id, label)| {
        Button::new(id.clone(), t(label), look.palette())
            .variant(ButtonVariant::Secondary)
            .render()
    }));
    let title = chip(strong_line(&view.title, 14.0, look.text)).shrink_to_fit();
    let over = match &view.stream {
        _ if compact => Element::column(vec![
            Element::spacer().flex(1.0),
            Element::row(vec![title, Element::spacer().flex(1.0)]),
        ])
        .padding(8.0),
        // A stream watched: its ways out are in its menu and the bar.
        Some(stream) => Element::column(vec![
            Element::row(vec![
                chip(line(&view.note, 13.0, look.text)).shrink_to_fit(),
                Element::spacer().flex(1.0),
                stream_buttons(stream, look),
            ])
            .gap(8.0),
            Element::spacer().flex(1.0),
            Element::row(vec![title, Element::spacer().flex(1.0)]),
        ])
        .padding(12.5),
        None => Element::column(vec![
            Element::row(top).gap(8.0),
            Element::spacer().flex(1.0),
            Element::row(vec![title, Element::spacer().flex(1.0), stop]).gap(8.0),
        ])
        .padding(12.5),
    };
    let shown = Element::stack(vec![
        Element::panel(picture)
            .size(Some(size.width), Some(size.height))
            .radius(TILE_RADIUS)
            .background(Color::rgb(0, 0, 0))
            .clip_children(),
        over,
    ]);
    match &view.stream {
        Some(stream) => Element::clickable(ids::stream_tile(stream.owner), shown)
            .radius(TILE_RADIUS)
            .hover_region(),
        None => shown,
    }
    .size(Some(size.width), Some(size.height))
    .accessibility_label(view.title.clone())
}

/// A stream's sound and «⋯» at its tile's top right, 32 across, shown
/// while the pointer is on it, as the official client's: the sound stays
/// while silenced, the «⋯» while its menu is open.
fn stream_buttons(stream: &StreamTile, look: &Look) -> Element {
    let button = |id: String, glyph: Icon, hint: &str| {
        IconButton::new(
            id,
            glyph,
            filled(Look::alpha(look.rail, 200), look.raised, look),
        )
        .variant(ButtonVariant::Raised)
        .sizing(ButtonSizing {
            height: 32.0,
            font_size: 14.0,
            icon_size: 18.0,
            horizontal_padding: 7.0,
            inner_spacing: 6.0,
        })
        .width(32.0)
        .radius(RadiusRole::Card)
        .ink(look.text)
        .hover_ink(look.text)
        .tooltip(hint)
        .render()
    };
    let reveal = |shown: Element, kept: bool, what: &str| {
        if kept {
            shown
        } else {
            shown.reveal_on_hover(format!("{}:{what}", ids::stream_tile(stream.owner)))
        }
    };
    let (glyph, hint) = if stream.silenced {
        (Icon::VolumeMuted, t("Activar el sonido"))
    } else {
        (Icon::Volume, t("Silenciar"))
    };
    let sound = button(ids::local_mute(stream.sound), glyph, hint);
    let open = stream.menu.is_some();
    let more = DropdownMenu::new(
        button(
            ids::stream_more(stream.owner),
            Icon::More,
            t("Más opciones"),
        ),
        menu_entries(stream.menu.clone().unwrap_or_default(), look),
        open,
        ids::MENU_DISMISS,
        look.menu_palette(),
    )
    .context_menu_style()
    .width(MENU_WIDTH)
    .placement(PopoverPlacement::BottomEnd)
    .render();
    Element::row(vec![
        reveal(sound, stream.silenced, "sound"),
        reveal(more, open, "more"),
    ])
    .gap(8.0)
}

/// A stream in a window of its own: the frame as large as fits, its note
/// and its title, nothing to click (the window's own close lets it go).
pub fn stream_window(view: &Preview, size: Size, look: &Look) -> Element {
    let picture = match view.frame {
        Some((width, height)) => {
            let scale = (size.width / width as f32).min(size.height / height as f32);
            centered(
                Element::surface(view.surface.clone(), view.title.clone())
                    .size(Some(width as f32 * scale), Some(height as f32 * scale)),
            )
        }
        None => centered(line(&view.waiting, 14.0, look.muted)),
    };
    let chip = |child: Element| {
        Element::panel(child.padding(8.0))
            .size(None, Some(34.0))
            .radius(8.0)
            .background(Look::alpha(look.rail, 200))
            .shrink_to_fit()
    };
    Element::stack(vec![
        Element::panel(picture)
            .size(Some(size.width), Some(size.height))
            .background(Color::rgb(0, 0, 0))
            .clip_children(),
        Element::column(vec![
            Element::row(vec![
                chip(line(&view.note, 13.0, look.text)),
                Element::spacer().flex(1.0),
            ]),
            Element::spacer().flex(1.0),
            Element::row(vec![
                chip(strong_line(&view.title, 14.0, look.text)),
                Element::spacer().flex(1.0),
            ]),
        ])
        .padding(12.5),
    ])
    .size(Some(size.width), Some(size.height))
    .accessibility_label(view.title.clone())
}

/// The voice section of the account's box: 112 high, its controls 14.5 in.
pub const PANEL_HEIGHT: f32 = 112.0;
const PANEL_INSET: f32 = 14.5;
/// The signal, 34 by 32.5, and the panel's buttons, 34 high, 12.5 apart.
const SIGNAL_SIZE: (f32, f32) = (34.0, 32.5);
const PANEL_BUTTON: f32 = 34.0;
const PANEL_GAP: f32 = 12.5;
/// The connection's details: 319 wide.
pub const DETAILS_WIDTH: f32 = 319.0;
/// A person's row under a voice channel: 34 high, 30 in from the channel's
/// row, their mark 25 across. Its highlight is 1 short above and below, so
/// two rows' never touch, and ends where a channel's does
/// (`sidebar::INSET` from the column's edge), rounded the same 6.
pub const ROW_HEIGHT: f32 = 34.0;
const ROW_INDENT: f32 = 38.0;
const ROW_AVATAR: f32 = 25.0;
const ROW_RADIUS: f32 = 6.0;
/// The card of someone live, beside their row: 12 around a 288 by 162
/// picture of their stream.
pub const LIVE_CARD_WIDTH: f32 = 312.0;
const LIVE_PICTURE: (f32, f32) = (288.0, 162.0);
/// Tiles keep 16:9, 8.5 apart, rounded 8, with 17 around them.
const TILE_GAP: f32 = 8.5;
const TILE_RADIUS: f32 = 8.0;
const STAGE_INSET: f32 = 17.0;
// Streams (transmisiones).
/// The strip under a stream focused: 18 % of the stage, between these.
const MIN_STRIP: f32 = 72.0;
const MAX_STRIP: f32 = 124.0;
/// The least a stream focused keeps; a shorter stage has no strip.
const MIN_FOCUS: f32 = 180.0;
/// The bar of the stage, as the official client's tray (REA): its buttons
/// 36 across, 8 apart in sections 4 in, outlined and rounded 12, 46 high;
/// sections and hanging up (62 wide) 12 apart, and 12 under the tiles;
/// 16 over the bottom and in from the sides, where its end keeps a
/// stream's own.
pub const BAR_HEIGHT: f32 = 46.0;
const BAR_BOTTOM: f32 = 16.0;
const BAR_BUTTON: f32 = 36.0;
const BAR_GAP: f32 = 12.0;
const HANG_UP_WIDTH: f32 = 62.0;
/// «Más opciones», 330 wide.
const MORE_WIDTH: f32 = 330.0;

/// "03:27", or "1:02:09" past the hour.
pub fn clock(duration: std::time::Duration) -> String {
    let seconds = duration.as_secs();
    let (hours, minutes, seconds) = (seconds / 3_600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// "5 min", or "1 h 05 min" past the hour: how long a voice channel has
/// had someone in it, to the minute (never under one).
pub fn on_for(duration: std::time::Duration) -> String {
    let minutes = (duration.as_secs() / 60).max(1);
    let (hours, minutes) = (minutes / 60, minutes % 60);
    if hours > 0 {
        tr!(
            "{hours} h {minutes} min",
            hours,
            minutes = format!("{minutes:02}")
        )
    } else {
        tr!("{minutes} min", minutes)
    }
}

/// "0,4 %": packets lost, per thousand, as a person reads it.
pub fn loss(permille: u16) -> String {
    tr!(
        "{whole},{tenth} %",
        whole = permille / 10,
        tenth = permille % 10
    )
}

/// What the call's title says in `phase`, and its color.
fn phase_title(phase: &CallPhase, simulated: bool, look: &Look) -> (String, Color) {
    match phase {
        CallPhase::Joining => (t("Conectando a la voz…").into(), look.idle),
        CallPhase::Securing => (t("Asegurando el audio…").into(), look.idle),
        CallPhase::Connected if simulated => (t("Voz conectada (simulada)").into(), look.online),
        CallPhase::Connected => (t("Voz conectada").into(), look.online),
        CallPhase::Waiting if simulated => {
            (t("Esperando a los demás (simulada)").into(), look.online)
        }
        CallPhase::Waiting => (t("Esperando a los demás").into(), look.online),
        CallPhase::Reconnecting { .. } => (t("Reconectando la voz…").into(), look.idle),
        CallPhase::Failed { .. } => (t("No se pudo conectar la voz").into(), look.danger),
    }
}

/// A palette whose raised fill is `fill`, for a button drawn on it.
fn filled(fill: Color, hover: Color, look: &Look) -> ComponentPalette {
    ComponentPalette {
        raised: fill,
        hover,
        selected: hover,
        ..look.palette()
    }
}

/// `color` a little lighter, for the hover of a filled button.
fn lighter(color: Color, by: u8) -> Color {
    Color::rgba(
        color.r.saturating_add(by),
        color.g.saturating_add(by),
        color.b.saturating_add(by),
        color.a.max(by),
    )
}

/// The hint of a control that cannot be used: what it would do, and why not.
fn denied_hint(title: &str, reason: &str) -> TooltipSpec {
    TooltipSpec::new(title).description(reason)
}

/// The account's call, as its panel shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub channel: String,
    /// The server the channel is in, after it: «General / RUNE prueba».
    pub server: Option<String>,
    pub phase: CallPhase,
    pub duration: Option<std::time::Duration>,
    pub quality: Option<CallQuality>,
    /// Joining and quality are simulated by the provider: no audio is carried.
    pub simulated: bool,
    pub video: VideoControls,
}

/// The voice section above the account's panel: the signal (its hint the
/// latency, a click its details), how far the call got and where, its clock
/// and the way to hang up; then the camera and the screen, lit while on.
/// While it failed, the reason and a way to try again.
/// `details` floats over it while open.
pub fn panel(view: &Panel, details: Option<Element>, look: &Look) -> Element {
    let (title, ink) = phase_title(&view.phase, view.simulated, look);
    let connected = view.phase.is_connected();
    let latency = match view.quality.filter(|_| connected) {
        Some(quality) => format!("{} ms", quality.rtt_ms),
        None => t("Sin medidas").into(),
    };
    let signal = IconButton::new(
        ids::DETAILS,
        Icon::Custom(SIGNAL),
        filled(look.raised, lighter(look.raised, 14), look),
    )
    .variant(ButtonVariant::Raised)
    .sizing(ButtonSizing {
        height: SIGNAL_SIZE.1,
        font_size: 14.0,
        icon_size: 20.0,
        horizontal_padding: 8.0,
        inner_spacing: 6.0,
    })
    .width(SIGNAL_SIZE.0)
    .radius(RadiusRole::Card)
    .ink(ink)
    .hover_ink(ink)
    .tooltip(latency)
    .render();
    let signal = match details {
        Some(details) => Element::popover(
            signal,
            Element::panel(details)
                .size(Some(DETAILS_WIDTH), None)
                .background(look.menu)
                .border(look.border, 1.0)
                .radius(8.0)
                .shadow(look.shadow, limn::Point::new(0.0, 8.0), 20.0, 0.0)
                .clip_children(),
            ids::DETAILS_DISMISS,
            PopoverPlacement::TopStart,
        )
        .popover_gap(10.0),
        None => signal,
    };
    let mut place = view.channel.clone();
    if let Some(server) = &view.server {
        place = format!("{place} / {server}");
    }
    let subtitle = match &view.phase {
        CallPhase::Failed { reason } => reason.clone(),
        CallPhase::Reconnecting { attempt } => tr!("Intento {attempt} · {place}", attempt, place),
        _ => place,
    };
    // The title reads «Detalles de voz» under the pointer: it opens them too.
    let heading = Element::stack(vec![
        strong_line(title, 14.0, ink).conceal_on_hover("vz:details:title:phase"),
        strong_line(t("Detalles de voz"), 14.0, ink).reveal_on_hover("vz:details:title:hover"),
    ]);
    // The call's clock beside where it is, the title keeping the whole width.
    let mut under = vec![line(subtitle, 13.0, look.muted).flex(1.0)];
    if let Some(duration) = view.duration.filter(|_| connected) {
        under.push(
            Element::text(clock(duration))
                .font_family(FontFamily::Monospace)
                .font_size(13.0)
                .foreground(look.online),
        );
    }
    let summary = Element::clickable(
        ids::DETAILS_TITLE,
        Element::column(vec![heading, Element::row(under).gap(6.0)]).gap(1.0),
    )
    .hover_region()
    .accessibility_label(t("Detalles de voz"));
    let mut first = vec![signal.centered_y(), summary.flex(1.0).centered_y()];
    first.push(
        IconButton::new(ids::LEAVE, Icon::HangUp, look.sidebar_palette())
            .variant(ButtonVariant::Ghost)
            .sizing(ButtonSizing {
                height: PANEL_BUTTON,
                font_size: 14.0,
                icon_size: 20.0,
                horizontal_padding: 9.0,
                inner_spacing: 6.0,
            })
            .radius(RadiusRole::Card)
            .ink(look.muted)
            .hover_ink(look.text)
            .tooltip(t("Desconectar"))
            .render()
            .centered_y(),
    );
    let second = if let CallPhase::Failed { .. } = view.phase {
        Element::row(vec![Button::new(
            ids::RETRY,
            t("Reintentar"),
            look.sidebar_palette(),
        )
        .variant(ButtonVariant::Secondary)
        .render()
        .flex(1.0)])
    } else {
        Element::row(
            video_buttons(&view.video, PANEL_BUTTON, look.raised, look)
                .map(|button| button.flex(1.0))
                .into(),
        )
        .gap(PANEL_GAP)
    };
    Element::panel(
        Element::column(vec![
            Element::row(first).gap(8.0).size(None, Some(33.0)),
            second.size(None, Some(PANEL_BUTTON)),
        ])
        .gap(13.5)
        .padding(PANEL_INSET),
    )
    .size(None, Some(PANEL_HEIGHT))
    .radius(8.0)
    .background(look.rail)
    .border(look.border, 1.0)
}

/// The connection's details, as its popover shows them.
#[derive(Clone, Debug, PartialEq)]
pub struct Details {
    /// The round trips measured in this call, oldest first, each with when
    /// it was taken, as a person reads the time.
    pub samples: Vec<(String, u32)>,
    /// Packets lost, per thousand, at the last measure.
    pub loss_permille: Option<u16>,
    pub simulated: bool,
}

/// The popover of the connection: its latency over the call, its average
/// and last, and the packets lost. Values that are made up say so.
pub fn details(view: &Details, look: &Look) -> Element {
    details_tabbed(view, None, look)
}

/// The tabs of the voice details beside «Conexión» (P6).
pub mod details_tabs {
    pub const CONNECTION: &str = "vz:details:tab=connection";
    pub const PRIVACY: &str = "vz:details:tab=privacy";
}

/// [`details`] with a «Privacidad» tab beside «Conexión» (P6): with
/// `privacy`, its tab shows instead of the connection's while `open`.
pub fn details_tabbed(view: &Details, privacy: Option<(Element, bool)>, look: &Look) -> Element {
    let tab = |id: &str, label: &str, width: f32, chosen: bool| {
        let ink = if chosen { look.accent } else { look.muted };
        let underline = Element::panel(Element::spacer())
            .size(Some(width), Some(2.0))
            .radius(1.0)
            .background(if chosen {
                look.accent
            } else {
                Color::rgba(0, 0, 0, 0)
            });
        let column = Element::column(vec![strong_line(label, 14.0, ink), underline]).gap(8.0);
        match privacy {
            Some(_) => Element::clickable(id.to_owned(), column).hover_foreground(look.text),
            None => column,
        }
    };
    let showing_privacy = privacy.as_ref().is_some_and(|(_, open)| *open);
    let mut tabs = vec![tab(
        details_tabs::CONNECTION,
        t("Conexión"),
        62.0,
        !showing_privacy,
    )];
    if privacy.is_some() {
        tabs.push(tab(
            details_tabs::PRIVACY,
            t("Privacidad"),
            70.0,
            showing_privacy,
        ));
    }
    let mut body = vec![
        Element::row(tabs).gap(16.0),
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.border),
    ];
    if let Some((privacy, true)) = privacy {
        body.push(privacy);
        return Element::column(body).gap(12.0).padding(16.0);
    }
    let rtts: Vec<u32> = view.samples.iter().map(|(_, rtt)| *rtt).collect();
    if rtts.len() >= 2 {
        body.push(
            LineChart::new(
                "vz:details:chart",
                view.samples.iter().map(|(when, _)| when.clone()),
                vec![ChartSeries::new(
                    t("Latencia (ms)"),
                    rtts.iter().map(|rtt| f64::from(*rtt)),
                )],
                look.menu_palette(),
            )
            .height(90.0)
            .color(look.accent)
            .smooth(false)
            .render(),
        );
    }
    let stat = |label: &str, value: String| {
        Element::row(vec![
            line(format!("{label}:"), 13.0, look.muted).shrink_to_fit(),
            strong_line(value, 13.0, look.text).shrink_to_fit(),
        ])
        .gap(4.0)
    };
    match (rtts.last(), rtts.is_empty()) {
        (Some(last), false) => {
            let average = rtts.iter().map(|rtt| f64::from(*rtt)).sum::<f64>() / rtts.len() as f64;
            body.push(
                Element::column(vec![
                    stat(
                        t("Latencia promedio"),
                        format!("{} ms", average.round() as u32),
                    ),
                    stat(t("Última latencia"), format!("{last} ms")),
                    stat(
                        t("Pérdida de paquetes"),
                        view.loss_permille.map_or_else(|| "—".into(), loss),
                    ),
                ])
                .gap(4.0),
            );
        }
        _ => body.push(text(
            t("Todavía no hay medidas de esta conexión."),
            13.0,
            look.muted,
        )),
    }
    body.push(text(
        t(
            "Una latencia alta retrasa la voz y una pérdida alta la entrecorta. Si \
         sigue así, desconectate y volvé a entrar.",
        ),
        12.0,
        look.faint,
    ));
    let note = if view.simulated {
        t("Valores simulados: esta llamada no transmite audio.")
    } else {
        t("Medido por el proveedor.")
    };
    body.push(
        Element::panel(
            Element::row(vec![
                icon(Icon::InfoCircle, look.muted).centered_y(),
                text(note, 12.0, look.muted).flex(1.0).centered_y(),
            ])
            .gap(6.0)
            .padding(8.0),
        )
        .radius(6.0)
        .background(look.raised),
    );
    Element::column(body).gap(12.0).padding(16.0)
}

/// Someone in a voice channel.
#[derive(Clone, Debug, PartialEq)]
pub struct Participant {
    pub user: u64,
    pub name: String,
    pub accent: Color,
    pub muted: bool,
    pub deafened: bool,
    pub speaking: bool,
    /// The server keeps their microphone off, whatever they chose.
    pub server_muted: bool,
    /// The server keeps their sound off.
    pub server_deafened: bool,
    /// The account silenced them for itself.
    pub silenced: bool,
    /// Sharing their screen in the channel.
    pub streaming: bool,
    /// The size of their camera's frame on their surface, while it shows:
    /// their tile shows it instead of their mark.
    pub camera: Option<(u32, u32)>,
    /// The latest picture of their stream, under their row in the column.
    pub thumbnail: Option<Thumbnail>,
    /// Their picture where they are seen, once loaded; their initial until
    /// then (2026-10-17: rows and tiles drew only the initial).
    pub image: Option<limn::RasterImage>,
    /// The frame over their face on their tile, once loaded, as the
    /// official client's call tiles draw it (2026-10-18).
    pub decoration: Option<limn::RasterImage>,
    /// Their row can be dragged to another voice channel: the account may
    /// move them.
    pub movable: bool,
    // Rows in the column (voz-filas-y-arrastre).
    /// They are being carried to another voice channel: their row is lifted
    /// on a card's surface, which the copy following the pointer keeps.
    pub carried: bool,
}

/// The latest picture of someone's stream, as the column shows it.
#[derive(Clone, Debug, PartialEq)]
pub enum Thumbnail {
    Image(limn::RasterImage),
    /// The simulation's: no picture leaves a simulated stream.
    Simulated,
}

/// A stream's picture: 160 by 90 under its owner's row, rounded 6.
const THUMBNAIL: (f32, f32) = (160.0, 90.0);

/// `thumbnail` of `person`'s stream, in line with their name; a click
/// watches it.
fn thumbnail(person: &Participant, thumbnail: &Thumbnail, look: &Look) -> Element {
    let (width, height) = THUMBNAIL;
    let picture = match thumbnail {
        Thumbnail::Image(image) => {
            Element::raster_image(image.clone().cover()).size(Some(width), Some(height))
        }
        Thumbnail::Simulated => centered(text(t("Vista previa simulada"), 12.0, look.muted)),
    };
    let picture = Element::panel(picture)
        .size(Some(width), Some(height))
        .radius(6.0)
        .background(look.raised)
        .clip_children();
    let picture = Element::clickable(ids::preview(person.user), picture)
        .radius(6.0)
        .accessibility_label(tr!("Ver la transmisión de {name}", name = person.name));
    Element::row(vec![
        gap(ROW_INDENT + 4.5 + ROW_AVATAR + 9.5, 0.0),
        picture,
        Element::spacer().flex(1.0),
    ])
    .padding(2.0)
}

/// The card of someone live beside their row in the column, while the
/// pointer is on it: who, what they stream, the latest picture of it, and
/// «Ver transmisión», as the official client's preview of a stream.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveCard {
    pub user: u64,
    pub name: String,
    /// What they stream: the game they play, when known.
    pub activity: Option<String>,
    /// The latest picture of their stream; what is said instead until one
    /// arrives, or when none can.
    pub picture: Result<Thumbnail, String>,
    /// Whether «Ver transmisión» can be used, or why not.
    pub watch: Result<(), String>,
}

/// The card of someone live: `card.picture` 288 by 162 under their name and
/// what they stream, «Ver transmisión» under it. Transparent 8 on its left,
/// toward the row, so the pointer crosses into it.
pub fn live_card(card: &LiveCard, look: &Look) -> Element {
    let (width, height) = LIVE_PICTURE;
    let mut head = vec![
        strong_line(&card.name, 15.0, look.text)
            .flex(1.0)
            .centered_y(),
        live(16.0, look).centered_y(),
    ];
    head.insert(1, gap(8.0, 0.0));
    let mut body = vec![Element::row(head)];
    if let Some(activity) = &card.activity {
        body.push(line(
            tr!("Transmitiendo {name}", name = activity),
            13.0,
            look.muted,
        ));
    }
    let picture = match &card.picture {
        Ok(Thumbnail::Image(image)) => {
            Element::raster_image(image.clone().cover()).size(Some(width), Some(height))
        }
        Ok(Thumbnail::Simulated) => centered(text(t("Vista previa simulada"), 13.0, look.muted)),
        Err(note) => centered(text(note.clone(), 13.0, look.muted)),
    };
    body.push(
        Element::panel(picture)
            .size(Some(width), Some(height))
            .radius(8.0)
            .background(look.canvas)
            .clip_children(),
    );
    let button = Button::new(
        ids::live_watch(card.user),
        t("Ver transmisión"),
        look.palette(),
    )
    .variant(ButtonVariant::Success)
    .icon(Icon::Screen)
    .width(width);
    body.push(match &card.watch {
        Ok(()) => button.render(),
        Err(reason) => button
            .tooltip_with(denied_hint(t("Ver transmisión"), reason))
            .disabled()
            .render(),
    });
    let surface = Element::panel(Element::column(body).gap(10.0).padding(12.0))
        .size(Some(LIVE_CARD_WIDTH), None)
        .background(look.menu)
        .border(look.border, 1.0)
        .radius(12.0)
        .shadow(look.shadow, limn::Point::new(0.0, 9.0), 22.0, 0.0)
        .accessibility_label(tr!("Transmisión de {name}", name = card.name));
    Element::row(vec![gap(8.0, 0.0), surface]).size(Some(LIVE_CARD_WIDTH + 8.0), None)
}

/// The «En vivo» mark of someone streaming, `size` high.
fn live(size: f32, look: &Look) -> Element {
    Element::panel(Element::row(vec![
        gap(6.0, 0.0),
        Element::text(t("EN VIVO"))
            .font_family(FontFamily::SansBold)
            .font_size(size * 0.6)
            .foreground(Color::rgb(255, 255, 255))
            .centered_y(),
        gap(6.0, 0.0),
    ]))
    .size(None, Some(size))
    .radius(4.0)
    .background(look.danger)
    .shrink_to_fit()
}

/// `face` ringed in green while its person speaks, the same size either way.
fn speaking_ring(face: Element, size: f32, speaking: bool, look: &Look) -> Element {
    if !speaking {
        return face;
    }
    Element::stack(vec![
        face,
        Element::panel(Element::spacer())
            .size(Some(size), Some(size))
            .radius(size / 2.0)
            .border(look.online, 2.0),
    ])
    .size(Some(size), Some(size))
}

/// The marks of a microphone off and of sound off, each with its hint.
fn marks(person: &Participant, prefix: &str, ink: Color, look: &Look) -> Vec<Element> {
    let mut marks = Vec::new();
    let mut mark = |glyph: Icon, suffix: &str, hint: &str, ink: Color| {
        let id = format!("{prefix}:{suffix}");
        marks.push(
            Tooltip::new(
                icon(glyph, ink).size(Some(16.0), Some(16.0)),
                id,
                hint,
                look.menu_palette(),
            )
            .render(),
        );
    };
    // The server's marks are red: their own unmute does not lift them.
    if person.server_muted {
        mark(
            Icon::MicrophoneMuted,
            "muted",
            t("Silenciado por el servidor"),
            look.danger,
        );
    } else if person.muted {
        mark(Icon::MicrophoneMuted, "muted", t("Silenciado"), ink);
    }
    if person.server_deafened {
        mark(
            Icon::Custom(crate::HEADPHONES_OFF),
            "deafened",
            t("Ensordecido por el servidor"),
            look.danger,
        );
    } else if person.deafened {
        mark(
            Icon::Custom(crate::HEADPHONES_OFF),
            "deafened",
            t("Ensordecido"),
            ink,
        );
    }
    if person.silenced {
        mark(Icon::VolumeMuted, "silenced", t("Silenciado para vos"), ink);
    }
    marks
}

/// Someone under the voice channel they are in: their mark (ringed while
/// they speak), their name, and whether they hear and can be heard. A click
/// opens their card (`card`, floating beside the row while open); a
/// secondary click, their menu. Someone live shows `streamed` beside the
/// row while the pointer is on it.
pub fn participant(
    person: &Participant,
    card: Option<Element>,
    streamed: Option<&LiveCard>,
    look: &Look,
) -> Element {
    let id = ids::row(person.user);
    let face = speaking_ring(
        crate::avatar_with_image(
            &person.name,
            person.accent,
            ROW_AVATAR,
            person.image.clone(),
        ),
        ROW_AVATAR,
        person.speaking,
        look,
    );
    let name_ink = if person.speaking {
        look.text
    } else {
        look.muted
    };
    let mut parts = vec![
        face.centered_y(),
        line(&person.name, 14.0, name_ink).flex(1.0).centered_y(),
    ];
    if person.streaming {
        parts.push(live(16.0, look).centered_y());
    }
    parts.extend(
        marks(person, &id, look.muted, look)
            .into_iter()
            .map(Element::centered_y),
    );
    let mut state = Vec::new();
    if person.streaming {
        state.push(t("en vivo"));
    }
    if person.speaking {
        state.push(t("hablando"));
    }
    if person.server_muted {
        state.push(t("silenciado por el servidor"));
    } else if person.muted {
        state.push(t("silenciado"));
    }
    if person.server_deafened {
        state.push(t("ensordecido por el servidor"));
    } else if person.deafened {
        state.push(t("ensordecido"));
    }
    if person.silenced {
        state.push(t("silenciado para vos"));
    }
    let label = if state.is_empty() {
        person.name.clone()
    } else {
        format!("{}, {}", person.name, state.join(", "))
    };
    // The highlight is drawn inside the row, short of its edges; the row
    // itself reaches the column's edge, so the pointer goes from it to the
    // card of someone live without leaving either.
    let fill = Element::panel(Element::spacer()).radius(ROW_RADIUS);
    let fill = if person.carried {
        fill.background(look.menu).border(look.border, 1.0)
    } else {
        fill.background(look.hover)
            .reveal_on_hover(format!("{id}:hover"))
    };
    let highlight = Element::column(vec![gap(0.0, 1.0), fill.flex(1.0), gap(0.0, 1.0)]);
    let body = Element::stack(vec![highlight, Element::row(parts).gap(9.5).padding(4.5)])
        .size(None, Some(ROW_HEIGHT));
    let row = Element::clickable(
        id,
        Element::row(vec![body.flex(1.0), gap(crate::sidebar::INSET, 0.0)]),
    )
    .hover_region()
    .hover_foreground(look.text)
    .accessibility_label(label);
    let row = if person.movable { row.draggable() } else { row };
    let row = match (card, streamed) {
        (Some(card), _) => crate::profile::popout(
            row,
            Some(card),
            ids::CARD_DISMISS,
            PopoverPlacement::RightStart,
            look,
        ),
        // Shown while the pointer is on the row or on the card, as the
        // official client's preview of a stream.
        (None, Some(live)) => Element::popover(
            row,
            live_card(live, look),
            ids::LIVE_DISMISS,
            PopoverPlacement::RightStart,
        )
        .popover_on_hover(),
        (None, None) => row,
    };
    let row = Element::row(vec![gap(ROW_INDENT, 0.0), row.flex(1.0)]);
    match &person.thumbnail {
        Some(shown) => Element::column(vec![row, thumbnail(person, shown, look)]),
        None => row,
    }
}

/// How tiles share an area: how many across, and each one's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub columns: usize,
    pub tile: Size,
}

/// The arrangement of `count` tiles of 16:9 in `area` that makes them
/// largest, `TILE_GAP` apart.
pub fn grid(count: usize, area: Size) -> Grid {
    let count = count.max(1);
    let mut best = Grid {
        columns: 1,
        tile: Size::new(0.0, 0.0),
    };
    for columns in 1..=count {
        let rows = count.div_ceil(columns);
        let across = (area.width - TILE_GAP * (columns - 1) as f32) / columns as f32;
        let down = (area.height - TILE_GAP * (rows - 1) as f32) / rows as f32;
        let width = across.min(down * 16.0 / 9.0).max(0.0);
        if width > best.tile.width {
            best = Grid {
                columns,
                tile: Size::new(width, width * 9.0 / 16.0),
            };
        }
    }
    best
}

/// A person on the stage: their mark on their color, or their camera
/// («Cámara simulada» over it when `simulated`), their name and marks in a
/// chip at the bottom left, outlined in green while they speak. Under the
/// pointer a «⋯» opens their menu (`menu`, while open); their card floats
/// beside the chip while open.
fn tile(
    person: &Participant,
    size: Size,
    card: Option<Element>,
    menu: Option<Vec<MenuEntry>>,
    watch: Option<&Result<(), String>>,
    simulated: bool,
    look: &Look,
) -> Element {
    let face = (size.height * 90.0 / 307.0).clamp(40.0, 120.0);
    let ground = Color::rgb(
        (person.accent.r as f32 * 0.35) as u8,
        (person.accent.g as f32 * 0.35) as u8,
        (person.accent.b as f32 * 0.35) as u8,
    );
    let mut chip: Vec<Element> = marks(person, &ids::tile(person.user), look.text, look)
        .into_iter()
        .map(Element::centered_y)
        .collect();
    chip.push(
        strong_line(&person.name, 14.0, look.text)
            .centered_y()
            .shrink_to_fit(),
    );
    let chip = Element::panel(Element::row(chip).gap(4.0).padding(8.0))
        .size(None, Some(34.0))
        .radius(8.0)
        .background(Look::alpha(look.rail, 200));
    let chip = crate::profile::popout(
        chip,
        card,
        ids::CARD_DISMISS,
        PopoverPlacement::RightStart,
        look,
    );
    let open = menu.is_some();
    let more = IconButton::new(
        ids::tile_more(person.user),
        Icon::More,
        filled(Look::alpha(look.rail, 200), look.raised, look),
    )
    .variant(ButtonVariant::Raised)
    .sizing(ButtonSizing {
        height: 34.0,
        font_size: 14.0,
        icon_size: 20.0,
        horizontal_padding: 8.0,
        inner_spacing: 6.0,
    })
    .width(32.0)
    .radius(RadiusRole::Card)
    .ink(look.text)
    .tooltip(t("Más"))
    .render();
    let more = DropdownMenu::new(
        more,
        menu_entries(menu.unwrap_or_default(), look),
        open,
        ids::MENU_DISMISS,
        look.menu_palette(),
    )
    .context_menu_style()
    .width(MENU_WIDTH)
    .placement(PopoverPlacement::RightStart)
    .render();
    // Kept in sight while its menu is open.
    let more = if open {
        more
    } else {
        more.reveal_on_hover(format!("{}:more", ids::tile(person.user)))
    };
    // Someone streaming: the mark, and the way to watch unless watched.
    let mut top = Vec::new();
    if person.camera.is_some() && simulated {
        top.push(
            Element::panel(line(t("Cámara simulada"), 13.0, look.text).padding(8.0))
                .size(None, Some(34.0))
                .radius(8.0)
                .background(Look::alpha(look.rail, 200))
                .shrink_to_fit(),
        );
    }
    if person.streaming {
        top.push(live(22.0, look));
        // Streams (transmisiones): the simulation has no picture to show.
        if person.thumbnail == Some(Thumbnail::Simulated) {
            top.push(
                Element::panel(line(t("Vista previa simulada"), 13.0, look.text).padding(6.0))
                    .size(None, Some(22.0))
                    .radius(4.0)
                    .background(Look::alpha(look.rail, 200))
                    .shrink_to_fit(),
            );
        }
        top.push(Element::spacer().flex(1.0));
        if let Some(permit) = watch {
            let button = Button::new(
                ids::watch(person.user),
                t("Ver transmisión"),
                look.palette(),
            )
            .variant(ButtonVariant::Secondary)
            .icon(Icon::Screen);
            top.push(match permit {
                Ok(()) => button.render(),
                Err(reason) => button
                    .tooltip_with(denied_hint(t("Ver transmisión"), reason))
                    .disabled()
                    .render(),
            });
        }
    }
    // Streams (transmisiones): the latest picture of their stream, dimmed so
    // what is over it reads, as the official client shows it.
    let streamed = match &person.thumbnail {
        Some(Thumbnail::Image(image)) if person.streaming => Some(image),
        _ => None,
    };
    let picture = match person.camera {
        _ if streamed.is_some() => Element::panel(Element::stack(vec![
            Element::raster_image(streamed.cloned().expect("matched").cover())
                .size(Some(size.width), Some(size.height)),
            Element::panel(Element::spacer())
                .size(Some(size.width), Some(size.height))
                .background(Look::alpha(Color::rgb(0, 0, 0), 96)),
        ]))
        .size(Some(size.width), Some(size.height))
        .radius(TILE_RADIUS)
        .background(Color::rgb(0, 0, 0))
        .clip_children(),
        Some((width, height)) => {
            let scale = (size.width / width as f32).min(size.height / height as f32);
            Element::panel(centered(
                Element::surface(ids::camera_surface(person.user), person.name.clone())
                    .size(Some(width as f32 * scale), Some(height as f32 * scale)),
            ))
            .size(Some(size.width), Some(size.height))
            .radius(TILE_RADIUS)
            .background(Color::rgb(0, 0, 0))
            .clip_children()
        }
        None => Element::panel(centered(crate::identity::decorated(
            |side| {
                crate::avatar_with_image(&person.name, person.accent, side, person.image.clone())
            },
            face,
            person.decoration.clone(),
        )))
        .size(Some(size.width), Some(size.height))
        .radius(TILE_RADIUS)
        .background(ground)
        .bottom_background_fade(Look::alpha(person.accent, 110), size.height),
    };
    let mut layers = vec![
        picture,
        Element::column(vec![
            Element::row(top).gap(8.0),
            Element::spacer().flex(1.0),
            // A long name is cut short: the «⋯» keeps its place in the tile.
            Element::row(vec![
                chip.shrink_to_fit(),
                Element::spacer().flex(1.0),
                more,
            ]),
        ])
        .padding(12.5),
    ];
    if person.speaking {
        layers.push(
            Element::panel(Element::spacer())
                .size(Some(size.width), Some(size.height))
                .radius(TILE_RADIUS)
                .border(look.online, 2.0),
        );
    }
    Element::clickable(ids::tile(person.user), Element::stack(layers))
        .size(Some(size.width), Some(size.height))
        .radius(TILE_RADIUS)
        .hover_region()
        .accessibility_label(person.name.clone())
}

/// The call of a voice channel where the conversation goes.
#[derive(Clone, Debug)]
pub struct Stage {
    pub channel: String,
    /// How far the account's call here got; `None` while it is not here.
    pub phase: Option<CallPhase>,
    /// Who is in the channel, the account first.
    pub people: Vec<Participant>,
    /// The account's microphone and sound.
    pub muted: bool,
    pub deafened: bool,
    pub simulated: bool,
    /// Why the account cannot join, if it cannot.
    pub join_denied: Option<String>,
    /// Whether the channel's chat shows beside the stage; or why it cannot.
    pub chat: Result<bool, String>,
    /// The room it has: the width of the conversation, the window's height.
    pub size: Size,
    /// A person's card, open from their tile.
    pub card: Option<(u64, Element)>,
    /// A person's menu, open from the «⋯» of their tile.
    pub tile_menu: Option<(u64, Vec<MenuEntry>)>,
    /// «Más opciones» of the bar is open.
    pub more_open: bool,
    pub video: VideoControls,
    /// The account's screen and camera, before the people.
    pub previews: Vec<Preview>,
    /// Whether the account can watch the streams here, or why not.
    pub watch: Result<(), String>,
    /// The streams watched: they take the stage, side by side.
    pub watching: Vec<Preview>,
    // Private calls.
    /// A direct or group conversation's call: what it adds. `None` for a
    /// voice channel.
    pub private: Option<crate::private_call::PrivateStage>,
    /// The quick voice panel of the bar's sound (`true`) or microphone,
    /// while open: [`crate::quick_voice::panel`].
    pub quick: Option<(bool, Element)>,
    // Calls and the voice panel (llamadas-y-panel-de-voz).
    /// The camera's panel over the bar's camera arrow, while open.
    pub camera_panel: Option<Element>,
    /// Room kept free at the header's end for the window's own buttons
    /// (barra-e-icono).
    pub end: f32,
}

/// The stage of a voice channel: its header, and its tiles over the bar of
/// controls while the account is in the call; else who is there and a way
/// to join.
pub fn stage(view: Stage, look: &Look) -> Element {
    let note = match &view.phase {
        Some(CallPhase::Joining) => Some((t("Conectando…").to_owned(), look.idle)),
        Some(CallPhase::Securing) => Some((t("Asegurando el audio…").to_owned(), look.idle)),
        Some(CallPhase::Reconnecting { attempt }) => {
            Some((tr!("Reconectando (intento {attempt})…", attempt), look.idle))
        }
        Some(CallPhase::Connected) if view.simulated => Some((
            t("Llamada simulada: no se transmite audio").to_owned(),
            look.faint,
        )),
        Some(CallPhase::Waiting) if view.simulated => {
            Some((t("Nadie más en la llamada simulada").to_owned(), look.faint))
        }
        Some(CallPhase::Waiting) => Some((t("Nadie más en la llamada").to_owned(), look.faint)),
        _ => None,
    };
    // `centered_y` wraps its child, so the row's shrinking goes on the wrapper.
    // Both give up width down to a floor: the note never shrinks below what
    // still reads «Llamada simulada…», so a simulated call always says so.
    let private = view.private.as_ref();
    // A conversation's call says how it goes where a channel says its name:
    // the conversation's header is right over it.
    let title = private
        .and_then(|private| private.status.clone())
        .unwrap_or_else(|| view.channel.clone());
    let mut header = vec![
        icon(
            if private.is_some() {
                Icon::Phone
            } else {
                Icon::Volume
            },
            look.faint,
        )
        .size(Some(18.0), Some(18.0))
        .centered_y(),
        strong_line(title, 16.0, look.text)
            .centered_y()
            .min_size(80.0, 0.0)
            .shrink_to_fit(),
        Element::spacer().flex(1.0),
    ];
    if let Some((note, ink)) = note {
        header.push(
            line(note, 13.0, ink)
                .centered_y()
                .min_size(124.0, 0.0)
                .shrink_to_fit(),
        );
    }
    if let Some(permit) = private.and_then(|private| private.ring_again.as_ref()) {
        header.push(crate::private_call::ring_again(permit, look).centered_y());
    }
    let chat =
        IconButton::new(ids::CHAT, Icon::ChatFill, look.palette()).variant(ButtonVariant::Ghost);
    let chat = match &view.chat {
        Ok(open) => {
            let hint = if *open {
                t("Ocultar chat")
            } else {
                t("Mostrar chat")
            };
            chat.active(*open)
                .tooltip_with(TooltipSpec::new(hint).placement(TooltipPlacement::Below))
        }
        Err(reason) => chat
            .tooltip_with(denied_hint(t("Mostrar chat"), reason).placement(TooltipPlacement::Below))
            .disabled(),
    };
    // A conversation's call has its chat under it already.
    if private.is_none() {
        header.push(chat.render().centered_y());
    }
    // The window's own buttons (barra-e-icono) end where the row's padding does.
    if view.end > 0.0 {
        header.push(Element::spacer().size(Some(view.end - 12.0), None));
    }
    let header = Element::panel(Element::row(header).gap(10.0).padding(12.0))
        .size(None, Some(crate::sidebar::HEADER))
        .background(look.canvas);
    let rule = Element::panel(Element::spacer())
        .size(None, Some(1.0))
        .background(look.border);
    let in_call = matches!(
        view.phase,
        Some(
            CallPhase::Joining
                | CallPhase::Securing
                | CallPhase::Connected
                | CallPhase::Waiting
                | CallPhase::Reconnecting { .. }
        )
    );
    let body = if in_call {
        call(&view, look)
    } else {
        invitation(&view, look)
    };
    Element::column(vec![header, rule, body.flex(1.0)]).background(look.canvas)
}

/// The tiles, as large as the room above the bar lets them be, and the bar.
fn call(view: &Stage, look: &Look) -> Element {
    let below = BAR_BOTTOM + BAR_HEIGHT + BAR_GAP;
    let area = Size::new(
        view.size.width - 2.0 * STAGE_INSET,
        view.size.height - crate::sidebar::HEADER - 1.0 - STAGE_INSET - below,
    );
    // The streams watched take the stage, with those streaming and not
    // watched, to watch them too; the people stay in the column.
    let (previews, people): (&[Preview], Vec<&Participant>) = if view.watching.is_empty() {
        (&view.previews, view.people.iter().collect())
    } else {
        let watched = |person: &Participant| {
            let surface = ids::stream_surface(person.user);
            view.watching.iter().any(|shown| shown.surface == surface)
        };
        let streaming = view.people.iter();
        let streaming = streaming.filter(|person| person.streaming && !watched(person));
        (&view.watching, streaming.collect())
    };
    // Private calls: whom it rings, after who is in it.
    let ringing = view
        .private
        .as_ref()
        .map_or(&[][..], |private| &private.ringing[..]);
    let person = |person: &Participant, size: Size, watch: bool| {
        let card = view
            .card
            .as_ref()
            .filter(|(user, _)| *user == person.user)
            .map(|(_, card)| card.clone());
        let menu = view
            .tile_menu
            .as_ref()
            .filter(|(user, _)| *user == person.user)
            .map(|(_, menu)| menu.clone());
        let watch = watch.then_some(&view.watch);
        tile(person, size, card, menu, watch, view.simulated, look)
    };
    // Streams (transmisiones): a stream focused takes the stage, the rest
    // in a strip under it, as in the official client and Serein.
    let tiles = match previews.iter().position(|shown| shown.focused) {
        Some(index) => {
            let count = previews.len() - 1 + people.len() + ringing.len();
            let strip = if count > 0 && area.height >= MIN_STRIP + TILE_GAP + MIN_FOCUS {
                (area.height * 0.18).clamp(MIN_STRIP, MAX_STRIP)
            } else {
                0.0
            };
            let main = Size::new(area.width, area.height - strip - TILE_GAP.min(strip));
            let fits = ((area.width + TILE_GAP) / (strip * 16.0 / 9.0 + TILE_GAP)).floor() as usize;
            let small = Size::new(strip * 16.0 / 9.0, strip);
            let mut rest: Vec<Element> = previews
                .iter()
                .enumerate()
                .filter(|(at, _)| *at != index)
                .map(|(_, shown)| preview_tile(shown, small, true, look))
                .collect();
            rest.extend(people.iter().map(|shown| person(shown, small, false)));
            rest.extend(
                ringing
                    .iter()
                    .map(|shown| crate::private_call::ringing_tile(shown, small, look)),
            );
            rest.truncate(fits.max(1));
            let mut column = vec![
                gap(0.0, STAGE_INSET),
                Element::row(vec![
                    gap(STAGE_INSET, 0.0),
                    preview(&previews[index], main, look),
                    gap(STAGE_INSET, 0.0),
                ]),
            ];
            if strip > 0.0 {
                column.push(gap(0.0, TILE_GAP));
                column.push(Element::row(vec![
                    Element::spacer().flex(1.0),
                    Element::row(rest).gap(TILE_GAP),
                    Element::spacer().flex(1.0),
                ]));
            }
            column.push(Element::spacer().flex(1.0));
            column.push(gap(0.0, below));
            Element::column(column)
        }
        None => {
            let layout = grid(previews.len() + people.len() + ringing.len(), area);
            let mut tiles: Vec<Element> = previews
                .iter()
                .map(|shown| preview(shown, layout.tile, look))
                .collect();
            tiles.extend(people.iter().map(|shown| person(shown, layout.tile, true)));
            tiles.extend(
                ringing
                    .iter()
                    .map(|shown| crate::private_call::ringing_tile(shown, layout.tile, look)),
            );
            let mut rows = Vec::new();
            let mut tiles = tiles.into_iter().peekable();
            while tiles.peek().is_some() {
                let row: Vec<Element> = tiles.by_ref().take(layout.columns).collect();
                rows.push(Element::row(vec![
                    Element::spacer().flex(1.0),
                    Element::row(row).gap(TILE_GAP),
                    Element::spacer().flex(1.0),
                ]));
            }
            Element::column(vec![
                gap(0.0, STAGE_INSET),
                Element::column(vec![
                    Element::spacer().flex(1.0),
                    Element::column(rows).gap(TILE_GAP),
                    Element::spacer().flex(1.0),
                ])
                .flex(1.0),
                gap(0.0, below),
            ])
        }
    };
    Element::stack(vec![
        tiles,
        Element::column(vec![
            Element::spacer().flex(1.0),
            bar(view, look),
            gap(0.0, BAR_BOTTOM),
        ]),
    ])
}

/// The bar under the tiles: the microphone, the sound and the camera; the
/// screen and «Más opciones»; hanging up.
fn bar(view: &Stage, look: &Look) -> Element {
    let sizing = ButtonSizing {
        height: BAR_BUTTON,
        font_size: 14.0,
        icon_size: 20.0,
        horizontal_padding: 8.0,
        inner_spacing: 6.0,
    };
    let toggle = |id: &str, on: Icon, off: Icon, off_now: bool, hint: (&str, &str)| {
        let (fill, ink) = if off_now {
            (Look::alpha(look.danger, 34), look.danger)
        } else {
            (look.menu, look.text)
        };
        IconButton::new(
            id,
            if off_now { off } else { on },
            filled(fill, lighter(fill, 16), look),
        )
        .variant(ButtonVariant::Raised)
        .sizing(sizing)
        .radius(RadiusRole::Card)
        .ink(ink)
        .hover_ink(ink)
        .tooltip(if off_now { hint.1 } else { hint.0 })
        .render()
    };
    let [camera, screen] = video_buttons(&view.video, BAR_BUTTON, look.menu, look);
    let screen = match view.video.picker {
        Some(chosen) => quality_picker(screen, chosen, &view.video, look),
        None => screen,
    };
    let group = |buttons: Vec<Element>| {
        Element::panel(Element::row(buttons).gap(8.0).padding(4.0))
            .size(None, Some(BAR_HEIGHT))
            .radius(12.0)
            .background(look.menu)
            .border(look.border, 1.0)
    };
    let microphone = toggle(
        ids::MUTE,
        Icon::Microphone,
        Icon::MicrophoneMuted,
        view.muted,
        (t("Silenciar"), t("Desactivar silencio")),
    );
    let sound = toggle(
        ids::DEAFEN,
        Icon::Headphones,
        Icon::Custom(crate::HEADPHONES_OFF),
        view.deafened,
        (t("Ensordecer"), t("Desactivar ensordecimiento")),
    );
    // Each with the arrow of its quick panel, which opens over the bar.
    let options = |button: Element, id: &str, hint: &str, panel: Option<&Element>| {
        let open = panel.is_some();
        let arrow = Element::clickable(
            id,
            centered(
                Element::icon(if open {
                    Icon::ChevronUp
                } else {
                    Icon::ChevronDown
                })
                .foreground(look.muted)
                .size(Some(12.0), Some(12.0)),
            ),
        )
        .size(Some(crate::quick_voice::ARROW_WIDTH), Some(BAR_BUTTON))
        .radius(6.0)
        .hover_background(look.raised)
        .hover_foreground(look.text)
        .accessible(limn::Access::new(limn::Role::Button).name(hint));
        let arrow = Tooltip::new(arrow, id, hint, look.menu_palette()).render();
        let arrow = match panel {
            Some(panel) => Element::popover(
                arrow,
                panel.clone(),
                crate::quick_voice::ids::DISMISS,
                PopoverPlacement::TopStart,
            )
            .popover_gap(12.0),
            None => arrow,
        };
        Element::row(vec![button, arrow]).gap(1.0)
    };
    let quick = |output: bool| {
        view.quick
            .as_ref()
            .filter(|(it, _)| *it == output)
            .map(|(_, panel)| panel)
    };
    let microphone = options(
        microphone,
        ids::INPUT_OPTIONS,
        t("Opciones de entrada"),
        quick(false),
    );
    let sound = options(
        sound,
        ids::OUTPUT_OPTIONS,
        t("Opciones de salida"),
        quick(true),
    );
    let camera = options(
        camera,
        ids::CAMERA_OPTIONS,
        t("Opciones de cámara"),
        view.camera_panel.as_ref(),
    );
    let more = IconButton::new(ids::MORE, Icon::More, filled(look.menu, look.raised, look))
        .variant(ButtonVariant::Raised)
        .sizing(sizing)
        .radius(RadiusRole::Card)
        .ink(look.text)
        .active(view.more_open)
        .tooltip(t("Más opciones"))
        .render();
    let more = DropdownMenu::new(
        more,
        vec![DropdownMenuEntry::Item(
            DropdownMenuItem::new(ids::VOICE_SETTINGS, t("Ajustes de voz y video"))
                .icon(Icon::Settings),
        )],
        view.more_open,
        ids::MORE_DISMISS,
        look.menu_palette(),
    )
    .context_menu_style()
    .width(MORE_WIDTH)
    .placement(PopoverPlacement::TopEnd)
    .render();
    // Streams (transmisiones): with one focused, as in the official client,
    // hanging up gives way to letting it go, and its window and full screen
    // go at the bar's end.
    let focused = view
        .watching
        .iter()
        .filter(|shown| shown.focused)
        .find_map(|shown| shown.stream.as_ref());
    let (id, glyph, hint) = match focused {
        Some(stream) => (
            ids::stop_watching(stream.owner),
            Icon::Custom(SCREEN_STOP),
            t("Dejar de ver"),
        ),
        None => (ids::LEAVE.to_owned(), Icon::HangUp, t("Desconectar")),
    };
    let hang_up = IconButton::new(
        id,
        glyph,
        filled(look.danger, lighter(look.danger, 18), look),
    )
    .variant(ButtonVariant::Raised)
    .sizing(ButtonSizing {
        height: BAR_HEIGHT,
        ..sizing
    })
    .width(HANG_UP_WIDTH)
    .radius(RadiusRole::Panel)
    .ink(Color::rgb(255, 255, 255))
    .hover_ink(Color::rgb(255, 255, 255))
    .tooltip(hint)
    .render();
    let mut end = vec![Element::spacer().flex(1.0)];
    if let Some(stream) = focused {
        let button = |id: String, glyph: Icon, hint: &str| {
            IconButton::new(
                id,
                glyph,
                filled(Color::rgba(0, 0, 0, 0), look.raised, look),
            )
            .variant(ButtonVariant::Raised)
            .sizing(ButtonSizing {
                icon_size: 24.0,
                horizontal_padding: 6.0,
                ..sizing
            })
            .radius(RadiusRole::Card)
            .ink(look.muted)
            .hover_ink(look.text)
            .tooltip(hint)
            .render()
            .centered_y()
        };
        if !stream.windowed {
            end.push(button(
                ids::window(stream.owner),
                Icon::Custom(POP_OUT),
                t("Ventana aparte"),
            ));
        }
        end.push(button(
            ids::fullscreen(stream.owner),
            Icon::Fullscreen,
            t("Pantalla completa"),
        ));
    }
    // Centered in the stage whatever its ends hold, as the official grid of
    // three columns does.
    Element::row(vec![
        gap(BAR_BOTTOM, 0.0),
        Element::spacer().flex(1.0),
        Element::row(vec![
            group(vec![microphone, sound, camera]),
            group(vec![screen, more]),
            hang_up,
        ])
        .gap(BAR_GAP),
        Element::row(end).gap(20.0).flex(1.0),
        gap(BAR_BOTTOM, 0.0),
    ])
    .size(None, Some(BAR_HEIGHT))
}

/// The quality picker over `button`: the size and the rate, `chosen`
/// checked, and going live with them.
fn quality_picker(
    button: Element,
    chosen: StreamQuality,
    video: &VideoControls,
    look: &Look,
) -> Element {
    let item = |id: String, label: String, on: bool| {
        DropdownMenuEntry::Item(DropdownMenuItem::new(id, label).selected(on))
    };
    // What to share, first, where RUNE itself offers it (windows-medios).
    let mut entries = Vec::new();
    for (windows, heading) in [(false, t("Pantallas")), (true, t("Ventanas"))] {
        let offered: Vec<&ScreenChoice> = video
            .sources
            .iter()
            .filter(|source| source.window == windows)
            .collect();
        if !offered.is_empty() {
            entries.push(DropdownMenuEntry::Heading(heading.into()));
        }
        entries.extend(offered.into_iter().map(|source| {
            item(
                format!("{}{}", ids::SOURCE, source.id),
                source.name.clone(),
                source.chosen,
            )
        }));
    }
    if !entries.is_empty() {
        entries.push(DropdownMenuEntry::Heading(t("Calidad").into()));
    }
    entries.extend(StreamQuality::HEIGHTS.iter().map(|height| {
        item(
            format!("{}{height}", ids::QUALITY),
            format!("{height}p"),
            chosen.height == *height,
        )
    }));
    entries.push(DropdownMenuEntry::Separator);
    entries.extend(StreamQuality::RATES.iter().map(|fps| {
        item(
            format!("{}{fps}", ids::FPS),
            format!("{fps} fps"),
            chosen.fps == *fps,
        )
    }));
    entries.push(DropdownMenuEntry::Separator);
    entries.push(item(ids::SOUND.into(), t("Con sonido").into(), video.sound));
    if video.pick_again {
        entries.push(item(
            ids::PICK_SCREEN.into(),
            t("Elegir otra pantalla").into(),
            false,
        ));
    }
    entries.push(DropdownMenuEntry::Separator);
    entries.push(DropdownMenuEntry::Item(
        DropdownMenuItem::new(ids::SHARE, t("Compartir pantalla")).icon(Icon::Screen),
    ));
    DropdownMenu::new(
        button,
        entries,
        true,
        ids::SHARE_DISMISS,
        look.menu_palette(),
    )
    .context_menu_style()
    .width(MORE_WIDTH)
    .placement(PopoverPlacement::TopStart)
    .render()
}

/// The faces over a call the account is not in, at most.
const INVITATION_FACES: usize = 6;

/// While the account is not in the call: who is in it, the channel, why it
/// could not connect if it tried, and the way in.
fn invitation(view: &Stage, look: &Look) -> Element {
    // Names up to three, as for who is typing; past that, how many.
    let who = match view.people.as_slice() {
        [] => t("No hay nadie en el chat de voz en este momento").to_owned(),
        [one] => tr!("{one} está en el chat de voz", one = one.name),
        [one, two] => tr!(
            "{one} y {two} están en el chat de voz",
            one = one.name,
            two = two.name
        ),
        [one, two, three] => tr!(
            "{one}, {two} y {three} están en el chat de voz",
            one = one.name,
            two = two.name,
            three = three.name
        ),
        people => tr!(
            "{count} personas están en el chat de voz",
            count = people.len()
        ),
    };
    // Private calls: their own line, and what joining says.
    let invited = view
        .private
        .as_ref()
        .and_then(|private| private.invitation.clone());
    let who = invited.as_ref().map_or(who, |(line, _)| line.clone());
    let mut parts = Vec::new();
    // Who is in it, by their faces, ringed while they speak, as the
    // official client's call header shows them (2026-10-21: a call joined
    // from another device showed no one). Cameras need RUNE's own
    // connection: faces only, until the account joins from here.
    if !view.people.is_empty() {
        const FACE: f32 = 72.0;
        let faces = view
            .people
            .iter()
            .take(INVITATION_FACES)
            .map(|person| {
                speaking_ring(
                    crate::avatar_with_image(
                        &person.name,
                        person.accent,
                        FACE,
                        person.image.clone(),
                    ),
                    FACE,
                    person.speaking,
                    look,
                )
            })
            .collect();
        parts.push(centered(Element::row(faces).gap(16.0)).size(None, Some(FACE + 8.0)));
        parts.push(gap(0.0, 4.0));
    }
    parts.extend([
        centered(
            Element::text(view.channel.clone())
                .font_family(FontFamily::SansBold)
                .font_size(32.0)
                .foreground(look.text),
        )
        .size(None, Some(40.0)),
        // Long names are cut short within the column.
        centered(line(who, 14.0, look.muted).shrink_to_fit()).size(None, Some(20.0)),
    ]);
    if let Some(CallPhase::Failed { reason }) = &view.phase {
        parts.push(centered(text(reason.clone(), 13.0, look.danger)).size(None, Some(20.0)));
    }
    let palette = ComponentPalette {
        text: Color::rgb(255, 255, 255),
        surface: Color::rgb(0, 0, 0),
        ..look.palette()
    };
    let label = if matches!(view.phase, Some(CallPhase::Failed { .. })) {
        t("Reintentar")
    } else if let Some((_, label)) = &invited {
        label.as_str()
    } else {
        t("Unirse al chat de voz")
    };
    // Answering a call coming in is green, as its notice's.
    let incoming = view
        .private
        .as_ref()
        .is_some_and(|private| private.incoming);
    let join = Button::new(ids::JOIN, label, palette)
        .variant(if incoming {
            ButtonVariant::Success
        } else {
            ButtonVariant::Default
        })
        .width(184.0);
    let join = match &view.join_denied {
        Some(reason) => join
            .tooltip_with(TooltipSpec::new(reason.clone()))
            .disabled(),
        None => join,
    };
    parts.push(gap(0.0, 12.0));
    let join = if incoming {
        Element::row(vec![crate::private_call::decline(look), join.render()]).gap(10.0)
    } else {
        join.render()
    };
    parts.push(centered(join).size(None, Some(41.0)));
    Element::panel(Element::column(vec![
        Element::spacer().flex(1.0),
        Element::column(parts).gap(8.0),
        Element::spacer().flex(1.0),
    ]))
    .background(look.canvas)
    .bottom_background_fade(Look::alpha(look.accent, 70), view.size.height * 0.8)
}

/// An entry of a person's menu.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    /// The action it reports, its label, and whether it is checked.
    Item(String, String, Option<bool>),
    /// How loud `user` plays for the account, from 0 to `MAX_VOLUME`, and a
    /// note under it.
    Volume {
        user: u64,
        /// «Volumen del usuario», «Volumen de la transmisión».
        label: &'static str,
        volume: f32,
        note: Option<String>,
    },
    Separator,
    // Voice moderation (P4).
    /// An item that opens a list of entries beside it: its label, and those
    /// entries.
    Submenu(String, Vec<MenuEntry>),
    // A person's menu as the official client draws it (2026-10-19).
    /// An action that removes or punishes, such as kicking: its action and
    /// label, drawn in the theme's danger color.
    Danger(String, String),
    /// A role in the menu's roles: its action, its name, its color (`None`
    /// for one without), and whether the person has it, a check box at the
    /// end of the row.
    Role {
        id: String,
        name: String,
        color: Option<Rgb>,
        has: bool,
    },
}

/// The most a person's volume goes: 200 %.
pub const MAX_VOLUME: f32 = 2.0;

/// A person's menu, 297 wide.
pub const MENU_WIDTH: f32 = 297.0;

/// `entries` as limn's menu takes them.
pub fn menu_entries(entries: Vec<MenuEntry>, look: &Look) -> Vec<DropdownMenuEntry> {
    entries
        .into_iter()
        .map(|entry| match entry {
            MenuEntry::Separator => DropdownMenuEntry::Separator,
            MenuEntry::Volume {
                user,
                label,
                volume,
                note,
            } => {
                let mut parts = vec![
                    line(t(label), 14.0, look.text),
                    Slider::new(ids::volume(user), volume / MAX_VOLUME, look.palette())
                        .ink(look.accent)
                        .accessibility_label(t(label))
                        .render(),
                    line(
                        tr!("{pct} %", pct = format!("{:.0}", volume * 100.0)),
                        12.0,
                        look.muted,
                    ),
                ];
                if let Some(note) = note {
                    parts.push(line(note, 12.0, look.faint));
                }
                DropdownMenuEntry::Custom(Box::new(Element::column(parts).gap(6.0).padding(8.0)))
            }
            MenuEntry::Item(id, label, checked) => {
                let item = DropdownMenuItem::new(id, label);
                DropdownMenuEntry::Item(match checked {
                    Some(checked) => item.checkbox(checked),
                    None => item,
                })
            }
            MenuEntry::Submenu(label, choices) => {
                let id = format!("vz:submenu:{label}");
                DropdownMenuEntry::Item(
                    DropdownMenuItem::new(id, label).submenu(menu_entries(choices, look)),
                )
            }
            MenuEntry::Danger(id, label) => {
                DropdownMenuEntry::Item(DropdownMenuItem::new(id, label).destructive(look.danger))
            }
            MenuEntry::Role {
                id,
                name,
                color,
                has,
            } => {
                // A 12 px dot in the role's color, centered in the 16 px icon slot.
                let dot = Element::spacer()
                    .background(color.map_or(look.muted, |color| look.ink(color)))
                    .radius(6.0);
                DropdownMenuEntry::Item(
                    DropdownMenuItem::new(id, name)
                        .leading(Element::panel(dot).padding(2.0))
                        .checkbox(has),
                )
            }
        })
        .collect()
}

/// A person's menu opened by a secondary click, its corner at `at` in the
/// window. Laid over everything: put it last in a stack the window's size.
pub fn context_menu(at: (f32, f32), entries: Vec<MenuEntry>, look: &Look) -> Element {
    DropdownMenu::new(
        gap(1.0, 1.0),
        menu_entries(entries, look),
        true,
        ids::MENU_DISMISS,
        look.menu_palette(),
    )
    .context_menu_style()
    .width(MENU_WIDTH)
    .at(limn::Point::new(at.0.max(0.0), at.1.max(0.0)))
    .render()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::testing::{controls, frame, texts};

    fn person(user: u64, muted: bool, deafened: bool, speaking: bool) -> Participant {
        Participant {
            user,
            name: format!("Persona {user}"),
            accent: Color::rgb(90, 120, 200),
            muted,
            deafened,
            speaking,
            server_muted: false,
            server_deafened: false,
            silenced: false,
            streaming: false,
            camera: None,
            thumbnail: None,
            image: None,
            decoration: None,
            movable: false,
            carried: false,
        }
    }

    #[test]
    fn a_participant_shows_their_picture_once_it_loaded() {
        let picture = limn::RasterImage::new(
            "2".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "x",
        )
        .unwrap();
        let drawn = |person: &Participant| {
            let (_, laid) = frame(
                participant(person, None, None, &Look::DARK),
                Size::new(240.0, 40.0),
            );
            laid.commands
                .iter()
                .any(|command| matches!(command, limn::DrawCommand::RasterImage { .. }))
        };
        let initial = person(1, false, false, false);
        assert!(!drawn(&initial), "their initial while it loads");
        assert!(drawn(&Participant {
            image: Some(picture),
            ..initial
        }));
    }

    #[test]
    fn a_tile_draws_the_frame_over_their_face() {
        let frame_over = limn::RasterImage::new(
            "3".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "marco",
        )
        .unwrap();
        let drawn = |person: &Participant| {
            let (_, laid) = frame(
                tile(
                    person,
                    Size::new(320.0, 180.0),
                    None,
                    None,
                    None,
                    false,
                    &Look::DARK,
                ),
                Size::new(320.0, 180.0),
            );
            laid.commands
                .iter()
                .filter_map(|command| match command {
                    limn::DrawCommand::RasterImage { image, .. } => Some(image.alt().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let plain = person(1, false, false, false);
        assert!(drawn(&plain).is_empty(), "their initial alone");
        assert_eq!(
            drawn(&Participant {
                decoration: Some(frame_over),
                ..plain
            }),
            ["marco"]
        );
    }

    #[test]
    fn a_server_mute_is_marked_red_and_said_apart_from_their_own() {
        let look = Look::DARK;
        let red_mic = |person: &Participant| {
            let (_, laid) = frame(
                participant(person, None, None, &look),
                Size::new(240.0, 40.0),
            );
            laid.commands.iter().any(|command| {
                matches!(command, limn::DrawCommand::Icon { icon: Icon::MicrophoneMuted, tint, .. } if *tint == look.danger)
            })
        };
        let own = person(1, true, false, false);
        assert!(!red_mic(&own));
        let held = Participant {
            server_muted: true,
            ..person(1, false, false, false)
        };
        assert!(
            red_mic(&held),
            "unmuted by choice, still held by the server"
        );
    }

    fn connected() -> Panel {
        Panel {
            channel: "General".into(),
            server: Some("RUNE prueba".into()),
            phase: CallPhase::Connected,
            duration: Some(Duration::from_secs(190)),
            quality: Some(CallQuality {
                rtt_ms: 47,
                loss_permille: 4,
            }),
            simulated: true,
            video: VideoControls {
                denied: Some(NO_VIDEO.into()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn call_clocks_read_as_minutes_until_the_hour() {
        assert_eq!(clock(Duration::from_secs(207)), "03:27");
        assert_eq!(clock(Duration::from_secs(3_729)), "1:02:09");
        assert_eq!(on_for(Duration::from_secs(20)), "1 min");
        assert_eq!(on_for(Duration::from_secs(59 * 60 + 59)), "59 min");
        assert_eq!(on_for(Duration::from_secs(3_929)), "1 h 05 min");
        assert_eq!(loss(4), "0,4 %");
    }

    #[test]
    fn the_panel_is_112_high_and_says_where_and_how_the_call_is() {
        let look = Look::ONYX;
        let alone = Element::column(vec![
            panel(&connected(), None, &look),
            Element::spacer().flex(1.0),
        ]);
        let (_, laid) = frame(alone, Size::new(357.5, 300.0));
        let shown = texts(&laid);
        assert!(shown.contains(&"Voz conectada (simulada)".to_owned()));
        assert!(shown.contains(&"General / RUNE prueba".to_owned()));
        assert!(shown.contains(&"03:10".to_owned()), "{shown:?}");
        let ids = controls(&laid);
        assert!(ids.contains(&ids::DETAILS.to_owned()));
        assert!(ids.contains(&ids::LEAVE.to_owned()));
        // No video on this system: the camera and the screen are drawn, not usable.
        assert!(!ids.contains(&ids::CAMERA.to_owned()));
        assert!(!ids.contains(&ids::SCREEN.to_owned()));
        let local = Panel {
            video: VideoControls::default(),
            ..connected()
        };
        let (_, usable) = frame(panel(&local, None, &look), Size::new(357.5, 300.0));
        let usable = controls(&usable);
        assert!(
            usable.contains(&ids::CAMERA.to_owned()) && usable.contains(&ids::SCREEN.to_owned())
        );
        let box_height = laid
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::FillRect { rect, color, .. } if *color == look.rail => {
                    Some(rect.height)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(box_height, PANEL_HEIGHT);
    }

    #[test]
    fn a_call_alone_in_its_channel_says_it_waits_and_keeps_its_clock_and_latency() {
        let waiting = Panel {
            phase: CallPhase::Waiting,
            simulated: false,
            ..connected()
        };
        let (_, laid) = frame(panel(&waiting, None, &Look::DARK), Size::new(357.5, 300.0));
        let shown = texts(&laid);
        assert!(
            shown.contains(&"Esperando a los demás".to_owned()),
            "{shown:?}"
        );
        assert!(shown.contains(&"03:10".to_owned()), "{shown:?}");
        // The simulation waits too, and says it is one.
        let simulated = Panel {
            simulated: true,
            ..waiting
        };
        let (_, laid) = frame(
            panel(&simulated, None, &Look::DARK),
            Size::new(357.5, 300.0),
        );
        let shown = texts(&laid);
        assert!(
            shown.contains(&"Esperando a los demás (simulada)".to_owned()),
            "{shown:?}"
        );
        // So does the stage.
        let view = |simulated| Stage {
            channel: "General".into(),
            phase: Some(CallPhase::Waiting),
            people: vec![person(1, false, false, false)],
            muted: false,
            deafened: false,
            simulated,
            join_denied: None,
            chat: Err("Sin chat".into()),
            size: Size::new(980.0, 762.0),
            card: None,
            tile_menu: None,
            more_open: false,
            video: VideoControls::default(),
            previews: Vec::new(),
            watch: Ok(()),
            watching: Vec::new(),
            private: None,
            quick: None,
            camera_panel: None,
            end: 0.0,
        };
        for (simulated, said) in [
            (false, "Nadie más en la llamada"),
            (true, "Nadie más en la llamada simulada"),
        ] {
            let (_, laid) = frame(stage(view(simulated), &Look::ONYX), Size::new(980.0, 762.0));
            let shown = texts(&laid);
            assert!(shown.contains(&said.to_owned()), "{shown:?}");
        }
    }

    #[test]
    fn a_failed_call_says_why_and_offers_to_try_again() {
        let failed = Panel {
            phase: CallPhase::Failed {
                reason: "Se perdió la conexión".into(),
            },
            ..connected()
        };
        let (_, laid) = frame(panel(&failed, None, &Look::DARK), Size::new(357.5, 300.0));
        let shown = texts(&laid);
        assert!(shown.contains(&"No se pudo conectar la voz".to_owned()));
        assert!(shown.contains(&"Se perdió la conexión".to_owned()));
        assert!(controls(&laid).contains(&ids::RETRY.to_owned()));
    }

    #[test]
    fn the_details_average_the_latency_and_say_the_values_are_made_up() {
        let view = Details {
            samples: vec![
                ("10:00".into(), 40),
                ("10:01".into(), 47),
                ("10:02".into(), 46),
            ],
            loss_permille: Some(0),
            simulated: true,
        };
        let (_, laid) = frame(details(&view, &Look::DARK), Size::new(DETAILS_WIDTH, 600.0));
        let shown = texts(&laid);
        for expected in ["Conexión", "44 ms", "46 ms", "0,0 %"] {
            assert!(
                shown.iter().any(|text| text == expected),
                "{expected} in {shown:?}"
            );
        }
        assert!(shown.iter().any(|text| text.contains("Valores simulados")));
    }

    #[test]
    fn tiles_take_the_arrangement_that_makes_them_largest() {
        // Two in a tall room go one over the other, as captured: 546.5 wide.
        let two = grid(2, Size::new(946.0, 622.5));
        assert_eq!(two.columns, 1);
        assert!((two.tile.width - 546.5).abs() < 1.0, "{two:?}");
        let four = grid(4, Size::new(946.0, 622.5));
        assert_eq!(four.columns, 2);
        assert!((four.tile.height - four.tile.width * 9.0 / 16.0).abs() < 0.01);
        assert_eq!(grid(0, Size::new(100.0, 100.0)).columns, 1);
    }

    #[test]
    fn the_stage_shows_each_person_and_a_bar_with_the_video_controls() {
        let view = Stage {
            channel: "General".into(),
            phase: Some(CallPhase::Connected),
            people: vec![person(1, true, true, false), person(2, false, false, true)],
            muted: true,
            deafened: true,
            simulated: true,
            join_denied: None,
            chat: Err("Sin chat".into()),
            size: Size::new(980.0, 762.0),
            card: None,
            tile_menu: None,
            more_open: false,
            video: VideoControls::default(),
            previews: Vec::new(),
            watch: Ok(()),
            watching: Vec::new(),
            private: None,
            quick: None,
            camera_panel: None,
            end: 0.0,
        };
        // The camera's arrow opens its panel over the bar.
        let open = Stage {
            camera_panel: Some(Element::text("Panel de la cámara")),
            ..view.clone()
        };
        let (_, opened) = frame(stage(open, &Look::ONYX), Size::new(980.0, 762.0));
        assert!(texts(&opened).contains(&"Panel de la cámara".to_owned()));
        let (_, laid) = frame(stage(view, &Look::ONYX), Size::new(980.0, 762.0));
        let ids = controls(&laid);
        for id in [ids::MUTE, ids::DEAFEN, ids::MORE, ids::LEAVE] {
            assert!(ids.contains(&id.to_owned()), "{id} in {ids:?}");
        }
        for id in [ids::CAMERA, ids::SCREEN, ids::CAMERA_OPTIONS] {
            assert!(ids.contains(&id.to_owned()), "{id} in {ids:?}");
        }
        assert!(!texts(&laid).contains(&"Panel de la cámara".to_owned()));
        assert!(
            !ids.contains(&ids::CHAT.to_owned()),
            "the chat is not usable"
        );
        assert!(ids.contains(&ids::tile(1)) && ids.contains(&ids::tile(2)));
        let shown = texts(&laid);
        assert!(shown.contains(&"Persona 1".to_owned()));
        assert!(shown.iter().any(|text| text.contains("simulada")));
        // The one speaking is outlined in green.
        let outlined = laid.commands.iter().any(|command| {
            matches!(command, limn::DrawCommand::StrokeRect { color, .. } if *color == Look::ONYX.online)
        });
        assert!(outlined);
    }

    #[test]
    fn the_quality_picker_offers_each_size_and_rate_with_the_chosen_checked() {
        let view = Stage {
            channel: "General".into(),
            phase: Some(CallPhase::Connected),
            people: vec![person(1, false, false, false)],
            muted: false,
            deafened: false,
            simulated: false,
            join_denied: None,
            chat: Err("Sin chat".into()),
            size: Size::new(980.0, 762.0),
            card: None,
            tile_menu: None,
            more_open: false,
            video: VideoControls {
                picker: Some(StreamQuality {
                    height: 1080,
                    fps: 60,
                }),
                ..Default::default()
            },
            watch: Ok(()),
            watching: Vec::new(),
            previews: Vec::new(),
            private: None,
            quick: None,
            camera_panel: None,
            end: 0.0,
        };
        let (_, laid) = frame(stage(view, &Look::ONYX), Size::new(980.0, 762.0));
        let ids = controls(&laid);
        for id in [
            "vz:quality:480",
            "vz:quality:720",
            "vz:quality:1080",
            "vz:fps:15",
            "vz:fps:30",
            "vz:fps:60",
            ids::SHARE,
        ] {
            assert!(ids.contains(&id.to_owned()), "{id} in {ids:?}");
        }
        let shown = texts(&laid);
        for expected in ["480p", "1080p", "60 fps", "Compartir pantalla"] {
            assert!(
                shown.iter().any(|text| text == expected),
                "{expected} in {shown:?}"
            );
        }
    }

    #[test]
    fn rune_s_own_picker_offers_screens_then_windows_before_the_quality() {
        let source = |id: &str, name: &str, window, chosen| ScreenChoice {
            id: id.into(),
            name: name.into(),
            window,
            chosen,
        };
        let view = Stage {
            channel: "General".into(),
            phase: Some(CallPhase::Connected),
            people: vec![person(1, false, false, false)],
            muted: false,
            deafened: false,
            simulated: false,
            join_denied: None,
            chat: Err("Sin chat".into()),
            size: Size::new(980.0, 762.0),
            card: None,
            tile_menu: None,
            more_open: false,
            video: VideoControls {
                picker: Some(StreamQuality::default()),
                sources: vec![
                    source(
                        r"screen:\\.\DISPLAY1",
                        "Pantalla 1 (principal) · 1920×1080",
                        false,
                        true,
                    ),
                    source("window:42", "Un juego", true, false),
                ],
                ..Default::default()
            },
            watch: Ok(()),
            watching: Vec::new(),
            previews: Vec::new(),
            private: None,
            quick: None,
            camera_panel: None,
            end: 0.0,
        };
        let (_, laid) = frame(stage(view, &Look::ONYX), Size::new(980.0, 762.0));
        let ids = controls(&laid);
        for id in [
            r"vz:source:screen:\\.\DISPLAY1",
            "vz:source:window:42",
            ids::SHARE,
        ] {
            assert!(ids.contains(&id.to_owned()), "{id} in {ids:?}");
        }
        let shown = texts(&laid);
        let at = |text: &str| {
            shown
                .iter()
                .position(|shown| shown == text)
                .unwrap_or_else(|| panic!("{text} in {shown:?}"))
        };
        assert!(at("Pantallas") < at("Pantalla 1 (principal) · 1920×1080"));
        assert!(at("Ventanas") < at("Un juego"));
        assert!(at("Un juego") < at("Calidad") && at("Calidad") < at("720p"));
    }

    #[test]
    fn previews_come_first_say_they_stay_here_and_can_be_stopped() {
        let note = "Solo vos lo ves. La transmisión llega con las llamadas.";
        let view = Stage {
            channel: "General".into(),
            phase: Some(CallPhase::Connected),
            people: vec![person(1, false, false, false)],
            muted: false,
            deafened: false,
            simulated: true,
            join_denied: None,
            chat: Err("Sin chat".into()),
            size: Size::new(980.0, 762.0),
            card: None,
            tile_menu: None,
            more_open: false,
            video: VideoControls {
                screen: true,
                camera: true,
                denied: None,
                note: note.into(),
                ..Default::default()
            },
            watch: Ok(()),
            watching: Vec::new(),
            private: None,
            quick: None,
            camera_panel: None,
            end: 0.0,
            previews: vec![
                Preview {
                    surface: ids::SCREEN_SURFACE.into(),
                    title: "Tu pantalla".into(),
                    waiting: "Esperando al sistema…".into(),
                    frame: Some((1920, 1080)),
                    stop: (ids::STOP_SCREEN.into(), "Dejar de compartir"),
                    note: note.into(),
                    actions: vec![(ids::PAUSE.into(), "Pausar")],
                    stream: None,
                    focused: false,
                },
                Preview {
                    surface: ids::CAMERA_SURFACE.into(),
                    title: "Tu cámara".into(),
                    waiting: "Esperando al sistema…".into(),
                    frame: None,
                    stop: (ids::STOP_CAMERA.into(), "Apagar la cámara"),
                    note: note.into(),
                    actions: Vec::new(),
                    stream: None,
                    focused: false,
                },
            ],
        };
        let (_, laid) = frame(stage(view, &Look::ONYX), Size::new(980.0, 762.0));
        let ids = controls(&laid);
        assert!(
            ids.contains(&ids::STOP_SCREEN.to_owned())
                && ids.contains(&ids::STOP_CAMERA.to_owned())
                && ids.contains(&ids::PAUSE.to_owned())
        );
        let shown = texts(&laid);
        for expected in ["Tu pantalla", "Tu cámara", "Esperando al sistema…", note] {
            assert!(
                shown.iter().any(|text| text == expected),
                "{expected} in {shown:?}"
            );
        }
        // Only the frame that arrived is drawn, at 16:9 in its tile.
        let surfaces: Vec<_> = laid
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::Surface { id, rect, .. } => Some((id.clone(), *rect)),
                _ => None,
            })
            .collect();
        assert_eq!(surfaces.len(), 1, "{surfaces:?}");
        assert_eq!(surfaces[0].0, ids::SCREEN_SURFACE);
        let rect = surfaces[0].1;
        assert!((rect.width / rect.height - 16.0 / 9.0).abs() < 0.01);
    }

    #[test]
    fn only_the_cameras_preview_is_mirrored() {
        let mirrored = |surface: &str| {
            let tile = Preview {
                surface: surface.into(),
                title: "Vista previa".into(),
                waiting: "Esperando al sistema…".into(),
                frame: Some((1280, 720)),
                stop: (ids::STOP_CAMERA.into(), "Apagar la cámara"),
                note: "Sólo en esta computadora".into(),
                actions: Vec::new(),
                stream: None,
                focused: false,
            };
            let (_, laid) = frame(
                preview(&tile, Size::new(480.0, 270.0), &Look::ONYX),
                Size::new(480.0, 270.0),
            );
            laid.commands.iter().find_map(|command| match command {
                limn::DrawCommand::Surface { mirrored, .. } => Some(*mirrored),
                _ => None,
            })
        };
        assert_eq!(
            mirrored(ids::CAMERA_SURFACE),
            Some(true),
            "as a webcam shows the person"
        );
        assert_eq!(mirrored(ids::SCREEN_SURFACE), Some(false));
    }

    #[test]
    fn out_of_the_call_the_stage_invites_to_join() {
        let view = Stage {
            channel: "General".into(),
            phase: None,
            people: Vec::new(),
            muted: false,
            deafened: false,
            simulated: true,
            join_denied: None,
            chat: Err("Sin chat".into()),
            size: Size::new(980.0, 762.0),
            card: None,
            tile_menu: None,
            more_open: false,
            video: VideoControls::default(),
            previews: Vec::new(),
            watch: Ok(()),
            watching: Vec::new(),
            private: None,
            quick: None,
            camera_panel: None,
            end: 0.0,
        };
        let (_, laid) = frame(stage(view, &Look::ONYX), Size::new(980.0, 762.0));
        let shown = texts(&laid);
        assert!(shown.contains(&"No hay nadie en el chat de voz en este momento".to_owned()));
        assert!(controls(&laid).contains(&ids::JOIN.to_owned()));
        assert!(!controls(&laid).contains(&ids::LEAVE.to_owned()));
    }

    #[test]
    fn a_row_under_the_channel_marks_who_is_muted_or_deafened() {
        let look = Look::DARK;
        let (_, laid) = frame(
            participant(&person(7, true, true, false), None, None, &look),
            Size::new(280.0, ROW_HEIGHT),
        );
        let ids: Vec<String> = laid
            .regions
            .iter()
            .map(|region| region.id.clone())
            .collect();
        assert!(ids.contains(&ids::row(7)));
        assert!(ids.contains(&format!("{}:muted", ids::row(7))));
        assert!(ids.contains(&format!("{}:deafened", ids::row(7))));
        assert!(!ids.contains(&format!("{}:silenced", ids::row(7))));
        let silenced = Participant {
            silenced: true,
            ..person(7, false, false, false)
        };
        let (_, laid) = frame(
            participant(&silenced, None, None, &look),
            Size::new(280.0, ROW_HEIGHT),
        );
        assert!(laid
            .regions
            .iter()
            .any(|region| region.id == format!("{}:silenced", ids::row(7))));
    }

    #[test]
    fn someone_streaming_is_live_can_be_watched_and_their_stream_takes_the_stage() {
        let streamer = Participant {
            streaming: true,
            ..person(2, false, false, false)
        };
        let view = |watch: Result<(), String>, watching| Stage {
            channel: "General".into(),
            phase: Some(CallPhase::Connected),
            people: vec![person(1, false, false, false), streamer.clone()],
            muted: false,
            deafened: false,
            simulated: true,
            join_denied: None,
            chat: Err("Sin chat".into()),
            size: Size::new(980.0, 762.0),
            card: None,
            tile_menu: None,
            more_open: false,
            video: VideoControls::default(),
            previews: Vec::new(),
            watch,
            watching,
            private: None,
            quick: None,
            camera_panel: None,
            end: 0.0,
        };
        let look = Look::ONYX;
        let (_, laid) = frame(
            stage(view(Ok(()), Vec::new()), &look),
            Size::new(980.0, 762.0),
        );
        assert!(controls(&laid).contains(&ids::watch(2)));
        assert!(
            !controls(&laid).contains(&ids::watch(1)),
            "only who streams"
        );
        assert!(texts(&laid).iter().any(|text| text == "EN VIVO"));
        // Not allowed: shown, disabled, with why.
        let (_, laid) = frame(
            stage(view(Err("Entrá a la llamada".into()), Vec::new()), &look),
            Size::new(980.0, 762.0),
        );
        assert!(!controls(&laid).contains(&ids::watch(2)));
        // Watched: the stream alone, with the way to stop.
        let stream = Preview {
            surface: ids::stream_surface(2),
            title: "Pantalla de Persona 2".into(),
            frame: Some((1280, 720)),
            waiting: "Esperando video…".into(),
            stop: (ids::stop_watching(2), "Dejar de ver"),
            note: "En vivo · simulado".into(),
            actions: Vec::new(),
            stream: None,
            focused: false,
        };
        let (_, laid) = frame(
            stage(view(Ok(()), vec![stream]), &look),
            Size::new(980.0, 762.0),
        );
        let ids = controls(&laid);
        assert!(ids.contains(&ids::stop_watching(2)));
        assert!(!ids.contains(&ids::tile(1)), "the stream takes the stage");
        assert!(laid.commands.iter().any(|command| matches!(
            command,
            limn::DrawCommand::Surface { id, .. } if *id == ids::stream_surface(2)
        )));
        // Their row in the column says it too.
        let (_, laid) = frame(
            participant(&streamer, None, None, &look),
            Size::new(280.0, ROW_HEIGHT),
        );
        assert!(texts(&laid).iter().any(|text| text == "EN VIVO"));
        // In a window of its own: the picture, nothing to click.
        let stream = Preview {
            surface: ids::window_surface(2),
            title: "Pantalla de Persona 2".into(),
            frame: Some((1280, 720)),
            waiting: "Esperando video…".into(),
            stop: (ids::stop_watching(2), "Dejar de ver"),
            note: "En vivo".into(),
            actions: Vec::new(),
            stream: None,
            focused: false,
        };
        let (_, laid) = frame(
            stream_window(&stream, Size::new(960.0, 540.0), &look),
            Size::new(960.0, 540.0),
        );
        assert!(controls(&laid).is_empty());
        assert!(laid.commands.iter().any(|command| matches!(
            command,
            limn::DrawCommand::Surface { id, rect, .. }
                if *id == ids::window_surface(2) && rect.width == 960.0
        )));
        // With a picture of their stream under it, to watch it from there.
        let previewed = Participant {
            thumbnail: Some(Thumbnail::Simulated),
            ..streamer.clone()
        };
        let (_, laid) = frame(
            participant(&previewed, None, None, &look),
            Size::new(280.0, ROW_HEIGHT + 100.0),
        );
        assert!(controls(&laid).contains(&ids::preview(2)));
        assert!(texts(&laid)
            .iter()
            .any(|text| text == "Vista previa simulada"));
    }

    // Streams (transmisiones).
    #[test]
    fn a_stream_focused_takes_the_stage_and_the_rest_go_in_a_strip_under_it() {
        let streaming = |user| Participant {
            streaming: true,
            ..person(user, false, false, false)
        };
        let stream = |user: u64, focused| Preview {
            surface: ids::stream_surface(user),
            title: format!("Pantalla de Persona {user}"),
            frame: Some((1280, 720)),
            waiting: "Esperando video…".into(),
            stop: (ids::stop_watching(user), "Dejar de ver"),
            note: "En vivo".into(),
            actions: Vec::new(),
            stream: Some(StreamTile {
                owner: user,
                sound: user + 1_000,
                silenced: false,
                windowed: false,
                menu: None,
            }),
            focused,
        };
        let view = |watching| Stage {
            channel: "General".into(),
            phase: Some(CallPhase::Connected),
            people: vec![
                person(1, false, false, false),
                streaming(2),
                streaming(3),
                streaming(4),
            ],
            muted: false,
            deafened: false,
            simulated: false,
            join_denied: None,
            chat: Err("Sin chat".into()),
            size: Size::new(980.0, 762.0),
            card: None,
            tile_menu: None,
            more_open: false,
            video: VideoControls::default(),
            previews: Vec::new(),
            watch: Ok(()),
            watching,
            private: None,
            quick: None,
            camera_panel: None,
            end: 0.0,
        };
        let surfaces = |laid: &limn::Frame| -> Vec<(String, limn::Rect)> {
            laid.commands
                .iter()
                .filter_map(|command| match command {
                    limn::DrawCommand::Surface { id, rect, .. } => Some((id.clone(), *rect)),
                    _ => None,
                })
                .collect()
        };
        let look = Look::ONYX;
        let size = Size::new(980.0, 762.0);
        // Side by side, each tile clickable to focus it.
        let (_, grid) = frame(
            stage(view(vec![stream(2, false), stream(3, false)]), &look),
            size,
        );
        let shown = surfaces(&grid);
        assert_eq!(shown.len(), 2);
        let ids = controls(&grid);
        assert!(ids.contains(&ids::stream_tile(2)));
        assert!(ids.contains(&ids::watch(4)));
        // Under the pointer, as the official client's: its sound and «⋯»;
        // the bar hangs up and has nothing at its end.
        assert!(ids.contains(&ids::local_mute(1_002)) && ids.contains(&ids::stream_more(2)));
        assert!(ids.contains(&ids::LEAVE.to_owned()));
        assert!(!ids.contains(&ids::stop_watching(2)) && !ids.contains(&ids::fullscreen(2)));
        // Focused: it takes the stage, the other and who else streams go
        // small under it, without their buttons.
        let (_, focused) = frame(
            stage(view(vec![stream(2, true), stream(3, false)]), &look),
            size,
        );
        let shown = surfaces(&focused);
        let big = shown
            .iter()
            .find(|(id, _)| *id == ids::stream_surface(2))
            .unwrap()
            .1;
        let small = shown
            .iter()
            .find(|(id, _)| *id == ids::stream_surface(3))
            .unwrap()
            .1;
        assert!(big.width > 2.0 * small.width);
        assert!(small.height <= MAX_STRIP && small.y > big.y + big.height);
        let ids = controls(&focused);
        assert!(
            ids.contains(&ids::stream_tile(3)),
            "the strip's focus it instead"
        );
        assert!(ids.contains(&ids::tile(4)) && !ids.contains(&ids::watch(4)));
        // The bar lets go of the one focused in place of hanging up, and
        // its end shows it apart or full screen.
        assert!(ids.contains(&ids::stop_watching(2)) && !ids.contains(&ids::stop_watching(3)));
        assert!(!ids.contains(&ids::LEAVE.to_owned()));
        assert!(ids.contains(&ids::fullscreen(2)) && !ids.contains(&ids::fullscreen(3)));
        assert!(ids.contains(&ids::window(2)));
        let at = |laid: &limn::Frame, id: &str| {
            laid.regions
                .iter()
                .find(|region| region.id == id)
                .unwrap_or_else(|| panic!("{id}"))
                .rect
        };
        let (window, full) = (
            at(&focused, &ids::window(2)),
            at(&focused, &ids::fullscreen(2)),
        );
        let hang_up = at(&focused, &ids::stop_watching(2));
        assert!(window.x < full.x && full.x + full.width <= size.width - BAR_BOTTOM + 0.5);
        assert!(window.x > hang_up.x + hang_up.width, "at the bar's end");
        // Already in a window of its own: only full screen.
        let mut windowed = stream(2, true);
        windowed.stream.as_mut().unwrap().windowed = true;
        let (_, laid) = frame(stage(view(vec![windowed, stream(3, false)]), &look), size);
        assert!(!controls(&laid).contains(&ids::window(2)));
        // Its «⋯» opens its menu there.
        let mut open = stream(2, false);
        open.stream.as_mut().unwrap().menu = Some(vec![MenuEntry::Item(
            ids::stop_watching(2),
            "Dejar de ver".into(),
            None,
        )]);
        let (_, laid) = frame(stage(view(vec![open, stream(3, false)]), &look), size);
        assert!(texts(&laid).contains(&"Dejar de ver".to_owned()));
    }

    #[test]
    fn the_bar_is_spaced_as_the_official_clients_tray() {
        let look = Look::ONYX;
        let size = Size::new(980.0, 762.0);
        let view = Stage {
            channel: "General".into(),
            phase: Some(CallPhase::Connected),
            people: vec![
                person(1, false, false, false),
                person(2, false, false, false),
            ],
            muted: false,
            deafened: false,
            simulated: false,
            join_denied: None,
            chat: Err("Sin chat".into()),
            size,
            card: None,
            tile_menu: None,
            more_open: false,
            video: VideoControls::default(),
            previews: Vec::new(),
            watch: Ok(()),
            watching: Vec::new(),
            private: None,
            quick: None,
            camera_panel: None,
            end: 0.0,
        };
        let (_, laid) = frame(stage(view, &look), size);
        let at = |id: &str| {
            laid.regions
                .iter()
                .find(|region| region.id == id)
                .unwrap_or_else(|| panic!("{id}"))
                .rect
        };
        let (mute, deafen, leave) = (at(ids::MUTE), at(ids::DEAFEN), at(ids::LEAVE));
        assert_eq!((mute.width, mute.height), (BAR_BUTTON, BAR_BUTTON));
        // 8 between a button's arrow and the next button, 12 between sections.
        let arrow = at(ids::INPUT_OPTIONS);
        assert_eq!(arrow.x, mute.x + mute.width + 1.0);
        assert_eq!(deafen.x, arrow.x + arrow.width + 8.0);
        assert_eq!((leave.width, leave.height), (HANG_UP_WIDTH, BAR_HEIGHT));
        assert_eq!(leave.y + leave.height, size.height - BAR_BOTTOM);
        // The tiles end 12 over the bar.
        let tile = at(&ids::tile(1));
        assert!(tile.y + tile.height <= leave.y - BAR_GAP + 0.5);
        // Centered.
        let left = at(ids::MUTE).x - 4.0;
        let right = leave.x + leave.width;
        assert!(((left + right) / 2.0 - size.width / 2.0).abs() < 1.0);
    }

    #[test]
    fn someone_streaming_shows_the_latest_picture_of_their_stream_on_their_tile() {
        let image = limn::RasterImage::new(
            "1".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "vista previa",
        )
        .unwrap();
        let shown = |thumbnail| Participant {
            streaming: true,
            thumbnail,
            ..person(2, false, false, false)
        };
        let look = Look::ONYX;
        let size = Size::new(480.0, 270.0);
        let laid = |person: &Participant| {
            frame(
                tile(person, size, None, None, Some(&Ok(())), false, &look),
                size,
            )
            .1
        };
        let pictured = laid(&shown(Some(Thumbnail::Image(image))));
        assert!(pictured
            .commands
            .iter()
            .any(|command| matches!(command, limn::DrawCommand::RasterImage { .. })));
        let texts_shown = texts(&pictured);
        for expected in ["EN VIVO", "Ver transmisión", "Persona 2"] {
            assert!(
                texts_shown.iter().any(|text| text == expected),
                "{expected}"
            );
        }
        // None yet: their mark; the simulation's says it is made up.
        let plain = laid(&shown(None));
        assert!(!plain
            .commands
            .iter()
            .any(|command| matches!(command, limn::DrawCommand::RasterImage { .. })));
        let simulated = laid(&shown(Some(Thumbnail::Simulated)));
        assert!(texts(&simulated)
            .iter()
            .any(|text| text == "Vista previa simulada"));
    }

    #[test]
    fn a_camera_that_shows_takes_the_place_of_the_mark_and_says_when_simulated() {
        let filmed = Participant {
            camera: Some((640, 360)),
            ..person(2, false, false, false)
        };
        let view = |simulated| Stage {
            channel: "General".into(),
            phase: Some(CallPhase::Connected),
            people: vec![person(1, false, false, false), filmed.clone()],
            muted: false,
            deafened: false,
            simulated,
            join_denied: None,
            chat: Err("Sin chat".into()),
            size: Size::new(980.0, 762.0),
            card: None,
            tile_menu: None,
            more_open: false,
            video: VideoControls::default(),
            previews: Vec::new(),
            watch: Ok(()),
            watching: Vec::new(),
            private: None,
            quick: None,
            camera_panel: None,
            end: 0.0,
        };
        let look = Look::ONYX;
        let surfaces = |laid: &limn::Frame| -> Vec<(String, limn::Rect)> {
            laid.commands
                .iter()
                .filter_map(|command| match command {
                    limn::DrawCommand::Surface { id, rect, .. } => Some((id.clone(), *rect)),
                    _ => None,
                })
                .collect()
        };
        let (_, laid) = frame(stage(view(true), &look), Size::new(980.0, 762.0));
        let shown = surfaces(&laid);
        assert_eq!(shown.len(), 1, "{shown:?}");
        assert_eq!(shown[0].0, ids::camera_surface(2));
        // 16:9 in a 16:9 tile: it fills it.
        assert!((shown[0].1.width / shown[0].1.height - 16.0 / 9.0).abs() < 0.01);
        assert!(texts(&laid).iter().any(|text| text == "Cámara simulada"));
        let (_, laid) = frame(stage(view(false), &look), Size::new(980.0, 762.0));
        assert!(!texts(&laid).iter().any(|text| text == "Cámara simulada"));
        assert!(controls(&laid).contains(&ids::tile(2)));
    }

    #[test]
    fn the_chat_toggles_beside_the_stage_when_the_channel_has_one() {
        let view = |chat| Stage {
            channel: "General".into(),
            phase: Some(CallPhase::Connected),
            people: vec![person(1, false, false, false)],
            muted: false,
            deafened: false,
            simulated: true,
            join_denied: None,
            chat,
            size: Size::new(600.0, 600.0),
            card: None,
            tile_menu: None,
            more_open: false,
            video: VideoControls::default(),
            previews: Vec::new(),
            watch: Ok(()),
            watching: Vec::new(),
            private: None,
            quick: None,
            camera_panel: None,
            end: 0.0,
        };
        for open in [false, true] {
            let (_, laid) = frame(stage(view(Ok(open)), &Look::ONYX), Size::new(600.0, 600.0));
            assert!(controls(&laid).contains(&ids::CHAT.to_owned()));
        }
    }

    // A person's menu, laid out alone as `context_menu` opens it.
    fn person_menu(entries: Vec<MenuEntry>, look: &Look) -> limn::Frame {
        frame(
            context_menu((0.0, 0.0), entries, look),
            Size::new(600.0, 600.0),
        )
        .1
    }

    fn icon_centers(laid: &limn::Frame, wanted: Icon) -> Vec<f32> {
        laid.commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::Icon { rect, icon, .. } if *icon == wanted => {
                    Some(rect.x + rect.width / 2.0)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_person_s_check_boxes_end_their_rows_where_a_submenu_s_chevron_does() {
        let look = Look::ONYX;
        let laid = person_menu(
            vec![
                MenuEntry::Item(
                    "vz:mod:mute:7".into(),
                    "Silenciar en el servidor".into(),
                    Some(true),
                ),
                MenuEntry::Submenu(
                    "Mover a".into(),
                    vec![MenuEntry::Item(
                        "vz:mod:move:7:9".into(),
                        "Sala".into(),
                        None,
                    )],
                ),
                MenuEntry::Separator,
                MenuEntry::Item(ids::local_mute(7), "Silenciar para mí".into(), Some(true)),
            ],
            &look,
        );
        let (checks, chevrons) = (
            icon_centers(&laid, Icon::Check),
            icon_centers(&laid, Icon::ChevronRight),
        );
        assert_eq!((checks.len(), chevrons.len()), (2, 1));
        for check in checks {
            assert_eq!(check, chevrons[0], "one column at the row's end");
        }
    }

    #[test]
    fn a_role_has_a_dot_of_its_color_and_a_check_box_and_punishing_is_in_danger_red() {
        let look = Look::ONYX;
        let red = Rgb(0xE7_4C_3C);
        let laid = person_menu(
            vec![
                MenuEntry::Role {
                    id: "role:1".into(),
                    name: "Moderación".into(),
                    color: Some(red),
                    has: true,
                },
                MenuEntry::Role {
                    id: "role:2".into(),
                    name: "Bots".into(),
                    color: None,
                    has: false,
                },
                MenuEntry::Separator,
                MenuEntry::Item("timeout-off".into(), "Quitar el aislamiento".into(), None),
                MenuEntry::Danger("kick".into(), "Expulsar a Ada".into()),
            ],
            &look,
        );
        let dot = |color: Color| {
            laid.commands.iter().any(|command| {
                matches!(command, limn::DrawCommand::FillRect { rect, color: fill, .. }
                    if *fill == color && rect.width == 12.0 && rect.height == 12.0)
            })
        };
        assert!(dot(look.ink(red)), "the role's own color");
        assert!(dot(look.muted), "a role without one is grey");
        // The role it has is ticked at the end of its row, past its name.
        let name = laid
            .commands
            .iter()
            .find_map(|command| match command {
                // Seven pixels a character, as `frame` measures.
                limn::DrawCommand::Text { text, position, .. } if text == "Moderación" => {
                    Some(position.x + text.chars().count() as f32 * 7.0)
                }
                _ => None,
            })
            .unwrap();
        assert!(icon_centers(&laid, Icon::Check)[0] > name);
        let ink = |wanted: &str| {
            laid.commands
                .iter()
                .find_map(|command| match command {
                    limn::DrawCommand::Text { text, color, .. } if text == wanted => Some(*color),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(ink("Expulsar a Ada"), look.danger);
        assert_eq!(ink("Quitar el aislamiento"), look.text);
    }

    #[test]
    fn a_person_s_volume_is_a_slider_in_their_menu() {
        let look = Look::ONYX;
        let entries = menu_entries(
            vec![
                MenuEntry::Volume {
                    user: 7,
                    label: "Volumen del usuario",
                    volume: 1.5,
                    note: Some("Simulado".into()),
                },
                MenuEntry::Item(ids::local_mute(7), "Silenciar para mí".into(), Some(true)),
            ],
            &look,
        );
        let (_, laid) = frame(
            DropdownMenu::new(
                gap(1.0, 1.0),
                entries,
                true,
                ids::MENU_DISMISS,
                look.menu_palette(),
            )
            .width(MENU_WIDTH)
            .render(),
            Size::new(400.0, 400.0),
        );
        let ids = controls(&laid);
        assert!(ids.contains(&ids::volume(7)), "{ids:?}");
        assert!(ids.contains(&ids::local_mute(7)), "{ids:?}");
        let shown = texts(&laid);
        for expected in [
            "Volumen del usuario",
            "150 %",
            "Simulado",
            "Silenciar para mí",
        ] {
            assert!(
                shown.iter().any(|text| text == expected),
                "{expected} in {shown:?}"
            );
        }
    }

    // Rows in the column (voz-filas-y-arrastre).
    /// The hover's fills of `look.hover` painted in `frame`.
    fn hover_fills(frame: &limn::Frame, look: &Look) -> Vec<limn::Rect> {
        frame
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::FillRect {
                    rect,
                    color,
                    radius,
                } if *color == look.hover => {
                    assert_eq!(*radius, ROW_RADIUS);
                    Some(*rect)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_row_under_the_pointer_lights_up_inside_it_as_a_channel_does() {
        use limn::{Point, UiEvent};
        let look = Look::DARK;
        let width = crate::sidebar::WIDTH - 1.0;
        let channel = crate::sidebar::channel(
            crate::sidebar::ChannelRow {
                id: "voice:1".into(),
                name: "Sala".into(),
                voice: true,
                ..Default::default()
            },
            &look,
        );
        let muted = person(7, true, true, false);
        let column = Element::column(vec![
            channel,
            participant(&muted, None, None, &look),
            participant(&person(8, false, false, false), None, None, &look),
        ]);
        let size = Size::new(width, 200.0);
        let (mut tree, laid) = frame(column, size);
        assert!(hover_fills(&laid, &look).is_empty(), "nothing lit at rest");
        let region = |laid: &limn::Frame, id: &str| {
            laid.regions
                .iter()
                .find(|region| region.id == id)
                .map(|region| region.rect)
                .unwrap()
        };
        let row = region(&laid, &ids::row(7));
        let next = region(&laid, &ids::row(8));
        // The row reaches the column's edge, for the card beside it.
        assert_eq!(row.x + row.width, width);
        tree.handle_event(UiEvent::PointerMove(Point::new(row.x + 20.0, row.y + 10.0)));
        let lit = hover_fills(&tree.layout(size), &look);
        assert_eq!(lit.len(), 1, "{lit:?}");
        let lit = lit[0];
        // Its right edge where a channel's is, short of the scroll bar.
        assert_eq!(lit.x + lit.width, width - crate::sidebar::INSET);
        assert_eq!(lit.x, ROW_INDENT);
        // Inside its row: it does not reach the next one.
        assert_eq!(lit.y, row.y + 1.0);
        assert_eq!(lit.height, ROW_HEIGHT - 2.0);
        assert!(lit.y + lit.height < next.y);
        // The marks stay inside it.
        let deafened = region(&tree.layout(size), &format!("{}:deafened", ids::row(7)));
        assert!(deafened.x + deafened.width <= lit.x + lit.width - 4.0);
        // The channel lights the same way, rounded alike.
        let voice = region(&tree.layout(size), "voice:1");
        tree.handle_event(UiEvent::PointerMove(Point::new(
            voice.x + 20.0,
            voice.y + 10.0,
        )));
        let lit = hover_fills(&tree.layout(size), &look);
        assert_eq!(lit.len(), 1, "{lit:?}");
        assert_eq!(lit[0].x + lit[0].width, width - crate::sidebar::INSET);
    }

    #[test]
    fn the_pointer_on_someone_live_shows_their_stream_s_card_beside_the_row() {
        use limn::{Point, UiEvent};
        let look = Look::DARK;
        let streamer = Participant {
            streaming: true,
            ..person(2, true, false, false)
        };
        let card = LiveCard {
            user: 2,
            name: "Persona 2".into(),
            activity: Some("VALORANT".into()),
            picture: Ok(Thumbnail::Simulated),
            watch: Ok(()),
        };
        let size = Size::new(900.0, 400.0);
        let column = |card: Option<&LiveCard>| {
            Element::row(vec![
                // Away from the window's top, which the card keeps clear of.
                Element::column(vec![
                    gap(0.0, 40.0),
                    participant(&streamer, None, card, &look),
                ])
                .size(Some(crate::sidebar::WIDTH - 1.0), None),
                Element::spacer().flex(1.0),
            ])
        };
        let (mut tree, laid) = frame(column(Some(&card)), size);
        assert!(
            !controls(&laid).contains(&ids::live_watch(2)),
            "not until the pointer is on the row"
        );
        let row = laid
            .regions
            .iter()
            .find(|region| region.id == ids::row(2))
            .unwrap()
            .rect;
        tree.handle_event(UiEvent::PointerMove(Point::new(row.x + 20.0, row.y + 10.0)));
        let shown = tree.layout(size);
        assert!(controls(&shown).contains(&ids::live_watch(2)));
        let words = texts(&shown);
        for expected in [
            "Persona 2",
            "Transmitiendo VALORANT",
            "Vista previa simulada",
            "Ver transmisión",
        ] {
            assert!(
                words.iter().any(|text| text == expected),
                "{expected} in {words:?}"
            );
        }
        // Beside the row, its top with the row's.
        let surface = shown
            .regions
            .iter()
            .find(|region| region.id == format!("popover_surface:{}", ids::LIVE_DISMISS))
            .unwrap()
            .rect;
        assert_eq!(surface.x, row.x + row.width);
        assert_eq!(surface.y, row.y);
        assert_eq!(surface.width, LIVE_CARD_WIDTH + 8.0);
        // On their marks too.
        let muted = shown
            .regions
            .iter()
            .find(|region| region.id == format!("{}:muted", ids::row(2)))
            .unwrap()
            .rect;
        tree.handle_event(UiEvent::PointerMove(Point::new(
            muted.x + 4.0,
            muted.y + 4.0,
        )));
        assert!(controls(&tree.layout(size)).contains(&ids::live_watch(2)));
        // The pointer goes over to it and it stays, to be clicked.
        let watch = shown
            .regions
            .iter()
            .find(|region| region.id == ids::live_watch(2))
            .unwrap()
            .rect;
        for point in [
            Point::new(surface.x + 2.0, row.y + 10.0),
            Point::new(watch.x + 4.0, watch.y + 4.0),
        ] {
            tree.handle_event(UiEvent::PointerMove(point));
            assert!(controls(&tree.layout(size)).contains(&ids::live_watch(2)));
        }
        // Away from both, it goes.
        tree.handle_event(UiEvent::PointerMove(Point::new(5.0, 390.0)));
        assert!(!controls(&tree.layout(size)).contains(&ids::live_watch(2)));
        // A click on «Ver transmisión» reports it.
        tree.handle_event(UiEvent::PointerMove(Point::new(row.x + 20.0, row.y + 10.0)));
        tree.layout(size);
        tree.handle_event(UiEvent::PointerMove(Point::new(
            watch.x + 4.0,
            watch.y + 4.0,
        )));
        tree.layout(size);
        tree.handle_event(UiEvent::PointerDown(Point::new(
            watch.x + 4.0,
            watch.y + 4.0,
        )));
        assert_eq!(
            tree.handle_event(UiEvent::PointerUp(Point::new(watch.x + 4.0, watch.y + 4.0))),
            vec![limn::UiAction::Clicked {
                id: ids::live_watch(2)
            }]
        );
        // Not allowed: disabled, with why; no picture yet: said so.
        let denied = LiveCard {
            picture: Err("Cargando la vista previa…".into()),
            watch: Err("Entrá al canal de voz para ver la transmisión".into()),
            activity: None,
            ..card
        };
        let (_, laid) = frame(live_card(&denied, &look), size);
        assert!(!controls(&laid).contains(&ids::live_watch(2)));
        let words = texts(&laid);
        assert!(words.iter().any(|text| text == "Cargando la vista previa…"));
        assert!(!words.iter().any(|text| text.starts_with("Transmitiendo")));
    }
}
