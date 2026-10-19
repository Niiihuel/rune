//! The quick voice panel (2026-10-18), as Serein has it: the arrow beside
//! the microphone or the headphones, in the account's panel or the call's
//! bar, opens the input's or the output's half of the voice settings over
//! it. Its controls are the voice page's (`us:…`), so they share their
//! state; this opens and closes it, and fills it.
//!
//! The devices are listed once when it opens, without opening them. Its
//! level meter shows only while the microphone is open already, by the
//! call or by «Probar micrófono»; closing the panel stops what it tested.

use std::time::Instant;

use limn::{Element, UiAction};
use rune_components::profile::{ids as profile_ids, Device};
use rune_components::quick_voice::{self as parts, ids, DeviceList, Input, Output};
use rune_components::voice::ids as voice_ids;
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_provider::VoiceSupport;
use rune_state::Store;

use crate::{settings, Intent, Ui};

/// How high the panel's body grows before it scrolls; a lower window gives
/// it what is left, and it scrolls sooner.
const BODY_HEIGHT: f32 = 420.0;

/// The panel while open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QuickVoice {
    /// The output's half, the headphones'; else the input's.
    pub output: bool,
    /// Opened from the call's bar; else from the account's panel.
    pub bar: bool,
    /// The call's privacy code is unfolded.
    pub privacy: bool,
    /// The camera's panel, from the arrow beside the bar's camera
    /// (llamadas-y-panel-de-voz); `output` is then false.
    pub camera: bool,
}

/// Open, close or steer the panel; `None` for other actions.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    let (output, bar, camera) = match id.as_str() {
        profile_ids::INPUT => (false, false, false),
        profile_ids::OUTPUT => (true, false, false),
        voice_ids::INPUT_OPTIONS => (false, true, false),
        voice_ids::OUTPUT_OPTIONS => (true, true, false),
        voice_ids::CAMERA_OPTIONS => (false, true, true),
        ids::DISMISS => return Some(close(ui)),
        ids::SETTINGS => {
            let mut intents = close(ui);
            intents.extend(settings::clicked(ui, store, "us:page=voice", now));
            return Some(intents);
        }
        ids::PRIVACY => {
            if let Some(quick) = ui.profiles.devices.as_mut() {
                quick.privacy ^= true;
            }
            return Some(Vec::new());
        }
        _ => return None,
    };
    let again = ui
        .profiles
        .devices
        .is_some_and(|quick| quick.output == output && quick.bar == bar && quick.camera == camera);
    let mut intents = close(ui);
    if again {
        return Some(intents);
    }
    crate::profile::close_floating(ui);
    ui.voice.more = false;
    ui.profiles.devices = Some(QuickVoice {
        output,
        bar,
        privacy: false,
        camera,
    });
    // The cameras are listed when the system gives access to one.
    if camera {
        return Some(intents);
    }
    if ui.audio.devices.is_none() {
        intents.push(Intent::RefreshDevices);
    }
    // The call holds the microphone already: its level costs nothing more.
    if !output && !ui.audio.testing && live(ui, store) {
        intents.extend(settings::clicked(ui, store, ids::METER, now));
    }
    Some(intents)
}

/// Close the panel and what it tested, unless the voice page shows it too.
pub(crate) fn close(ui: &mut Ui) -> Vec<Intent> {
    if ui.profiles.devices.take().is_none() {
        return Vec::new();
    }
    ui.settings_state.select = None;
    if ui.settings.is_none() {
        settings::stop_meter(ui)
    } else {
        Vec::new()
    }
}

/// Without a call there is no bar: a panel opened from it closes, and the
/// microphone or camera it tested stops (llamadas-y-panel-de-voz).
pub(crate) fn on_applied(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    if ui.profiles.devices.is_some_and(|quick| quick.bar) && !crate::video::in_call(store) {
        close(ui)
    } else {
        Vec::new()
    }
}

/// A call carrying audio holds the output: what the microphone hears cannot
/// be played back to oneself meanwhile.
pub(crate) fn call_holds_output(store: &Store) -> bool {
    store.call().is_some()
        && store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.voice == VoiceSupport::Real)
}

/// The call holds the microphone open now.
fn live(ui: &Ui, store: &Store) -> bool {
    !ui.muted
        && store.mic_held().is_none()
        && store
            .call()
            .is_some_and(|call| call.phase.is_connected() && !call.muted)
}

/// The panel open from the call's bar (`bar`) or from the account's panel,
/// with whether it is the output's.
pub(crate) fn view(ui: &Ui, store: &Store, look: &Look, bar: bool) -> Option<(bool, Element)> {
    let quick = ui
        .profiles
        .devices
        .filter(|quick| quick.bar == bar && !quick.camera)?;
    let (title, body) = if quick.output {
        (
            t("Ajustes de salida"),
            parts::output(&output(ui, store, quick, look), look),
        )
    } else {
        (t("Ajustes de entrada"), parts::input(&input(ui), look))
    };
    Some((quick.output, parts::panel(title, body, BODY_HEIGHT, look)))
}

/// The camera's panel over the bar's camera arrow, while open: the camera,
/// its picture, and «Probar video» while it is off in the call. The test
/// never sends the camera, and closing the panel stops it.
pub(crate) fn camera_view(ui: &Ui, store: &Store, look: &Look) -> Option<Element> {
    ui.profiles.devices.filter(|quick| quick.camera)?;
    let video = &ui.video;
    let on = crate::video::in_call(store) && video.camera.on;
    let mut cameras: Vec<Device> = video
        .cameras
        .iter()
        .map(|(id, name)| Device {
            id: id.clone(),
            name: name.clone(),
            default: false,
        })
        .collect();
    // Listed only once the system gives access: until then the one chosen
    // shows as itself, as on the voice page, not as missing.
    if let Some(id) = video.device.as_ref().filter(|_| cameras.is_empty()) {
        cameras.push(Device {
            id: id.clone(),
            name: id.clone(),
            default: false,
        });
    }
    let view = parts::Camera {
        devices: DeviceList {
            name: ids::CAMERAS,
            glyph: limn::Icon::Video,
            label: t("Cámara").into(),
            devices: Some(Ok(cameras)),
            chosen: video.device.clone(),
            open: ui.settings_state.select == Some(ids::CAMERAS),
        },
        frame: video.camera.live.frame(),
        testing: video.testing,
        on,
        denied: crate::video::unsupported().map(str::to_owned),
        note: if on {
            crate::video::camera_note(ui, store)
        } else {
            t("Vista previa local; no envía video. El sistema pide permiso la primera vez.")
                .to_owned()
        },
    };
    Some(parts::panel(
        t("Ajustes de cámara"),
        parts::camera(&view, look),
        BODY_HEIGHT,
        look,
    ))
}

/// The microphones or the speakers found, as the lists show them.
pub(crate) fn devices(ui: &Ui, output: bool) -> DeviceList {
    let found = ui.audio.devices.as_ref().map(|found| match found {
        Ok(found) => Ok(if output {
            &found.outputs
        } else {
            &found.inputs
        }
        .iter()
        .map(|device| Device {
            id: device.id.clone(),
            name: device.name.clone(),
            default: device.default,
        })
        .collect()),
        Err(error) => Err(error.clone()),
    });
    let (name, glyph, label, chosen) = if output {
        (
            ids::SPEAKERS,
            limn::Icon::Headphones,
            t("Altavoz"),
            &ui.audio.output,
        )
    } else {
        (
            ids::MICROPHONES,
            limn::Icon::Microphone,
            t("Micrófono"),
            &ui.audio.input,
        )
    };
    DeviceList {
        name,
        glyph,
        label: label.into(),
        devices: found,
        chosen: chosen.clone(),
        open: ui.settings_state.select == Some(name),
    }
}

/// The level heard and the threshold, as one reads them.
pub(crate) fn reading(ui: &Ui) -> Option<String> {
    let decibels = |position: f32| {
        format!(
            "{:.0}",
            position * -rune_media::FLOOR_DBFS + rune_media::FLOOR_DBFS
        )
    };
    let level = ui.audio.levels.last().copied()?;
    Some(match threshold(ui) {
        Some(threshold) => tr!(
            "{level} dBFS · umbral {threshold} dBFS",
            level = decibels(level),
            threshold = decibels(threshold)
        ),
        None => tr!("{level} dBFS", level = decibels(level)),
    })
}

/// Where voice starts on the meter; none with push-to-talk, or while
/// RNNoise decides what voice is.
pub(crate) fn threshold(ui: &Ui) -> Option<f32> {
    let prefs = &ui.prefs;
    if prefs.push_to_talk || prefs.speech_gate() {
        return None;
    }
    Some(if prefs.auto_sensitivity {
        ui.audio
            .threshold
            .unwrap_or(rune_media::VoiceActivity::default().on)
    } else {
        prefs.sensitivity
    })
}

/// The volumes are not as they start.
pub(crate) fn levels_changed(ui: &Ui) -> bool {
    ui.prefs.input_volume != 1.0 || ui.prefs.output_volume != 1.0
}

fn input(ui: &Ui) -> Input {
    let testing = ui.audio.testing && !ui.audio.starting;
    Input {
        devices: devices(ui, false),
        volume: ui.prefs.input_volume,
        most: rune_media::MAX_INPUT_VOLUME,
        testing: ui.audio.testing,
        level: ui.audio.levels.last().copied().filter(|_| testing),
        threshold: threshold(ui),
        reading: if ui.audio.starting {
            Some(t("Abriendo micrófono…").into())
        } else {
            reading(ui)
        },
        push_to_talk: ui.prefs.push_to_talk,
        suppression: ui.prefs.effective_processing().suppression != rune_media::ProcessingMode::Off,
        changed: levels_changed(ui),
    }
}

fn output(ui: &Ui, store: &Store, quick: QuickVoice, look: &Look) -> Output {
    let real = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.voice == VoiceSupport::Real);
    let in_call = store.call().is_some();
    Output {
        devices: devices(ui, true),
        volume: ui.prefs.output_volume,
        most: rune_media::MAX_VOLUME,
        note: Some(
            if in_call && !real {
                t("La voz es simulada: no se reproduce audio de la llamada. «Probar sonido» suena igual.")
            } else {
                t("«Probar sonido» toca el sonido de entrar a la llamada, también durante una.")
            }
            .into(),
        ),
        privacy: crate::desktop::privacy(ui, store, look)
            .map(|code| quick.privacy.then_some(code)),
    }
}
