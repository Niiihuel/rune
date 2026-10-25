//! The menus of the arrows beside the microphone and the headphones, in the
//! account's panel or the call's bar, as Discord has them (2026-10-25), and
//! the camera's panel of the bar (2026-10-18). Their controls are the voice
//! page's (`us:…`), so they share their state; this opens and closes them,
//! and fills them.
//!
//! The devices are listed once when one opens, without opening them. The
//! microphone's level shows only in a call, while the call holds it open;
//! closing the menu stops what it measured.

use std::time::Instant;

use limn::{Element, UiAction};
use rune_components::profile::{ids as profile_ids, Device};
use rune_components::quick_voice::{self as parts, ids, AudioMenu, DeviceList};
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
        // A device or a profile chosen in a menu closes it, as in Discord.
        chosen
            if ui.profiles.devices.is_some_and(|quick| !quick.camera)
                && [
                    "us:mic=",
                    "us:mic-default",
                    "us:speaker=",
                    "us:speaker-default",
                    "us:profile=",
                ]
                .iter()
                .any(|prefix| chosen.starts_with(prefix)) =>
        {
            let mut intents = close(ui);
            intents.extend(settings::clicked(ui, store, chosen, now));
            return Some(intents);
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

/// The menu open from the call's bar (`bar`) or from the account's panel,
/// with whether it is the headphones'.
pub(crate) fn view(ui: &Ui, store: &Store, look: &Look, bar: bool) -> Option<(bool, Element)> {
    let quick = ui
        .profiles
        .devices
        .filter(|quick| quick.bar == bar && !quick.camera)?;
    let output = quick.output;
    let prefs = &ui.prefs;
    let menu = AudioMenu {
        output,
        devices: devices(ui, output),
        volume: if output {
            prefs.output_volume
        } else {
            prefs.input_volume
        },
        most: if output {
            rune_media::MAX_VOLUME
        } else {
            rune_media::MAX_INPUT_VOLUME
        },
        profile: (!output).then(|| prefs.input_profile.key()),
        level: (!output && crate::video::in_call(store)).then(|| {
            let heard = ui.audio.testing && !ui.audio.starting;
            ui.audio.levels.last().copied().filter(|_| heard)
        }),
        push_to_talk: (!output && prefs.push_to_talk_key.is_some()).then_some(prefs.push_to_talk),
    };
    Some((output, parts::audio_menu(&menu, look)))
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
        // A saved virtual node is the system default, which is offered first.
        chosen: chosen
            .clone()
            .filter(|id| !rune_media::is_virtual_device(id)),
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
