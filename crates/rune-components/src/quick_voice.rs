//! The quick voice panel (2026-10-18), from the arrow beside the microphone
//! or the headphones: the input's or the output's half of the voice
//! settings in a window of its own, and the pieces it shares with the voice
//! page: the device lists, the percent sliders and the level meter.
//!
//! Its controls report the voice page's own actions (`us:…`), so choosing
//! here and there is the same choice.

use limn::{
    Button, ButtonVariant, Component, DropdownOption, Element, FontFamily, Icon, ScrollArea,
    SegmentedControl, Select, Size, Slider, SurfaceFit, Switch, Tab, Tooltip, TooltipSpec,
};
use rune_i18n::{t, tr};

use crate::profile::Device;
use crate::{gap, line, strong_line, text, Look};

/// The panel: as wide as the reference's, its inset and its corners.
pub const WIDTH: f32 = 340.0;
const INSET: f32 = 16.0;
const RADIUS: f32 = 12.0;
/// Its body scrolls between these heights: the window decides which.
pub const BODY_HEIGHTS: (f32, f32) = (180.0, 460.0);
/// The arrow beside the microphone and the headphones of the call's bar.
pub const ARROW_WIDTH: f32 = 20.0;
/// The lists show the default and this many devices at most.
pub const MAX_DEVICES: usize = 32;
/// A device's name past this many characters is cut; its hint has it whole.
const NAME_CHARS: usize = 40;
/// The meter's height, and where its yellow and its red start.
const METER_HEIGHT: f32 = 9.0;
const YELLOW: f32 = 0.7;
const RED: f32 = 0.9;
/// The slider's track and the value beside it.
const SLIDER_HEIGHT: f32 = 28.0;
const VALUE_WIDTH: f32 = 52.0;

/// The actions of the panel; the rest are the voice page's.
pub mod ids {
    pub const DISMISS: &str = "qv:dismiss";
    /// «Todos los ajustes de voz»: the voice page.
    pub const SETTINGS: &str = "qv:settings";
    /// The privacy code of the call, folded and unfolded.
    pub const PRIVACY: &str = "qv:privacy";
    pub const BODY: &str = "qv:body";
    /// The voice page's actions the panel reports.
    pub const MICROPHONES: &str = "mic";
    pub const SPEAKERS: &str = "speaker";
    pub const INPUT_VOLUME: &str = "us:input-volume";
    pub const OUTPUT_VOLUME: &str = "us:output-volume";
    pub const REFRESH: &str = "us:devices";
    pub const RESET: &str = "us:reset-levels";
    pub const METER: &str = "us:meter";
    pub const NOISE: &str = "us:noise";
    pub const VOICE_MODE: &str = "us:input=voice";
    pub const PUSH_TO_TALK: &str = "us:input=ptt";
    pub const SOUND_TEST: &str = "us:speaker-test";
    /// The camera's panel (llamadas-y-panel-de-voz): its list and its test.
    pub const CAMERAS: &str = "camera";
    pub const VIDEO_TEST: &str = "us:video-test";
}

/// A list of audio devices: its name (`mic`: `us:select:mic`, its devices
/// `us:mic={id}`, the default `us:mic-default`), what is found and chosen.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceList {
    pub name: &'static str,
    pub glyph: Icon,
    pub label: String,
    /// `None` while they are looked for; the error when they could not be.
    pub devices: Option<Result<Vec<Device>, String>>,
    /// `None` is the system's default.
    pub chosen: Option<String>,
    pub open: bool,
}

impl DeviceList {
    /// The device chosen is no longer there.
    pub fn missing(&self) -> bool {
        match (&self.devices, &self.chosen) {
            (Some(Ok(devices)), Some(chosen)) => !devices.iter().any(|it| it.id == *chosen),
            _ => false,
        }
    }
}

/// `name`, cut to fit the lists.
pub fn short_name(name: &str) -> String {
    if name.chars().count() <= NAME_CHARS {
        return name.to_owned();
    }
    let cut: String = name.chars().take(NAME_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// The list: the system's default first, then the devices found, up to
/// [`MAX_DEVICES`]. Disabled while they are looked for or could not be;
/// a chosen device that went away reads «Dispositivo no disponible». The
/// name chosen, when cut, is whole in its hint.
pub fn device_select(list: &DeviceList, width: f32, look: &Look) -> Element {
    let select = |current: &str, options: Vec<DropdownOption>, enabled: bool| {
        Select::new(
            format!("us:select:{}", list.name),
            current,
            options,
            list.open && enabled,
            "us:select:dismiss",
            look.palette(),
        )
        .menu_palette(look.menu_palette())
        .icon(list.glyph)
        .accessibility_label(list.label.as_str())
        .width(width)
        .menu_width(width)
        .enabled(enabled)
        .render()
    };
    let devices = match &list.devices {
        None => return select(t("Buscando dispositivos…"), Vec::new(), false),
        Some(Err(_)) => return select(t("Sin dispositivos"), Vec::new(), false),
        Some(Ok(devices)) => devices,
    };
    let chosen = list.chosen.as_deref();
    let options = device_options(list, devices);
    let whole = match chosen {
        None => t("Predeterminado del sistema").to_owned(),
        Some(id) => devices
            .iter()
            .find(|device| device.id == id)
            .map_or(t("Dispositivo no disponible").to_owned(), |device| {
                device.name.clone()
            }),
    };
    let shown = short_name(&whole);
    let cut = shown != whole;
    let element = select(&shown, options, true);
    if cut {
        Tooltip::new(
            element,
            format!("us:select:{}:hint", list.name),
            whole,
            look.menu_palette(),
        )
        .render()
    } else {
        element
    }
}

/// The options of `list`: the system's default, then up to [`MAX_DEVICES`]
/// of `devices`, their names cut and, when cut, whole in their hint.
pub fn device_options(list: &DeviceList, devices: &[Device]) -> Vec<DropdownOption> {
    let chosen = list.chosen.as_deref();
    let mut options = vec![DropdownOption::new(
        format!("us:{}-default", list.name),
        t("Predeterminado del sistema"),
    )
    .icon(list.glyph)
    .selected(chosen.is_none())];
    options.extend(devices.iter().take(MAX_DEVICES).map(|device| {
        let shown = short_name(&device.name);
        let option = DropdownOption::new(format!("us:{}={}", list.name, device.id), &shown)
            .icon(list.glyph)
            .selected(chosen == Some(device.id.as_str()));
        if shown == device.name {
            option
        } else {
            option.tooltip(device.name.clone())
        }
    }));
    options
}

/// A volume from 0 to `most`, as a track with its value in percent at its
/// right. It snaps to whole percents, so the arrow keys step by one.
pub fn percent_slider(
    id: &str,
    label: &str,
    value: f32,
    most: f32,
    width: f32,
    look: &Look,
) -> Element {
    let percent = (value.clamp(0.0, most) * 100.0).round();
    let stops = (most * 100.0).round() as usize + 1;
    Element::row(vec![
        Slider::new(id, value / most, look.palette())
            .steps(stops)
            .width((width - VALUE_WIDTH).max(40.0))
            .ink(look.accent)
            .accessibility_label(label)
            .render()
            .centered_y(),
        Element::row(vec![
            Element::spacer().flex(1.0),
            line(tr!("{percent} %", percent), 13.0, look.text)
                .font_family(FontFamily::Monospace)
                .centered_y(),
        ])
        .size(Some(VALUE_WIDTH), None),
    ])
    .size(Some(width), Some(SLIDER_HEIGHT))
}

/// How much of the meter each color takes at `level` (0 to 1): green to
/// 70 %, yellow to 90 %, red past it, and what is left empty.
pub fn meter_zones(level: f32) -> [f32; 4] {
    let level = if level.is_nan() {
        0.0
    } else {
        level.clamp(0.0, 1.0)
    };
    [
        level.min(YELLOW),
        (level.min(RED) - YELLOW).max(0.0),
        (level - RED).max(0.0),
        1.0 - level,
    ]
}

/// The microphone's level as a bar, green, yellow and red as it gets loud,
/// and the threshold where voice starts as a mark over it.
pub fn level_meter(
    id: &str,
    level: Option<f32>,
    threshold: Option<f32>,
    width: f32,
    look: &Look,
) -> Element {
    let zones = meter_zones(level.unwrap_or(0.0));
    let bar = |share: f32, color| {
        Element::panel(Element::spacer())
            .background(color)
            .flex(share.max(0.0001))
    };
    let fill = Element::row(vec![
        bar(zones[0], look.online),
        bar(zones[1], look.idle),
        bar(zones[2], look.danger),
        Element::spacer().flex(zones[3].max(0.0001)),
    ])
    .radius(METER_HEIGHT / 2.0)
    .clip_children();
    let track = Element::stack(vec![
        Element::panel(Element::spacer())
            .background(look.selected)
            .radius(METER_HEIGHT / 2.0),
        fill,
    ])
    .size(None, Some(METER_HEIGHT));
    let mut layers = vec![Element::column(vec![
        Element::spacer().flex(1.0),
        track,
        Element::spacer().flex(1.0),
    ])];
    if let Some(threshold) = threshold {
        let threshold = threshold.clamp(0.0005, 0.9995);
        layers.push(Element::row(vec![
            Element::spacer().flex(threshold),
            Element::panel(Element::spacer())
                .size(Some(2.0), None)
                .radius(1.0)
                .background(look.text),
            Element::spacer().flex(1.0 - threshold),
        ]));
    }
    Element::stack(layers)
        .size(Some(width), Some(METER_HEIGHT + 8.0))
        .accessible(
            limn::Access::new(limn::Role::ProgressIndicator)
                .id(id)
                .name(t("Nivel del micrófono"))
                .value(level.unwrap_or(0.0)),
        )
}

/// What the input's half shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub devices: DeviceList,
    /// From 0 to [`Input::most`].
    pub volume: f32,
    pub most: f32,
    /// The meter runs: the level heard, where voice starts, and both as
    /// one reads them («−32 dBFS · umbral −45 dBFS»).
    pub testing: bool,
    pub level: Option<f32>,
    pub threshold: Option<f32>,
    pub reading: Option<String>,
    pub push_to_talk: bool,
    pub suppression: bool,
    /// The volumes are not as they start: «Restablecer niveles» does something.
    pub changed: bool,
}

/// What the output's half shows.
#[derive(Clone, Debug)]
pub struct Output {
    pub devices: DeviceList,
    pub volume: f32,
    pub most: f32,
    /// What the sound test or the provider adds, when it adds something.
    pub note: Option<String>,
    /// The call's privacy code, while in one with it: unfolded, its view.
    pub privacy: Option<Option<Element>>,
}

fn label(value: &str, look: &Look) -> Element {
    strong_line(value, 12.0, look.muted)
}

fn switch_row(id: &str, value: &str, on: bool, look: &Look) -> Element {
    Element::row(vec![
        line(value, 14.0, look.text).flex(1.0).centered_y(),
        Switch::new(id, on, look.palette())
            .accessibility_label(value)
            .render()
            .centered_y(),
    ])
}

fn secondary(id: &str, value: &str, enabled: bool, look: &Look) -> Element {
    Button::new(id, value, look.palette())
        .variant(ButtonVariant::Secondary)
        .enabled(enabled)
        .render()
}

/// The note under the volumes: what 100 % is.
pub fn volume_note(look: &Look) -> Element {
    text(
        t("100 % es el nivel original; más alto puede distorsionar."),
        12.0,
        look.muted,
    )
}

/// The input's half: the microphone, its volume, its level with the
/// threshold, how it goes through (voice or push-to-talk), noise
/// suppression, and finding the devices again or putting the levels back.
pub fn input(view: &Input, look: &Look) -> Vec<Element> {
    let width = WIDTH - 2.0 * INSET;
    let mut body = vec![Element::column(vec![
        label(t("Dispositivo de entrada"), look),
        device_select(&view.devices, width, look),
    ])
    .gap(6.0)];
    if view.devices.missing() {
        body.push(text(
            t("El micrófono elegido no está conectado. Conectalo o elegí otro."),
            12.0,
            look.danger,
        ));
    }
    if let Some(Err(error)) = &view.devices.devices {
        body.push(text(
            tr!("No se pudieron leer los dispositivos: {error}", error),
            12.0,
            look.danger,
        ));
    }
    body.push(
        Element::column(vec![
            label(t("Volumen de entrada"), look),
            percent_slider(
                ids::INPUT_VOLUME,
                t("Volumen de entrada"),
                view.volume,
                view.most,
                width,
                look,
            ),
            volume_note(look),
        ])
        .gap(6.0),
    );
    let meter = if view.testing {
        Element::column(vec![
            level_meter("qv:level", view.level, view.threshold, width, look),
            Element::row(vec![
                line(view.reading.clone().unwrap_or_default(), 12.0, look.muted)
                    .flex(1.0)
                    .centered_y(),
                Button::new(ids::METER, t("Detener"), look.palette())
                    .variant(ButtonVariant::Link)
                    .render(),
            ]),
        ])
        .gap(4.0)
    } else {
        Element::row(vec![
            line(t("El nivel se ve mientras probás."), 12.0, look.muted)
                .flex(1.0)
                .centered_y(),
            secondary(ids::METER, t("Probar micrófono"), true, look),
        ])
        .gap(8.0)
    };
    body.push(Element::column(vec![label(t("Nivel del micrófono"), look), meter]).gap(6.0));
    let half = (width - 8.0) / 2.0;
    body.push(
        Element::column(vec![
            label(t("Modo de entrada"), look),
            SegmentedControl::new(
                vec![
                    Tab::new(ids::VOICE_MODE, t("Detección de voz"), half)
                        .active(!view.push_to_talk),
                    Tab::new(ids::PUSH_TO_TALK, t("Pulsar para hablar"), half)
                        .active(view.push_to_talk),
                ],
                look.palette(),
            )
            .height(32.0)
            .render(),
        ])
        .gap(6.0),
    );
    body.push(switch_row(
        ids::NOISE,
        t("Supresión de ruido"),
        view.suppression,
        look,
    ));
    body.push(
        Element::row(vec![
            secondary(ids::REFRESH, t("Actualizar dispositivos"), true, look),
            secondary(ids::RESET, t("Restablecer niveles"), view.changed, look),
        ])
        .gap(8.0),
    );
    body
}

/// The output's half: the speaker, its volume, a call sound to check it,
/// what deafening does and, in a call that has one, its privacy code.
pub fn output(view: &Output, look: &Look) -> Vec<Element> {
    let width = WIDTH - 2.0 * INSET;
    let mut body = vec![Element::column(vec![
        label(t("Dispositivo de salida"), look),
        device_select(&view.devices, width, look),
    ])
    .gap(6.0)];
    if view.devices.missing() {
        body.push(text(
            t("La salida elegida no está conectada. Conectala o elegí otra."),
            12.0,
            look.danger,
        ));
    }
    body.push(
        Element::column(vec![
            label(t("Volumen de salida"), look),
            percent_slider(
                ids::OUTPUT_VOLUME,
                t("Volumen de salida"),
                view.volume,
                view.most,
                width,
                look,
            ),
            volume_note(look),
        ])
        .gap(6.0),
    );
    body.push(
        Element::row(vec![
            secondary(ids::SOUND_TEST, t("Probar sonido"), true, look),
            secondary(ids::REFRESH, t("Actualizar dispositivos"), true, look),
        ])
        .gap(8.0),
    );
    if let Some(note) = &view.note {
        body.push(text(note.as_str(), 12.0, look.muted));
    }
    body.push(text(
        t("Ensordecer apaga lo que oís y también tu micrófono."),
        12.0,
        look.muted,
    ));
    if let Some(privacy) = &view.privacy {
        let open = privacy.is_some();
        body.push(
            Element::clickable(
                ids::PRIVACY,
                Element::row(vec![
                    Element::icon(Icon::Lock)
                        .foreground(look.online)
                        .size(Some(14.0), Some(14.0))
                        .centered_y(),
                    gap(8.0, 0.0),
                    line(t("Código de privacidad"), 14.0, look.text)
                        .flex(1.0)
                        .centered_y(),
                    Element::icon(if open {
                        Icon::ChevronUp
                    } else {
                        Icon::ChevronDown
                    })
                    .foreground(look.muted)
                    .size(Some(12.0), Some(12.0))
                    .centered_y(),
                ])
                .padding(6.0),
            )
            .radius(6.0)
            .hover_background(look.hover)
            .accessibility_label(t("Código de privacidad")),
        );
        if let Some(code) = privacy {
            body.push(code.clone());
        }
    }
    body
}

/// What the camera's panel shows (llamadas-y-panel-de-voz), from the arrow
/// beside the camera of the call's bar, as Serein has it: the camera to
/// use, its picture, and a way to try it without sending it.
#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    /// Named [`ids::CAMERAS`]: the voice page's own list.
    pub devices: DeviceList,
    /// The size of the frame on the camera's surface, while it shows.
    pub frame: Option<(u32, u32)>,
    /// «Probar video» is on: the camera shows here and goes nowhere.
    pub testing: bool,
    /// The camera is on in the call: its picture shows here too.
    pub on: bool,
    /// Why the camera cannot be used here, if it cannot.
    pub denied: Option<String>,
    /// What the picture is: a test on this machine, or what goes to the call.
    pub note: String,
}

/// The camera's picture, `size` large on black, or `waiting` while there is
/// none. Scaled to its own box, so it can show while the stage shows the
/// same camera at another size.
// shortcut: not mirrored, as limn's surface cannot flip yet; mirror this
// preview alone once it can (package limn-teclado-y-superficies).
pub fn camera_preview(
    frame: Option<(u32, u32)>,
    waiting: &str,
    size: Size,
    look: &Look,
) -> Element {
    let picture = match frame {
        // Mirrored, as a webcam shows the person in front of it; what goes
        // to the call is not.
        Some(_) => Element::surface(crate::voice::ids::CAMERA_SURFACE, t("Tu cámara"))
            .surface_fit(SurfaceFit::Contain)
            .surface_mirrored(true)
            .size(Some(size.width), Some(size.height)),
        None => crate::centered(line(waiting, 14.0, look.muted)),
    };
    Element::panel(picture)
        .size(Some(size.width), Some(size.height))
        .radius(8.0)
        .background(limn::Color::rgb(0, 0, 0))
        .clip_children()
}

/// The camera's half: the camera, its picture and «Probar video» while it
/// is not on in the call, and what the picture is.
pub fn camera(view: &Camera, look: &Look) -> Vec<Element> {
    let width = WIDTH - 2.0 * INSET;
    let mut body = vec![Element::column(vec![
        label(t("Cámara"), look),
        device_select(&view.devices, width, look),
    ])
    .gap(6.0)];
    if matches!(&view.devices.devices, Some(Ok(found)) if found.is_empty()) {
        body.push(text(
            t("Las cámaras aparecen cuando el sistema da acceso."),
            12.0,
            look.muted,
        ));
    }
    let showing = view.testing || view.on;
    body.push(camera_preview(
        view.frame.filter(|_| showing),
        if showing {
            t("Esperando la cámara…")
        } else {
            t("Probá el video para verte acá.")
        },
        Size::new(width, width * 9.0 / 16.0),
        look,
    ));
    if !view.on {
        let (title, glyph) = if view.testing {
            (t("Detener video"), Icon::VideoOff)
        } else {
            (t("Probar video"), Icon::Video)
        };
        let test = Button::new(ids::VIDEO_TEST, title, look.palette())
            .variant(ButtonVariant::Secondary)
            .icon(glyph);
        body.push(match &view.denied {
            Some(reason) => test
                .tooltip_with(TooltipSpec::new(title).description(reason.clone()))
                .disabled()
                .render(),
            None => test.render(),
        });
    }
    body.push(text(
        view.denied.as_deref().unwrap_or(&view.note),
        12.0,
        look.muted,
    ));
    body
}

/// The window: `title`, its body scrolling at most `body_height` high, and
/// «Todos los ajustes de voz» across its foot. In a window too low for it,
/// the body gives up height and scrolls, so the title and the foot stay in
/// sight (limn keeps the popover inside the window).
pub fn panel(title: &str, body: Vec<Element>, body_height: f32, look: &Look) -> Element {
    let (low, high) = BODY_HEIGHTS;
    let body_height = body_height.clamp(low, high);
    Element::panel(
        Element::column(vec![
            strong_line(title, 16.0, look.text),
            ScrollArea::new(ids::BODY, Element::column(body).gap(14.0))
                .render()
                .min_size(0.0, low)
                .max_height(body_height)
                .shrink_height_to_fit(),
            Element::panel(Element::spacer())
                .size(None, Some(1.0))
                .background(look.border),
            Button::new(ids::SETTINGS, t("Todos los ajustes de voz"), look.palette())
                .variant(ButtonVariant::Secondary)
                .icon(Icon::Settings)
                .render()
                .size(Some(WIDTH - 2.0 * INSET), None),
        ])
        .gap(12.0)
        .padding(INSET),
    )
    .size(Some(WIDTH), None)
    .background(look.menu)
    .border(look.border, 1.0)
    .radius(RADIUS)
    .shadow(look.shadow, limn::Point::new(0.0, 8.0), 20.0, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    fn devices(open: bool, chosen: Option<&str>) -> DeviceList {
        DeviceList {
            name: ids::MICROPHONES,
            glyph: Icon::Microphone,
            label: "Micrófono".into(),
            devices: Some(Ok(vec![
                Device {
                    id: "a".into(),
                    name: "Micrófono de prueba".into(),
                    default: true,
                },
                Device {
                    id: "b".into(),
                    name: "Un dispositivo con un nombre larguísimo que no entra en la lista".into(),
                    default: false,
                },
            ])),
            chosen: chosen.map(str::to_owned),
            open,
        }
    }

    fn input_view(push_to_talk: bool, testing: bool) -> Input {
        Input {
            devices: devices(false, None),
            volume: 1.0,
            most: 2.0,
            testing,
            level: testing.then_some(0.5),
            threshold: Some(0.4),
            reading: testing.then(|| "−30 dBFS".into()),
            push_to_talk,
            suppression: true,
            changed: false,
        }
    }

    #[test]
    fn the_meter_is_green_then_yellow_then_red() {
        assert_eq!(meter_zones(0.5), [0.5, 0.0, 0.0, 0.5]);
        let [green, yellow, red, rest] = meter_zones(0.95);
        assert_eq!(green, 0.7);
        assert!((yellow - 0.2).abs() < 1e-6 && (red - 0.05).abs() < 1e-6);
        assert!((rest - 0.05).abs() < 1e-6);
        assert_eq!(meter_zones(f32::NAN), [0.0, 0.0, 0.0, 1.0]);
        let [_, yellow, red, rest] = meter_zones(3.0);
        assert!((yellow - 0.2).abs() < 1e-6 && (red - 0.1).abs() < 1e-6 && rest == 0.0);
    }

    #[test]
    fn long_names_are_cut_and_a_missing_device_says_so() {
        assert_eq!(short_name("Corto"), "Corto");
        let cut = short_name(&"x".repeat(80));
        assert!(cut.ends_with('…') && cut.chars().count() == NAME_CHARS);
        assert!(devices(false, Some("gone")).missing());
        assert!(!devices(false, Some("a")).missing() && !devices(false, None).missing());
        let look = Look::DARK;
        let (_, laid) = frame(
            device_select(&devices(false, Some("gone")), 300.0, &look),
            Size::new(300.0, 40.0),
        );
        assert!(texts(&laid).contains(&"Dispositivo no disponible".to_owned()));
        // Each option cut to fit says its whole name in its own hint.
        let list = devices(true, None);
        let Some(Ok(found)) = &list.devices else {
            unreachable!()
        };
        let options = device_options(&list, found);
        let long = "Un dispositivo con un nombre larguísimo que no entra en la lista";
        assert_eq!(options[2].tooltip.as_deref(), Some(long));
        assert!(options[2].label.ends_with('…'));
        assert!(
            options[..2].iter().all(|option| option.tooltip.is_none()),
            "a whole name needs no hint"
        );
    }

    #[test]
    fn the_list_offers_the_default_first_and_at_most_32_devices() {
        let look = Look::DARK;
        let mut list = devices(true, None);
        list.devices = Some(Ok((0..40)
            .map(|n| Device {
                id: format!("d{n}"),
                name: format!("Dispositivo {n}"),
                default: false,
            })
            .collect()));
        let Some(Ok(found)) = &list.devices else {
            unreachable!()
        };
        let options = device_options(&list, found);
        assert_eq!(options.len(), 1 + MAX_DEVICES);
        assert_eq!(options[0].action_id, "us:mic-default");
        assert!(options[0].selected);
        assert_eq!(options[MAX_DEVICES].action_id, "us:mic=d31");
        list.devices = None;
        let (_, laid) = frame(device_select(&list, 300.0, &look), Size::new(300.0, 40.0));
        assert!(texts(&laid).contains(&"Buscando dispositivos…".to_owned()));
    }

    #[test]
    fn the_input_half_has_its_controls_and_the_meter_only_while_testing() {
        let look = Look::DARK;
        let (_, laid) = frame(
            panel(
                "Ajustes de entrada",
                input(&input_view(true, false), &look),
                900.0,
                &look,
            ),
            Size::new(WIDTH, 900.0),
        );
        let ids = controls(&laid);
        for id in [
            "us:select:mic",
            ids::INPUT_VOLUME,
            ids::METER,
            ids::VOICE_MODE,
            ids::PUSH_TO_TALK,
            ids::NOISE,
            ids::REFRESH,
            ids::SETTINGS,
        ] {
            assert!(ids.contains(&id.to_owned()), "{id}: {ids:?}");
        }
        let painted = texts(&laid);
        assert!(painted.contains(&"Ajustes de entrada".to_owned()));
        assert!(painted.contains(&"Probar micrófono".to_owned()));
        assert!(painted.contains(&"100 %".to_owned()), "{painted:?}");
        let (_, laid) = frame(
            panel(
                "Ajustes de entrada",
                input(&input_view(false, true), &look),
                900.0,
                &look,
            ),
            Size::new(WIDTH, 900.0),
        );
        let painted = texts(&laid);
        assert!(painted.contains(&"−30 dBFS".to_owned()), "{painted:?}");
        assert!(painted.contains(&"Detener".to_owned()));
    }

    #[test]
    fn the_output_half_tests_with_a_sound_and_folds_the_privacy_code() {
        let look = Look::DARK;
        let view = |privacy| Output {
            devices: DeviceList {
                name: ids::SPEAKERS,
                ..devices(false, None)
            },
            volume: 0.5,
            most: 2.0,
            note: None,
            privacy,
        };
        let (_, laid) = frame(
            panel(
                "Ajustes de salida",
                output(&view(None), &look),
                900.0,
                &look,
            ),
            Size::new(WIDTH, 900.0),
        );
        let ids = controls(&laid);
        assert!(ids.contains(&ids::SOUND_TEST.to_owned()));
        assert!(ids.contains(&"us:select:speaker".to_owned()));
        assert!(!ids.contains(&ids::PRIVACY.to_owned()), "not in a call");
        assert!(texts(&laid).contains(&"50 %".to_owned()));
        let code = Element::text("12345 67890");
        let (_, laid) = frame(
            panel(
                "Ajustes de salida",
                output(&view(Some(Some(code))), &look),
                900.0,
                &look,
            ),
            Size::new(WIDTH, 900.0),
        );
        assert!(controls(&laid).contains(&ids::PRIVACY.to_owned()));
        assert!(texts(&laid).contains(&"12345 67890".to_owned()));
    }

    /// The panel over an arrow at the foot of a window `height` high: where
    /// its title is painted, and its foot's button and its body laid out.
    fn over_the_foot(height: f32) -> (f32, limn::Rect, limn::Rect) {
        let look = Look::DARK;
        let body = input(&input_view(false, true), &look);
        let arrow = Element::clickable("arrow", Element::spacer()).size(Some(20.0), Some(20.0));
        let root = Element::column(vec![
            Element::spacer().flex(1.0),
            Element::row(vec![
                Element::popover(
                    arrow,
                    panel("Ajustes de entrada", body, 420.0, &look),
                    ids::DISMISS,
                    limn::PopoverPlacement::TopStart,
                ),
                Element::spacer().flex(1.0),
            ])
            .size(None, Some(20.0)),
        ]);
        let (_, laid) = frame(root, Size::new(420.0, height));
        let title = laid
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::Text { text, position, .. } if text == "Ajustes de entrada" => {
                    Some(position.y)
                }
                _ => None,
            })
            .expect("the title is painted");
        let region = |id: &str| {
            laid.regions
                .iter()
                .find(|region| region.id == id)
                .unwrap_or_else(|| panic!("{id} is laid out"))
                .rect
        };
        (title, region(ids::SETTINGS), region(ids::BODY))
    }

    #[test]
    fn in_a_low_window_the_body_scrolls_and_the_title_and_foot_stay_in_sight() {
        let (_, _, tall) = over_the_foot(900.0);
        assert!(tall.height > 300.0, "{tall:?}");
        let (title, settings, body) = over_the_foot(380.0);
        assert!(title >= 0.0, "the title is cut at the top: {title}");
        assert!(settings.height >= 28.0, "{settings:?}");
        assert!(
            settings.y + settings.height <= 380.0 - 20.0,
            "the foot stays above the arrow: {settings:?}"
        );
        assert!(body.height < tall.height && body.height > 0.0, "{body:?}");
    }

    fn camera_view(testing: bool, on: bool, frame: Option<(u32, u32)>) -> Camera {
        Camera {
            devices: DeviceList {
                name: ids::CAMERAS,
                glyph: Icon::Video,
                label: "Cámara".into(),
                devices: Some(Ok(Vec::new())),
                chosen: None,
                open: false,
            },
            frame,
            testing,
            on,
            denied: None,
            note: "Vista previa local".into(),
        }
    }

    /// The camera's previews, every one mirrored (limn-teclado-y-superficies).
    fn surfaces(laid: &limn::Frame) -> Vec<limn::Rect> {
        laid.commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::Surface {
                    id, rect, mirrored, ..
                } if id == crate::voice::ids::CAMERA_SURFACE => {
                    assert!(mirrored, "as a webcam shows the person");
                    Some(*rect)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_camera_half_tries_the_camera_and_shows_its_picture() {
        let look = Look::DARK;
        let laid = |view: &Camera| {
            frame(
                panel("Ajustes de cámara", camera(view, &look), 900.0, &look),
                Size::new(WIDTH, 900.0),
            )
            .1
        };
        // Off: the list, «Probar video» and no picture yet.
        let off = laid(&camera_view(false, false, Some((640, 480))));
        let ids = controls(&off);
        assert!(ids.contains(&"us:select:camera".to_owned()), "{ids:?}");
        assert!(ids.contains(&ids::VIDEO_TEST.to_owned()));
        assert!(surfaces(&off).is_empty(), "nothing shows before the test");
        let painted = texts(&off);
        assert!(painted.contains(&"Probar video".to_owned()));
        assert!(painted.contains(&"Probá el video para verte acá.".to_owned()));
        assert!(painted
            .iter()
            .any(|it| it.starts_with("Las cámaras aparecen")));
        // Testing: its picture, scaled into the 16:9 box, and a way to stop.
        let testing = laid(&camera_view(true, false, Some((640, 480))));
        let shown = surfaces(&testing);
        assert_eq!(shown.len(), 1);
        let width = WIDTH - 2.0 * INSET;
        assert!(
            (shown[0].width - width).abs() < 0.5
                && (shown[0].height - width * 9.0 / 16.0).abs() < 0.5
        );
        assert!(texts(&testing).contains(&"Detener video".to_owned()));
        let waiting = laid(&camera_view(true, false, None));
        assert!(texts(&waiting).contains(&"Esperando la cámara…".to_owned()));
        // On in the call: its picture, and no test.
        let on = laid(&camera_view(false, true, Some((1280, 720))));
        assert_eq!(surfaces(&on).len(), 1);
        assert!(!controls(&on).contains(&ids::VIDEO_TEST.to_owned()));
        // Where it cannot be used, the test says why and does nothing.
        let mut denied = camera_view(false, false, None);
        denied.denied = Some("Solo en Linux".into());
        let denied = laid(&denied);
        assert!(!controls(&denied).contains(&ids::VIDEO_TEST.to_owned()));
        assert!(texts(&denied).contains(&"Solo en Linux".to_owned()));
    }
}
