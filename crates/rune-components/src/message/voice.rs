//! A voice message's player: play or pause, its loudness as bars that fill as
//! it plays and seek when dragged, the time left and the speed. Measures
//! follow the official client's player (docs/REA.md); the drawing is RUNE's own.

use limn::{Access, Element, Icon, Role};
use rune_i18n::{t, tr};
use rune_model::{Attachment, MessageId, VoiceClip};

use super::attachments::ids;
use crate::{centered, icon, line, Look};

/// Bars are 2 px wide every 6 px.
const PITCH: f32 = 6.0;
const BAR: f32 = 2.0;
const BUTTON: f32 = 32.0;
const TIME: f32 = 40.0;
const HEIGHT: f32 = 24.0;
const SPEED: f32 = 40.0;

/// The speeds a click on the player's speed goes through, from 1×.
pub const VOICE_SPEEDS: [f32; 4] = [1.0, 1.5, 2.0, 0.75];

/// `1,5×`, as the speed button reads, with the language's decimal mark.
pub fn speed_label(speed: f32) -> String {
    format!("{speed}×").replace('.', t(","))
}

/// The voice message the account is playing, as its player shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VoiceNow<'a> {
    pub attachment: rune_model::AttachmentId,
    /// Its audio is on its way.
    pub loading: bool,
    pub playing: bool,
    /// Seconds into the clip.
    pub position: f32,
    /// Why it could not play.
    pub error: Option<&'a str>,
    /// How long it lasts, once its file was read.
    pub duration: Option<f32>,
    /// Its sound's gain, from 0 to 1.
    pub volume: f32,
    /// A picture of its video is on its surface.
    pub picture: bool,
}

/// The waveform's width for a clip of `secs`: 40 px up to half a second,
/// 294 px from 45 s, in between in proportion, on the bars' pitch.
pub fn waveform_width(secs: f32) -> f32 {
    let secs = if secs.is_finite() { secs } else { 0.0 };
    let width = if secs <= 0.5 {
        40.0
    } else if secs >= 45.0 {
        294.0
    } else {
        (secs - 0.5) / 44.5 * 254.0 + 40.0
    };
    PITCH * ((width + 4.0) / PITCH).floor() - 4.0
}

/// `count` loudnesses from 0 to 1: buckets averaged when the clip has more
/// samples, zeros after them when it has fewer, all zero when unknown.
pub fn bars(waveform: &[u8], count: usize) -> Vec<f32> {
    let values: Vec<f32> = waveform.iter().map(|&v| f32::from(v) / 255.0).collect();
    if values.len() <= count {
        let mut padded = values;
        padded.resize(count, 0.0);
        return padded;
    }
    let step = values.len() as f32 / count as f32;
    let mut start = 0;
    (0..count)
        .map(|index| {
            let end = (((index + 1) as f32 * step).round() as usize).clamp(start + 1, values.len());
            let bucket = &values[start..end];
            start = end;
            bucket.iter().sum::<f32>() / bucket.len() as f32
        })
        .collect()
}

/// `m:ss`, rounded up: the whole clip before it starts, what is left after.
pub fn time_left(duration: f32, position: Option<f32>) -> String {
    if !(duration.is_finite() && duration > 0.0) {
        return "--:--".into();
    }
    let left = position.map_or(duration, |position| {
        duration - position.clamp(0.0, duration)
    });
    let seconds = left.ceil() as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub(super) fn player(
    attachment: &Attachment,
    clip: &VoiceClip,
    message: MessageId,
    width: f32,
    passive: bool,
    now: Option<VoiceNow<'_>>,
    left: Option<f32>,
    speed: f32,
    look: &Look,
) -> Element {
    let now = now.filter(|now| now.attachment == attachment.id);
    let action = |verb| ids::action(message, attachment.id, verb);
    // Its file's own length once read, which a seek also goes by; the bars
    // keep the announced one's width, so the row does not move.
    let duration = now
        .and_then(|now| now.duration)
        .unwrap_or(clip.duration_secs);
    // Not playing, it shows where the account left it.
    let started = now.map(|now| now.position).or(left);
    let progress = started.map_or(0.0, |position| (position / duration).clamp(0.0, 1.0));
    // The bars take what the button, the time and the padding leave; the
    // speed goes under them when they would not fit.
    let fixed = BUTTON + TIME + 2.0 * 8.0 + 2.0 * 10.0;
    let with_speed = width - fixed - SPEED - 10.0 >= PITCH + BAR;
    let room = (width - fixed - if with_speed { SPEED + 10.0 } else { 0.0 }).max(PITCH + BAR);
    let count = ((waveform_width(clip.duration_secs).min(room) + 4.0) / PITCH).floor() as usize;
    let lit = (progress * count as f32).round() as usize;
    let drawn: Vec<Element> = bars(&clip.waveform, count)
        .into_iter()
        .enumerate()
        .map(|(index, loudness)| {
            let color = if index < lit && started.is_some() {
                look.text
            } else {
                look.faint
            };
            Element::panel(Element::spacer())
                .size(Some(BAR), Some(22.0 * loudness + 2.0))
                .background(color)
                .radius(BAR / 2.0)
                .centered_y()
        })
        .collect();
    let wave_width = count as f32 * PITCH - (PITCH - BAR);
    let wave = Element::row(drawn)
        .gap(PITCH - BAR)
        .size(Some(wave_width), Some(HEIGHT));
    let time = time_left(duration, started);
    let playing = now.is_some_and(|now| now.playing);
    let loading = now.is_some_and(|now| now.loading);
    let (glyph, name) = if loading {
        (Icon::PlayFill, t("Cargando mensaje de voz"))
    } else if playing {
        (Icon::PauseFill, t("Pausar mensaje de voz"))
    } else {
        (Icon::PlayFill, t("Reproducir mensaje de voz"))
    };
    let button = Element::panel(centered(icon(glyph, look.on_accent)))
        .size(Some(BUTTON), Some(BUTTON))
        .background(if loading { look.faint } else { look.accent })
        .radius(BUTTON / 2.0);
    let (button, wave) = if passive {
        (button, wave)
    } else {
        (
            Element::clickable(action("voice"), button)
                .accessible(Access::new(Role::Button).name(name)),
            Element::clickable(action("seek"), wave)
                .slider(progress, 0, 0.0)
                .size(Some(wave_width), Some(HEIGHT))
                .accessible(
                    Access::new(Role::Slider)
                        .id(action("seek"))
                        .name(t("Posición del mensaje de voz"))
                        .value(progress)
                        .value_text(tr!("Quedan {time}", time)),
                ),
        )
    };
    let mut parts = vec![
        button,
        wave.centered_y(),
        line(time, 12.0, look.muted)
            .size(Some(TIME), None)
            .centered_y(),
    ];
    let label = speed_label(speed);
    let chip = Element::panel(centered(crate::strong_line(label.clone(), 12.0, look.text)))
        .size(Some(SPEED), Some(HEIGHT))
        .background(look.hover)
        .radius(HEIGHT / 2.0);
    let chip = if passive {
        chip
    } else {
        Element::clickable(action("speed"), chip)
            .accessible(Access::new(Role::Button).name(tr!("Velocidad {label}", label)))
    };
    let under = if with_speed {
        parts.push(chip.centered_y());
        None
    } else {
        Some(chip)
    };
    let pill = Element::row(parts)
        .gap(10.0)
        .padding(8.0)
        .background(look.card)
        .border(look.border, 1.0)
        .radius(24.0)
        .accessible(Access::new(Role::Group).name(t("Mensaje de voz")));
    let error = now.and_then(|now| now.error).map(|error| {
        line(
            tr!("No se pudo reproducir: {error}", error),
            12.0,
            look.danger,
        )
        .size(Some(width.max(0.0)), None)
    });
    if under.is_none() && error.is_none() {
        return pill;
    }
    Element::column([Some(pill), under, error].into_iter().flatten().collect()).gap(4.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::{DrawCommand, Size};
    use rune_model::AttachmentId;

    #[test]
    fn its_width_follows_the_length_on_the_bars_pitch() {
        assert_eq!(waveform_width(0.2), 38.0);
        assert_eq!(waveform_width(45.0), 290.0);
        assert_eq!(waveform_width(600.0), 290.0);
        assert_eq!(waveform_width(f32::NAN), 38.0);
        let middle = waveform_width(10.0);
        assert!(middle > 38.0 && middle < 290.0);
        assert_eq!((middle + 4.0) % PITCH, 0.0);
    }

    #[test]
    fn loudness_is_averaged_padded_or_flat() {
        assert_eq!(bars(&[255, 0, 255, 0], 2), [0.5, 0.5]);
        assert_eq!(bars(&[255], 3), [1.0, 0.0, 0.0]);
        assert_eq!(bars(&[], 4), [0.0; 4]);
        let many: Vec<u8> = (0..=255).collect();
        assert_eq!(bars(&many, 7).len(), 7);
    }

    #[test]
    fn the_time_is_the_whole_clip_until_it_starts_then_what_is_left() {
        assert_eq!(time_left(4.2, None), "0:05");
        assert_eq!(time_left(75.0, Some(10.5)), "1:05");
        assert_eq!(time_left(4.0, Some(9.0)), "0:00");
        assert_eq!(time_left(0.0, None), "--:--");
    }

    fn clip() -> (Attachment, VoiceClip) {
        let clip = VoiceClip {
            duration_secs: 12.0,
            waveform: vec![40, 200, 120, 255, 10],
        };
        let attachment = Attachment {
            id: AttachmentId(7),
            filename: "voice-message.ogg".into(),
            size_bytes: 9_000,
            content_type: Some("audio/ogg".into()),
            dimensions: None,
            preview: None,
            download: None,
            spoiler: false,
            voice: Some(clip.clone()),
            explicit: false,
            placeholder: None,
        };
        (attachment, clip)
    }

    #[test]
    fn it_plays_pauses_and_seeks_by_ids_of_the_message_and_file() {
        let (attachment, clip) = clip();
        let laid_left = |now, left| {
            frame(
                player(
                    &attachment,
                    &clip,
                    MessageId(3),
                    360.0,
                    false,
                    now,
                    left,
                    1.5,
                    &Look::DARK,
                ),
                Size::new(360.0, 80.0),
            )
            .1
        };
        let laid = |now| laid_left(now, None);
        let idle = laid(None);
        assert!(controls(&idle).contains(&ids::action(MessageId(3), AttachmentId(7), "voice")));
        assert!(idle
            .regions
            .iter()
            .any(|region| region.id == ids::action(MessageId(3), AttachmentId(7), "seek")));
        assert!(texts(&idle).contains(&"0:12".to_owned()));
        assert!(controls(&idle).contains(&ids::action(MessageId(3), AttachmentId(7), "speed")));
        assert!(texts(&idle).contains(&"1,5×".to_owned()));
        let playing = laid(Some(VoiceNow {
            attachment: AttachmentId(7),
            playing: true,
            position: 6.0,
            ..VoiceNow::default()
        }));
        assert!(texts(&playing).contains(&"0:06".to_owned()));
        let bright = |frame: &limn::Frame| {
            frame
                .commands
                .iter()
                .filter(|command| {
                    matches!(command, DrawCommand::FillRect { color, .. } if *color == Look::DARK.text)
                })
                .count()
        };
        assert_eq!(bright(&idle), 0);
        assert!(bright(&playing) > 0, "the played half is lit");
        let failed = laid(Some(VoiceNow {
            attachment: AttachmentId(7),
            error: Some("sin red"),
            ..VoiceNow::default()
        }));
        assert!(texts(&failed).contains(&"No se pudo reproducir: sin red".to_owned()));
        // Another clip's state is not this one's.
        let other = laid(Some(VoiceNow {
            attachment: AttachmentId(8),
            playing: true,
            position: 6.0,
            ..VoiceNow::default()
        }));
        assert!(texts(&other).contains(&"0:12".to_owned()));
        // Left halfway and not playing: it shows that place, lit, until it
        // plays again; playing, its own place wins.
        let left = laid_left(None, Some(6.0));
        assert!(texts(&left).contains(&"0:06".to_owned()));
        assert_eq!(bright(&left), bright(&playing));
        let again = laid_left(
            Some(VoiceNow {
                attachment: AttachmentId(7),
                playing: true,
                position: 9.0,
                ..VoiceNow::default()
            }),
            Some(6.0),
        );
        assert!(texts(&again).contains(&"0:03".to_owned()));
        // Its file lasts 10 s, not the 12 announced: the time left and the
        // lit bars go by it, as a seek does.
        let read = |position| {
            laid(Some(VoiceNow {
                attachment: AttachmentId(7),
                playing: true,
                position,
                duration: Some(10.0),
                ..VoiceNow::default()
            }))
        };
        assert!(texts(&read(6.0)).contains(&"0:04".to_owned()));
        assert_eq!(bright(&read(5.0)), bright(&playing), "half of it");
    }

    #[test]
    fn the_speed_reads_as_the_client_writes_it() {
        let labels: Vec<String> = VOICE_SPEEDS.into_iter().map(speed_label).collect();
        assert_eq!(labels, ["1×", "1,5×", "2×", "0,75×"]);
    }

    #[test]
    fn a_narrow_row_keeps_the_player_and_its_speed_inside_it() {
        let (attachment, clip) = clip();
        let speed = ids::action(MessageId(3), AttachmentId(7), "speed");
        for width in [120.0, 165.0, 200.0, 360.0] {
            let (_, laid) = frame(
                player(
                    &attachment,
                    &clip,
                    MessageId(3),
                    width,
                    false,
                    None,
                    None,
                    1.0,
                    &Look::DARK,
                ),
                Size::new(width, 120.0),
            );
            for region in &laid.regions {
                assert!(region.rect.x + region.rect.width <= width + 0.5, "{width}");
            }
            assert!(controls(&laid).contains(&speed), "{width}");
            let button = |id: &str| {
                laid.regions
                    .iter()
                    .find(|region| region.id == id)
                    .unwrap()
                    .rect
            };
            let (chip, play) = (
                button(&speed),
                button(&ids::action(MessageId(3), AttachmentId(7), "voice")),
            );
            // Beside the time while it fits, under the player when not.
            assert_eq!(chip.y > play.y + play.height, width < 166.0, "{width}");
        }
    }
}
