//! An audio or video file's player: play or pause, a bar of its time that
//! seeks on release, the time, the volume and, for a video, its pictures and
//! the full-window view. A file RUNE does not play says why and offers to
//! open it in the browser or save it.

use limn::{Access, Button, ButtonVariant, Component, Element, Icon, IconButton, Role, Slider};
use rune_i18n::{es, t, tr};
use rune_model::{Attachment, AttachmentId, MessageId};

use super::attachments::ids;
use super::VoiceNow;
use crate::{centered, file_size, icon, line, Look};

const BUTTON: f32 = 32.0;
const HEIGHT: f32 = 32.0;
/// The box a video fits in, as an attachment's preview.
const VIDEO: (f32, f32) = (400.0, 300.0);
/// The widest an audio player gets.
const AUDIO_WIDTH: f32 = 400.0;
const VOLUME_WIDTH: f32 = 64.0;

/// How a file plays in RUNE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Playable {
    Audio,
    Video,
    /// A video RUNE does not play, and why.
    Not(&'static str),
}

/// The surface a host presents a playing video's pictures on.
pub fn media_surface(attachment: AttachmentId) -> String {
    format!("media:{attachment}")
}

/// Whether `attachment` plays in RUNE, by its type or else its name: the
/// sound files and MP4, WebM and MKV videos Discord carries; whether a
/// video's codec plays is known once its file is read. A voice message has its own
/// player; other files keep their card.
pub fn playable(attachment: &Attachment) -> Option<Playable> {
    let kind = format(attachment)?;
    if kind != Playable::Audio && kind != Playable::Video {
        return Some(kind);
    }
    if attachment.size_bytes > Attachment::MAX_PLAYED_BYTES {
        return Some(Playable::Not(es(
            "Supera los 50 MiB que RUNE reproduce dentro de la conversación.",
        )));
    }
    Some(kind)
}

/// Whether `attachment` is a video, played here or not, whatever its size.
pub fn is_video(attachment: &Attachment) -> bool {
    matches!(format(attachment), Some(Playable::Video | Playable::Not(_)))
}

/// How `attachment` plays by its type or name alone; a `Not` is a video.
fn format(attachment: &Attachment) -> Option<Playable> {
    if attachment.voice.is_some() {
        return None;
    }
    let mime = attachment
        .content_type
        .as_deref()
        .and_then(|mime| mime.split(';').next())
        .map(|mime| mime.trim().to_ascii_lowercase());
    let extension = attachment
        .filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    let named = |names: &[&str]| {
        extension
            .as_deref()
            .is_some_and(|found| names.contains(&found))
    };
    let typed = |types: &[&str]| mime.as_deref().is_some_and(|found| types.contains(&found));
    let kind = if typed(&[
        "audio/mpeg",
        "audio/mp3",
        "audio/ogg",
        "audio/opus",
        "audio/mp4",
        "audio/x-m4a",
        "audio/aac",
        "audio/wav",
        "audio/x-wav",
        "audio/wave",
        "audio/vnd.wave",
        "audio/flac",
        "audio/x-flac",
        "audio/webm",
    ]) || mime.is_none()
        && named(&[
            "mp3", "ogg", "oga", "opus", "m4a", "aac", "wav", "flac", "weba",
        ]) {
        Playable::Audio
    } else if typed(&[
        "video/mp4",
        "video/quicktime",
        "video/x-m4v",
        "video/webm",
        "video/x-matroska",
    ]) || mime.is_none() && named(&["mp4", "m4v", "mov", "webm", "mkv"])
    {
        Playable::Video
    } else if mime
        .as_deref()
        .is_some_and(|mime| mime.starts_with("video/"))
    {
        Playable::Not(es("RUNE no reproduce este formato de video."))
    } else {
        return None;
    };
    Some(kind)
}

/// `m:ss` of `seconds`, or `--:--` when unknown.
fn clock(seconds: Option<f32>) -> String {
    match seconds.filter(|seconds| seconds.is_finite() && *seconds >= 0.0) {
        Some(seconds) => {
            let whole = seconds.floor() as u64;
            format!("{}:{:02}", whole / 60, whole % 60)
        }
        None => "--:--".into(),
    }
}

/// The row of a player: play or pause, its time as a bar and as text, the
/// volume, and, for a video, the full-window view. `width` is all it has.
pub fn controls(
    attachment: &Attachment,
    message: MessageId,
    now: Option<VoiceNow<'_>>,
    video: bool,
    fullscreen: bool,
    width: f32,
    look: &Look,
) -> Element {
    let action = |verb| ids::action(message, attachment.id, verb);
    let playing = now.is_some_and(|now| now.playing);
    let loading = now.is_some_and(|now| now.loading);
    let duration = now.and_then(|now| now.duration);
    let position = now.map_or(0.0, |now| now.position);
    let progress = duration
        .filter(|duration| *duration > 0.0)
        .map_or(0.0, |duration| (position / duration).clamp(0.0, 1.0));
    let volume = now.map_or(1.0, |now| now.volume);
    let (glyph, name) = if loading {
        (Icon::PlayFill, t("Cargando"))
    } else if playing {
        (Icon::PauseFill, t("Pausar"))
    } else {
        (Icon::PlayFill, t("Reproducir"))
    };
    let play = Element::clickable(
        action("play"),
        Element::panel(centered(icon(glyph, look.on_accent)))
            .size(Some(BUTTON), Some(BUTTON))
            .background(if loading { look.faint } else { look.accent })
            .radius(BUTTON / 2.0),
    )
    .accessible(Access::new(Role::Button).name(name));
    let time = format!("{} / {}", clock(Some(position)), clock(duration));
    let time_width = 84.0;
    let mute = IconButton::new(
        action("mute"),
        if volume == 0.0 {
            Icon::VolumeMuted
        } else {
            Icon::Volume
        },
        look.palette(),
    )
    .tooltip(if volume == 0.0 {
        t("Activar el sonido")
    } else {
        t("Silenciar")
    })
    .variant(ButtonVariant::Subtle)
    .render();
    let screen = video.then(|| {
        IconButton::new(
            action("fullscreen"),
            if fullscreen {
                Icon::Collapse
            } else {
                Icon::Fullscreen
            },
            look.palette(),
        )
        .tooltip(if fullscreen {
            t("Salir de pantalla completa")
        } else {
            t("Pantalla completa")
        })
        .variant(ButtonVariant::Subtle)
        .render()
    });
    // What the buttons and the time take; the bars share what is left, the
    // volume's first to go when narrow.
    let fixed = BUTTON + time_width + 32.0 + if video { 32.0 } else { 0.0 } + 5.0 * 8.0;
    let with_volume = width - fixed - VOLUME_WIDTH - 8.0 >= 60.0;
    let seek_width = (width - fixed - if with_volume { VOLUME_WIDTH + 8.0 } else { 0.0 }).max(40.0);
    let seek = Slider::new(action("seek"), progress, look.palette())
        .width(seek_width)
        .enabled(now.is_some() && duration.is_some())
        .accessibility_label(t("Posición"))
        .render();
    let mut row = vec![
        play.centered_y(),
        seek.centered_y(),
        line(time, 12.0, look.muted)
            .size(Some(time_width), None)
            .centered_y(),
        mute.centered_y(),
    ];
    if with_volume {
        row.push(
            Slider::new(action("volume"), volume, look.palette())
                .width(VOLUME_WIDTH)
                .accessibility_label(t("Volumen"))
                .render()
                .centered_y(),
        );
    }
    row.extend(screen.map(Element::centered_y));
    Element::row(row)
        .gap(8.0)
        .size(Some(width.max(0.0)), Some(HEIGHT + 8.0))
}

/// A file's name and size, beside the mark of its kind.
fn heading(attachment: &Attachment, mark: Icon, width: f32, look: &Look) -> Element {
    Element::row(vec![
        icon(mark, look.muted).centered_y(),
        line(&attachment.filename, 14.0, look.text)
            .flex(1.0)
            .centered_y(),
        line(file_size(attachment.size_bytes), 12.0, look.faint).centered_y(),
    ])
    .gap(8.0)
    .size(Some(width), None)
}

/// Why a file does not play here, with the browser and «Guardar como…».
fn elsewhere(
    attachment: &Attachment,
    message: MessageId,
    reason: &str,
    width: f32,
    passive: bool,
    look: &Look,
) -> Element {
    let action = |verb| ids::action(message, attachment.id, verb);
    let mut parts = vec![
        heading(attachment, Icon::Video, width - 24.0, look),
        line(reason, 12.0, look.muted).size(Some(width - 24.0), None),
    ];
    if attachment.download.is_some() && !passive {
        parts.push(
            Element::row(vec![
                Button::new(action("open"), t("Abrir en el navegador"), look.palette()).render(),
                Button::new(action("save-as"), t("Guardar como…"), look.palette()).render(),
                Element::spacer().flex(1.0),
            ])
            .gap(8.0)
            .wrap(),
        );
    }
    Element::panel(Element::column(parts).gap(8.0))
        .padding(12.0)
        .background(look.card)
        .border(look.border, 1.0)
        .radius(8.0)
        .size(Some(width), None)
        .accessible(Access::new(Role::Group).name(&attachment.filename))
}

/// The player of an audio or video file in the timeline, `width` at most;
/// `now` is the clip playing, when it is this one.
pub(super) fn player(
    attachment: &Attachment,
    kind: Playable,
    message: MessageId,
    width: f32,
    passive: bool,
    now: Option<VoiceNow<'_>>,
    look: &Look,
) -> Element {
    let now = now.filter(|now| now.attachment == attachment.id);
    let video = match kind {
        Playable::Not(reason) => {
            return elsewhere(
                attachment,
                message,
                t(reason),
                width.min(AUDIO_WIDTH),
                passive,
                look,
            )
        }
        Playable::Audio => false,
        Playable::Video => true,
    };
    if let Some(error) = now.and_then(|now| now.error) {
        let reason = tr!("No se pudo reproducir: {error}", error);
        return elsewhere(
            attachment,
            message,
            &reason,
            width.min(AUDIO_WIDTH),
            passive,
            look,
        );
    }
    let action = |verb| ids::action(message, attachment.id, verb);
    if !video {
        let width = width.min(AUDIO_WIDTH);
        let mut parts = vec![heading(attachment, Icon::Audio, width - 24.0, look)];
        if !passive {
            parts.push(controls(
                attachment,
                message,
                now,
                false,
                false,
                width - 24.0,
                look,
            ));
        }
        return Element::panel(Element::column(parts).gap(6.0))
            .padding(12.0)
            .background(look.card)
            .border(look.border, 1.0)
            .radius(8.0)
            .size(Some(width), None)
            .accessible(
                Access::new(Role::Group).name(tr!("Audio: {name}", name = attachment.filename)),
            );
    }
    let (w, h) = attachment
        .dimensions
        .filter(|(w, h)| *w > 0 && *h > 0)
        .unwrap_or((16, 9));
    let fit = (width.min(VIDEO.0) / w as f32).min(VIDEO.1 / h as f32);
    let (w, h) = (
        (w as f32 * fit).max(160.0).min(width),
        (h as f32 * fit).max(90.0),
    );
    let picture = if now.is_some_and(|now| now.picture) {
        Element::surface(media_surface(attachment.id), &attachment.filename)
            .surface_fit(limn::SurfaceFit::Contain)
    } else {
        // Before it plays: a mark to start it, and its name.
        let start = Element::panel(centered(icon(Icon::PlayFill, look.text).icon_size(24.0)))
            .size(Some(56.0), Some(56.0))
            .radius(28.0)
            .background(Look::alpha(look.canvas, 200));
        let start = if passive {
            start
        } else {
            Element::clickable(action("play"), start).accessible(
                Access::new(Role::Button)
                    .name(tr!("Reproducir {name}", name = attachment.filename)),
            )
        };
        Element::column(vec![
            Element::spacer().flex(1.0),
            Element::row(vec![
                Element::spacer().flex(1.0),
                start,
                Element::spacer().flex(1.0),
            ]),
            Element::spacer().flex(1.0),
        ])
    }
    .size(Some(w), Some(h))
    .background(limn::Color::rgb(0, 0, 0))
    .radius(8.0)
    .clip_children()
    .accessible(Access::new(Role::Image).name(tr!("Video: {name}", name = attachment.filename)));
    let mut parts = vec![picture, heading(attachment, Icon::Video, w, look)];
    if !passive {
        parts.push(controls(attachment, message, now, true, false, w, look));
    }
    Element::column(parts).gap(6.0).size(Some(w), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls as ids_of, frame, texts};
    use limn::Size;

    fn file(name: &str, mime: Option<&str>, size: u64) -> Attachment {
        Attachment {
            id: AttachmentId(9),
            filename: name.into(),
            size_bytes: size,
            content_type: mime.map(str::to_owned),
            dimensions: Some((1280, 720)),
            preview: None,
            download: Some(rune_model::RemoteFile {
                url: "https://cdn.example.test/a".into(),
            }),
            spoiler: false,
            voice: None,
            explicit: false,
            placeholder: None,
        }
    }

    #[test]
    fn files_play_by_their_type_or_their_name() {
        let audio = [
            ("song.mp3", None),
            ("x.bin", Some("audio/mpeg")),
            ("nota.m4a", None),
            ("a.flac", Some("audio/flac")),
            ("a.ogg", Some("audio/ogg; codecs=vorbis")),
            ("a.opus", None),
            ("a.wav", Some("audio/x-wav")),
        ];
        for (name, mime) in audio {
            assert_eq!(
                playable(&file(name, mime, 1)),
                Some(Playable::Audio),
                "{name}"
            );
        }
        assert_eq!(
            playable(&file("clip.mp4", Some("video/mp4"), 1)),
            Some(Playable::Video)
        );
        assert_eq!(playable(&file("clip.MOV", None, 1)), Some(Playable::Video));
        assert_eq!(
            playable(&file("clip.webm", Some("video/webm"), 1)),
            Some(Playable::Video)
        );
        assert_eq!(playable(&file("game.MKV", None, 1)), Some(Playable::Video));
        assert!(matches!(
            playable(&file("clip.avi", Some("video/x-msvideo"), 1)),
            Some(Playable::Not(_))
        ));
        assert!(matches!(
            playable(&file("big.mp4", Some("video/mp4"), Attachment::MAX_PLAYED_BYTES + 1)),
            Some(Playable::Not(reason)) if reason.contains("50 MiB")
        ));
        assert_eq!(
            playable(&file("notes.pdf", Some("application/pdf"), 1)),
            None
        );
        assert_eq!(playable(&file("mystery.mp3", Some("text/plain"), 1)), None);
    }

    #[test]
    fn a_player_plays_seeks_and_sets_its_volume_by_ids_of_the_message_and_file() {
        let audio = file("ensayo.mp3", Some("audio/mpeg"), 4_000);
        let action = |verb| ids::action(MessageId(5), AttachmentId(9), verb);
        let laid = |now| {
            frame(
                player(
                    &audio,
                    Playable::Audio,
                    MessageId(5),
                    400.0,
                    false,
                    now,
                    &Look::DARK,
                ),
                Size::new(400.0, 120.0),
            )
            .1
        };
        let idle = laid(None);
        assert!(ids_of(&idle).contains(&action("play")));
        assert!(texts(&idle).contains(&"0:00 / --:--".to_owned()));
        let playing = laid(Some(VoiceNow {
            attachment: AttachmentId(9),
            playing: true,
            position: 61.0,
            duration: Some(125.0),
            volume: 0.5,
            ..VoiceNow::default()
        }));
        assert!(texts(&playing).contains(&"1:01 / 2:05".to_owned()));
        for verb in ["seek", "volume", "mute"] {
            assert!(
                playing
                    .regions
                    .iter()
                    .any(|region| region.id == action(verb)),
                "{verb}"
            );
        }
        assert!(!playing
            .regions
            .iter()
            .any(|region| region.id == action("fullscreen")));
    }

    #[test]
    fn a_video_shows_its_pictures_once_presented_and_offers_the_full_window() {
        let video = file("demo.mp4", Some("video/mp4"), 4_000);
        let action = |verb| ids::action(MessageId(5), AttachmentId(9), verb);
        let laid = |now| {
            frame(
                player(
                    &video,
                    Playable::Video,
                    MessageId(5),
                    500.0,
                    false,
                    now,
                    &Look::DARK,
                ),
                Size::new(500.0, 400.0),
            )
            .1
        };
        let surface = |frame: &limn::Frame| {
            frame.commands.iter().any(|command| {
                matches!(command, limn::DrawCommand::Surface { id, .. } if *id == media_surface(AttachmentId(9)))
            })
        };
        let idle = laid(None);
        assert!(!surface(&idle));
        assert!(ids_of(&idle).contains(&action("play")));
        let shown = laid(Some(VoiceNow {
            attachment: AttachmentId(9),
            playing: true,
            picture: true,
            duration: Some(4.0),
            volume: 1.0,
            ..VoiceNow::default()
        }));
        assert!(surface(&shown));
        assert!(shown
            .regions
            .iter()
            .any(|region| region.id == action("fullscreen")));
    }

    #[test]
    fn a_file_rune_does_not_play_says_why_and_opens_elsewhere() {
        let avi = file("clip.avi", Some("video/x-msvideo"), 4_000);
        let kind = playable(&avi).unwrap();
        let (_, laid) = frame(
            player(&avi, kind, MessageId(5), 400.0, false, None, &Look::DARK),
            Size::new(400.0, 200.0),
        );
        assert!(texts(&laid)
            .iter()
            .any(|text| text.contains("formato de video")));
        let action = |verb| ids::action(MessageId(5), AttachmentId(9), verb);
        assert!(ids_of(&laid).contains(&action("open")));
        assert!(ids_of(&laid).contains(&action("save-as")));
        // A failed one too, with its reason.
        let mp4 = file("demo.mp4", Some("video/mp4"), 4_000);
        let (_, failed) = frame(
            player(
                &mp4,
                Playable::Video,
                MessageId(5),
                400.0,
                false,
                Some(VoiceNow {
                    attachment: AttachmentId(9),
                    error: Some("Este video usa H.265 (HEVC), que RUNE no reproduce."),
                    ..VoiceNow::default()
                }),
                &Look::DARK,
            ),
            Size::new(400.0, 200.0),
        );
        assert!(texts(&failed).iter().any(|text| text.contains("H.265")));
        assert!(ids_of(&failed).contains(&action("open")));
    }
}
