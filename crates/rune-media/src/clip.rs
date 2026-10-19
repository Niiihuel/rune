//! One voice message, audio file or video at a time: its sound read and
//! decoded on its own thread and paced into the output's [`Feed`] by the
//! clock, at its speed and volume, and its pictures (H264 in MP4; VP8, VP9,
//! AV1 or H264 in WebM and MKV) decoded on another that follows that clock,
//! so the interface only flips flags, reads a position and takes the newest
//! picture.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use rune_i18n::{t, tr};

use crate::stretch::Stretch;
use crate::{Feed, MediaError, PLAYBACK_RATE};

/// Any other sound file, through Symphonia.
mod file;
/// Voice messages: Ogg Opus, through RUNE's reader and libopus.
mod ogg;
/// The pictures of a video, through OpenH264.
#[cfg(target_os = "linux")]
mod pictures;
/// The pictures of a WebM or MKV video, through GStreamer.
#[cfg(target_os = "linux")]
mod stream;

/// Audio queued ahead of the clock: 60 ms, under the output's feed backlog,
/// so none of it is dropped.
const LEAD: u64 = PLAYBACK_RATE as u64 * 60 / 1_000;
const TICK: Duration = Duration::from_millis(10);
/// Far more than any Opus packet; a bigger one is not a voice message.
const MAX_PACKET_BYTES: usize = 64 * 1024;
const NO_SEEK: u64 = u64::MAX;
/// The longest sound played: a file claiming more is refused.
const MAX_SECONDS: f64 = 4.0 * 60.0 * 60.0;

fn unreadable() -> MediaError {
    MediaError(t("RUNE no pudo leer este archivo.").into())
}

fn invalid() -> MediaError {
    MediaError(t("El mensaje de voz no es un audio Ogg Opus válido.").into())
}

/// Refuse a file whose sound lasts `samples`, when that is past [`MAX_SECONDS`].
fn too_long(samples: u64) -> Result<(), MediaError> {
    if samples as f64 > MAX_SECONDS * f64::from(PLAYBACK_RATE) {
        return Err(MediaError(
            t("El archivo dura más de lo que RUNE reproduce.").into(),
        ));
    }
    Ok(())
}

/// Where a [`Clip`] is.
#[derive(Clone, Debug, PartialEq)]
pub enum ClipStatus {
    /// Playing or paused.
    Open,
    /// It played to its end, or its output went away.
    Ended,
    Failed(String),
}

struct Shared {
    stop: AtomicBool,
    paused: AtomicBool,
    /// Samples into the clip to jump to, or [`NO_SEEK`].
    seek: AtomicU64,
    /// Jumps made, so the pictures start again from the new place.
    seeks: AtomicU64,
    /// Samples into the clip heard so far.
    position: AtomicU64,
    /// The speed, as `f32` bits.
    speed: AtomicU32,
    /// The gain applied to its sound, as `f32` bits.
    volume: AtomicU32,
    /// Its length in samples once its file was read; 0 while unknown.
    length: AtomicU64,
    ended: AtomicBool,
    failed: Mutex<Option<String>>,
    /// The newest picture of a video, until the host takes it.
    #[cfg(target_os = "linux")]
    picture: Mutex<Option<crate::video::Frame>>,
    /// It has pictures to show.
    video: AtomicBool,
}

impl Shared {
    fn fail(&self, error: MediaError) {
        *self
            .failed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error.0);
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
}

fn samples(secs: f32) -> u64 {
    if secs.is_finite() {
        (secs.max(0.0) as f64 * f64::from(PLAYBACK_RATE)) as u64
    } else {
        0
    }
}

/// One clip playing into a [`Feed`]. It starts paused; dropping it stops it
/// without joining its threads.
pub struct Clip {
    shared: Arc<Shared>,
    worker: Thread,
}

impl Clip {
    /// Play `bytes` into `feed` from `from_secs` once unpaused: an Ogg Opus
    /// voice message, a sound file (MP3, M4A/AAC, Ogg Vorbis or Opus, WAV,
    /// FLAC, WebM sound), an MP4 video with H264 pictures or a WebM or MKV
    /// one with VP8, VP9, AV1 or H264 pictures. Reading it
    /// happens on its thread: a file it cannot play ends in
    /// [`ClipStatus::Failed`] with the reason.
    pub fn start(bytes: Arc<[u8]>, feed: Feed, from_secs: f32) -> Result<Self, MediaError> {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(true),
            seek: AtomicU64::new(samples(from_secs)),
            seeks: AtomicU64::new(0),
            position: AtomicU64::new(samples(from_secs)),
            speed: AtomicU32::new(1.0_f32.to_bits()),
            volume: AtomicU32::new(1.0_f32.to_bits()),
            length: AtomicU64::new(0),
            ended: AtomicBool::new(false),
            failed: Mutex::new(None),
            #[cfg(target_os = "linux")]
            picture: Mutex::new(None),
            video: AtomicBool::new(false),
        });
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("rune-clip".into())
            .spawn(move || {
                // A decoder that panics on a strange file ends this clip, not RUNE.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    open(&bytes, &worker_shared)
                        .and_then(|mut sound| run(sound.as_mut(), feed, &worker_shared))
                }))
                .unwrap_or_else(|_| Err(unreadable()));
                if let Err(error) = result {
                    worker_shared.fail(error);
                }
            })
            .map_err(|_| MediaError(t("No se pudo reproducir el audio.").into()))?
            .thread()
            .clone();
        Ok(Self { shared, worker })
    }

    pub fn set_paused(&self, paused: bool) {
        if self.shared.paused.swap(paused, Ordering::AcqRel) != paused {
            self.worker.unpark();
        }
    }

    /// Continue from `secs` into the clip.
    pub fn seek(&self, secs: f32) {
        self.shared.seek.store(samples(secs), Ordering::Release);
        self.shared.position.store(samples(secs), Ordering::Release);
        self.shared.seeks.fetch_add(1, Ordering::AcqRel);
        self.worker.unpark();
    }

    /// Play at `speed` times, from 0.5 to 2, with the same pitch.
    pub fn set_speed(&self, speed: f32) {
        let speed = if speed.is_finite() {
            speed.clamp(0.5, 2.0)
        } else {
            1.0
        };
        self.shared.speed.store(speed.to_bits(), Ordering::Release);
    }

    /// Its sound's gain, from 0 (silent) to 1 (as recorded).
    pub fn set_volume(&self, volume: f32) {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.shared
            .volume
            .store(volume.to_bits(), Ordering::Release);
    }

    /// Seconds into the clip heard so far.
    pub fn position(&self) -> f32 {
        self.shared.position.load(Ordering::Acquire) as f32 / PLAYBACK_RATE as f32
    }

    /// How long it lasts, once its file was read.
    pub fn duration(&self) -> Option<f32> {
        let length = self.shared.length.load(Ordering::Acquire);
        (length > 0).then(|| length as f32 / PLAYBACK_RATE as f32)
    }

    /// Whether it has pictures to show: a video.
    pub fn has_pictures(&self) -> bool {
        self.shared.video.load(Ordering::Acquire)
    }

    /// The newest picture since the last call, straight RGBA within 1920×1080.
    #[cfg(target_os = "linux")]
    pub fn take_picture(&self) -> Option<crate::video::Frame> {
        self.shared
            .picture
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    pub fn status(&self) -> ClipStatus {
        if let Some(error) = self
            .shared
            .failed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
        {
            ClipStatus::Failed(error)
        } else if self.shared.ended.load(Ordering::Acquire) {
            ClipStatus::Ended
        } else {
            ClipStatus::Open
        }
    }
}

impl Drop for Clip {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.worker.unpark();
    }
}

/// Where sound comes from, mono at [`PLAYBACK_RATE`].
trait Sound: Send {
    /// Append the next stretch of sound to `out`; `false` once it ended.
    fn next(&mut self, out: &mut Vec<f32>) -> Result<bool, MediaError>;
    /// Continue from `samples` into it.
    fn seek(&mut self, samples: u64) -> Result<(), MediaError>;
    /// Its length in samples, when known.
    fn length(&self) -> Option<u64>;
}

/// Silence as long as a video without sound, so its pictures have a clock.
struct Silence {
    length: u64,
    at: u64,
}

impl Sound for Silence {
    fn next(&mut self, out: &mut Vec<f32>) -> Result<bool, MediaError> {
        let take = (self.length - self.at).min(PLAYBACK_RATE as u64 / 50);
        self.at += take;
        out.resize(out.len() + take as usize, 0.0);
        Ok(take > 0)
    }

    fn seek(&mut self, samples: u64) -> Result<(), MediaError> {
        self.at = samples.min(self.length);
        Ok(())
    }

    fn length(&self) -> Option<u64> {
        Some(self.length)
    }
}

/// How long the sound of `bytes` lasts, decoding it whole, or `None` once
/// it passes `limit`: for short files the person brings, so what plays
/// later is known to decode. No pictures, no thread.
pub(crate) fn sound_length(
    bytes: &Arc<[u8]>,
    limit: Duration,
) -> Result<Option<Duration>, MediaError> {
    let mut sound: Box<dyn Sound> =
        if bytes.starts_with(b"OggS") && bytes.get(28..36) == Some(b"OpusHead") {
            Box::new(ogg::OggOpus::open(bytes)?)
        } else {
            match file::FileSound::open(bytes.clone())? {
                Some(sound) => Box::new(sound),
                None => return Err(unreadable()),
            }
        };
    let most = (limit.as_secs_f64() * f64::from(PLAYBACK_RATE)) as usize;
    let (mut samples, mut out) = (0, Vec::new());
    while sound.next(&mut out)? {
        samples += out.len();
        out.clear();
        if samples > most {
            return Ok(None);
        }
    }
    Ok(Some(Duration::from_secs_f64(
        samples as f64 / f64::from(PLAYBACK_RATE),
    )))
}

/// Read `bytes` and start its pictures, if any; the sound it plays.
fn open(bytes: &Arc<[u8]>, shared: &Arc<Shared>) -> Result<Box<dyn Sound>, MediaError> {
    // A voice message, as before: the first packet names Opus.
    if bytes.starts_with(b"OggS") && bytes.get(28..36) == Some(b"OpusHead") {
        let sound = ogg::OggOpus::open(bytes)?;
        shared
            .length
            .store(sound.length().unwrap_or(0), Ordering::Release);
        return Ok(Box::new(sound));
    }
    let refused = |codec: &str| {
        MediaError(tr!(
            "Este video usa {codec}, que RUNE no reproduce.",
            codec = codec_name(codec)
        ))
    };
    // The pictures, and how long they last when the file says.
    let (video, seconds) = if bytes.get(4..8) == Some(b"ftyp") {
        match crate::mp4::video_track(bytes)
            .map_err(|_| MediaError(t("RUNE no puede leer este archivo MP4.").into()))?
        {
            Some(track) => {
                if let crate::mp4::Codec::Other(codec) = &track.codec {
                    return Err(refused(codec));
                }
                let seconds = track.seconds();
                (Some(Pictures::Mp4(track)), Some(seconds))
            }
            None => (None, None),
        }
    } else if bytes.starts_with(&crate::mkv::MAGIC) {
        match crate::mkv::video_track(bytes)
            .map_err(|_| MediaError(t("RUNE no puede leer este archivo WebM o MKV.").into()))?
        {
            Some(track) => {
                let decoders = decoders(&track.codec).ok_or_else(|| refused(&track.codec))?;
                // Refused before a decoder reserves room for pictures this large.
                #[cfg(target_os = "linux")]
                let (most_wide, most_tall) = crate::video::decode::MAX_PICTURE;
                // Elsewhere no picture is decoded (`start_pictures`).
                #[cfg(not(target_os = "linux"))]
                let (most_wide, most_tall) = (usize::MAX, usize::MAX);
                if track
                    .size
                    .is_some_and(|(wide, tall)| wide > most_wide as u64 || tall > most_tall as u64)
                {
                    return Err(MediaError(
                        t("Las imágenes de este video son más grandes de lo que RUNE muestra.")
                            .into(),
                    ));
                }
                (Some(Pictures::Matroska(decoders)), track.seconds)
            }
            None => (None, None),
        }
    } else {
        (None, None)
    };
    let sound: Box<dyn Sound> = match file::FileSound::open(bytes.clone())? {
        Some(sound) => Box::new(sound),
        None => match (&video, seconds) {
            // A video without sound runs on a silent clock.
            (Some(_), Some(seconds)) => Box::new(Silence {
                length: (seconds * f64::from(PLAYBACK_RATE)) as u64,
                at: 0,
            }),
            // shortcut: a silent recording that does not say how long it is
            // (a live WebM) has no clock; measure its pictures if one turns up.
            (Some(_), None) => {
                return Err(MediaError(
                    t("Este video no dice cuánto dura y no tiene sonido.").into(),
                ))
            }
            (None, _) => {
                return Err(MediaError(
                    t("Este archivo no tiene un sonido que RUNE reproduzca.").into(),
                ))
            }
        },
    };
    let length = sound
        .length()
        .max(seconds.map(|seconds| (seconds * f64::from(PLAYBACK_RATE)) as u64));
    too_long(length.unwrap_or(0))?;
    shared.length.store(length.unwrap_or(0), Ordering::Release);
    if let Some(video) = video {
        start_pictures(bytes.clone(), video, shared)?;
    }
    Ok(sound)
}

/// How a video's pictures are read; only Linux decodes them for now.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
enum Pictures {
    /// H264 in MP4, by RUNE's reader and OpenH264.
    Mp4(crate::mp4::VideoTrack),
    /// WebM or MKV, by GStreamer with the first of these decoders it has.
    Matroska(&'static [&'static str]),
}

/// The GStreamer decoders of a Matroska codec, in the order they are tried;
/// `None` for one RUNE does not play: H.265, whose only software decoder
/// at hand (libde265) brings its patents, among others.
fn decoders(codec: &str) -> Option<&'static [&'static str]> {
    Some(match codec {
        "V_VP8" => &["vp8dec"],
        "V_VP9" => &["vp9dec"],
        // dav1d where the system has it, libaom's otherwise.
        "V_AV1" => &["dav1ddec", "av1dec"],
        "V_MPEG4/ISO/AVC" => &["h264parse ! openh264dec"],
        _ => return None,
    })
}

/// What a codec is called, by its four letters (MP4) or its Matroska name.
fn codec_name(codec: &str) -> String {
    match codec {
        "hvc1" | "hev1" | "V_MPEGH/ISO/HEVC" => "H.265 (HEVC)".into(),
        "vp08" => "VP8".into(),
        "vp09" => "VP9".into(),
        "av01" => "AV1".into(),
        other => tr!(
            "el formato «{name}»",
            name = other.trim().chars().take(32).collect::<String>()
        ),
    }
}

#[cfg(target_os = "linux")]
fn start_pictures(
    bytes: Arc<[u8]>,
    video: Pictures,
    shared: &Arc<Shared>,
) -> Result<(), MediaError> {
    let shared = shared.clone();
    shared.video.store(true, Ordering::Release);
    thread::Builder::new()
        .name("rune-clip-video".into())
        .spawn(move || {
            let shown = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match video {
                Pictures::Mp4(track) => pictures::run(&bytes, &track, &shared),
                Pictures::Matroska(decoders) => stream::run(bytes, decoders, &shared),
            }));
            if shown.is_err() {
                shared.fail(unreadable());
            }
        })
        .map(drop)
        .map_err(|_| MediaError(t("No se pudo reproducir el video.").into()))
}

#[cfg(not(target_os = "linux"))]
fn start_pictures(_: Arc<[u8]>, _: Pictures, _: &Arc<Shared>) -> Result<(), MediaError> {
    Err(MediaError(
        t("RUNE reproduce videos sólo en Linux por ahora.").into(),
    ))
}

fn run(sound: &mut dyn Sound, mut feed: Feed, shared: &Shared) -> Result<(), MediaError> {
    let mut fresh = Vec::new();
    // The sound ran out.
    let mut over = false;
    // At 1× the audio goes out as decoded; otherwise through `stretch`.
    let mut stretch: Option<Stretch> = None;
    let mut stretched = Vec::new();
    // Samples into the clip already queued, and samples sent to the output.
    let mut decoded = 0;
    let mut sent = 0;
    // When the clock started, at which speed, and both counts then.
    let mut clock: Option<(Instant, f32, u64, u64)> = None;
    loop {
        if shared.stopped() {
            return Ok(());
        }
        if !feed.connected() {
            // Something else took the output, such as a call.
            shared.ended.store(true, Ordering::Release);
            return Ok(());
        }
        let target = shared.seek.swap(NO_SEEK, Ordering::AcqRel);
        if target != NO_SEEK {
            sound.seek(target)?;
            (decoded, clock, stretch, over) = (target, None, None, false);
        }
        let speed = f32::from_bits(shared.speed.load(Ordering::Acquire));
        if shared.paused.load(Ordering::Acquire)
            || clock.is_some_and(|(_, at_speed, ..)| at_speed != speed)
        {
            // What is queued plays out; it resumes after it.
            if clock.take().is_some() {
                shared.position.store(decoded, Ordering::Release);
            }
            thread::park_timeout(TICK);
            continue;
        }
        let (since, _, at, from) = *clock.get_or_insert((Instant::now(), speed, decoded, sent));
        let played = (since.elapsed().as_secs_f64() * f64::from(PLAYBACK_RATE)) as u64;
        let heard = at + (played as f64 * f64::from(speed)) as u64;
        shared.position.store(heard.min(decoded), Ordering::Release);
        if over && from + played >= sent {
            shared.ended.store(true, Ordering::Release);
            return Ok(());
        }
        while !over && sent < from + played + LEAD {
            fresh.clear();
            if !sound.next(&mut fresh)? {
                over = true;
                if let Some(mut done) = stretch.take() {
                    done.finish(&mut stretched);
                }
            }
            let gain = f32::from_bits(shared.volume.load(Ordering::Acquire));
            if gain != 1.0 {
                fresh.iter_mut().for_each(|sample| *sample *= gain);
            }
            decoded += fresh.len() as u64;
            if speed == 1.0 {
                if let Some(mut done) = stretch.take() {
                    done.finish(&mut stretched);
                }
                stretched.extend_from_slice(&fresh);
            } else if !fresh.is_empty() {
                stretch
                    .get_or_insert_with(Stretch::new)
                    .push(&fresh, speed, &mut stretched);
            }
            feed.push(&stretched);
            sent += stretched.len() as u64;
            stretched.clear();
        }
        thread::park_timeout(TICK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four seconds of a synthesized, speech-like tone (see docs/MEDIA.md).
    const SAMPLE: &[u8] = include_bytes!("../../../assets/voice-sample.ogg");

    fn wait(until: impl Fn() -> bool) {
        let started = Instant::now();
        while !until() {
            assert!(started.elapsed() < Duration::from_secs(5), "timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_clip_waits_paused_plays_by_the_clock_seeks_and_ends() {
        let (feed, mut output) = Feed::pair(PLAYBACK_RATE as usize);
        let clip = Clip::start(SAMPLE.into(), feed, 1.0).unwrap();
        thread::sleep(Duration::from_millis(50));
        assert_eq!(output.slots(), 0, "nothing plays until unpaused");
        assert_eq!(clip.position(), 1.0);
        // Its last page says how long it is, to the sample.
        wait(|| clip.duration().is_some());
        assert_eq!(clip.duration(), Some(4.0));
        clip.set_paused(false);
        wait(|| clip.position() > 1.1);
        // Paced: never far ahead of the clock.
        assert!(output.slots() < PLAYBACK_RATE as usize / 2);
        clip.set_paused(true);
        thread::sleep(Duration::from_millis(30));
        let paused = clip.position();
        thread::sleep(Duration::from_millis(60));
        assert_eq!(clip.position(), paused);
        assert_eq!(clip.status(), ClipStatus::Open);
        while output.pop().is_ok() {}
        clip.seek(3.9);
        clip.set_paused(false);
        wait(|| clip.status() == ClipStatus::Ended);
        assert!(clip.position() >= 3.9);
        assert!(output.slots() > 0);
    }

    #[test]
    fn a_faster_clip_ends_sooner() {
        let (feed, mut output) = Feed::pair(PLAYBACK_RATE as usize);
        let clip = Clip::start(SAMPLE.into(), feed, 2.0).unwrap();
        clip.set_speed(2.0);
        let started = Instant::now();
        clip.set_paused(false);
        let mut queued = 0;
        while clip.status() == ClipStatus::Open {
            assert!(started.elapsed() < Duration::from_secs(5), "timed out");
            while output.pop().is_ok() {
                queued += 1;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(clip.status(), ClipStatus::Ended);
        // Two seconds of the clip in about one.
        let took = started.elapsed().as_secs_f32();
        assert!(took > 0.8 && took < 1.6, "{took} s");
        let seconds = queued as f32 / PLAYBACK_RATE as f32;
        assert!(seconds > 0.9 && seconds < 1.2, "{seconds} s of audio");
    }

    fn fixture(name: &str) -> Arc<[u8]> {
        let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(path).expect("fixture").into()
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_video_shows_its_pictures_by_the_clock_of_its_sound() {
        let (feed, mut output) = Feed::pair(PLAYBACK_RATE as usize);
        let clip = Clip::start(fixture("clip.mp4"), feed, 0.0).unwrap();
        wait(|| clip.duration().is_some());
        assert!((clip.duration().unwrap() - 2.0).abs() < 0.1);
        assert!(clip.has_pictures());
        // Paused at the start, its first picture shows already.
        let first = {
            let started = Instant::now();
            loop {
                if let Some(picture) = clip.take_picture() {
                    break picture;
                }
                assert!(started.elapsed() < Duration::from_secs(5), "no picture");
                thread::sleep(Duration::from_millis(5));
            }
        };
        assert_eq!((first.width, first.height), (160, 90));
        clip.set_volume(0.0);
        clip.set_paused(false);
        wait(|| clip.position() > 0.3);
        assert!(clip.take_picture().is_some(), "later pictures follow");
        let mut heard = Vec::new();
        while let Ok(sample) = output.pop() {
            heard.push(sample);
        }
        assert!(crate::rms(&heard) < 1e-6, "muted");
        clip.seek(1.9);
        wait(|| clip.status() == ClipStatus::Ended);
    }

    #[test]
    fn a_video_in_another_codec_says_which() {
        for name in ["hevc.mp4", "hevc.mkv"] {
            let (feed, _output) = Feed::pair(1_024);
            let clip = Clip::start(fixture(name), feed, 0.0).unwrap();
            wait(|| matches!(clip.status(), ClipStatus::Failed(_)));
            let ClipStatus::Failed(reason) = clip.status() else {
                unreachable!()
            };
            assert!(reason.contains("H.265"), "{name}: {reason}");
        }
    }

    /// The first picture shown, waiting for it at most five seconds.
    #[cfg(target_os = "linux")]
    fn picture(clip: &Clip) -> crate::video::Frame {
        let started = Instant::now();
        loop {
            if let Some(picture) = clip.take_picture() {
                return picture;
            }
            assert_eq!(clip.status(), ClipStatus::Open);
            assert!(started.elapsed() < Duration::from_secs(5), "no picture");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn webm_and_mkv_videos_show_their_pictures_by_the_clock() {
        for (name, seconds) in [
            ("clip-vp8.webm", 2.0),
            ("clip-vp9.webm", 1.0),
            ("clip-av1.mkv", 1.0),
            ("clip-h264.mkv", 1.0),
        ] {
            let (feed, _output) = Feed::pair(PLAYBACK_RATE as usize);
            let clip = Clip::start(fixture(name), feed, 0.0).unwrap();
            wait(|| clip.duration().is_some());
            let duration = clip.duration().unwrap();
            assert!((duration - seconds).abs() < 0.1, "{name}: {duration} s");
            assert!(clip.has_pictures(), "{name}");
            // Paused at the start, its first picture shows already.
            let first = picture(&clip);
            assert_eq!((first.width, first.height), (160, 90), "{name}");
            clip.set_volume(0.0);
            clip.set_paused(false);
            wait(|| clip.position() > 0.3);
            assert_ne!(
                picture(&clip).rgba,
                first.rgba,
                "{name}: later pictures follow"
            );
            // A jump while paused shows where it landed: the picture a clip
            // started there shows first. One handed over before the jump
            // may come first.
            let target = seconds - 0.2;
            let (there, _heard) = Feed::pair(PLAYBACK_RATE as usize);
            let there = picture(&Clip::start(fixture(name), there, target).unwrap());
            assert_ne!(there.rgba, first.rgba, "{name}");
            clip.set_paused(true);
            clip.seek(target);
            let started = Instant::now();
            while picture(&clip).rgba != there.rgba {
                assert!(
                    started.elapsed() < Duration::from_secs(5),
                    "{name}: not landed"
                );
            }
            clip.set_paused(false);
            wait(|| clip.status() == ClipStatus::Ended);
        }
    }

    /// Why `bytes` failed to play, waiting for it at most five seconds.
    #[cfg(target_os = "linux")]
    fn refused(bytes: Vec<u8>) -> String {
        let (feed, _output) = Feed::pair(1_024);
        let clip = Clip::start(bytes.into(), feed, 0.0).unwrap();
        wait(|| clip.status() != ClipStatus::Open);
        match clip.status() {
            ClipStatus::Failed(reason) => reason,
            other => panic!("{other:?}"),
        }
    }

    // Pictures are decoded on Linux only (windows-base).
    #[cfg(target_os = "linux")]
    #[test]
    fn a_matroska_video_too_large_says_why_before_decoding() {
        // A header alone: one VP9 track of 5120×1440 pictures, a monitor's.
        let element = |id: &[u8], body: &[u8]| [id, &[0x80 | body.len() as u8], body].concat();
        let video = [
            element(&[0xB0], &5120_u16.to_be_bytes()),
            element(&[0xBA], &1440_u16.to_be_bytes()),
        ]
        .concat();
        let entry = [
            element(&[0x83], &[1]),
            element(&[0x86], b"V_VP9"),
            element(&[0xE0], &video),
        ]
        .concat();
        let tracks = element(&[0x16, 0x54, 0xAE, 0x6B], &element(&[0xAE], &entry));
        let large = [
            element(&crate::mkv::MAGIC, &[]),
            element(&[0x18, 0x53, 0x80, 0x67], &tracks),
        ]
        .concat();
        let track = crate::mkv::video_track(&large).unwrap().unwrap();
        assert_eq!(track.size, Some((5120, 1440)));
        assert!(refused(large).contains("más grandes"));
    }

    #[test]
    fn matroska_codecs_are_played_or_named() {
        assert!(decoders("V_VP9").is_some() && decoders("V_AV1").is_some());
        assert_eq!(decoders("V_MPEGH/ISO/HEVC"), None);
        assert_eq!(codec_name("V_MPEGH/ISO/HEVC"), "H.265 (HEVC)");
        assert_eq!(codec_name("V_THEORA"), "el formato «V_THEORA»");
    }

    #[test]
    fn a_sound_file_plays_as_a_voice_message_does() {
        let (feed, mut output) = Feed::pair(PLAYBACK_RATE as usize);
        let clip = Clip::start(fixture("tone.mp3"), feed, 0.5).unwrap();
        wait(|| clip.duration().is_some());
        assert!(!clip.has_pictures());
        clip.set_paused(false);
        let mut queued = 0;
        while clip.status() == ClipStatus::Open {
            while output.pop().is_ok() {
                queued += 1;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(clip.status(), ClipStatus::Ended);
        let seconds = queued as f32 / PLAYBACK_RATE as f32;
        assert!(
            seconds > 0.4 && seconds < 0.7,
            "{seconds} s from the middle"
        );
    }

    #[test]
    fn a_clip_ends_when_its_output_goes_and_reports_a_broken_file() {
        let (feed, output) = Feed::pair(1_024);
        let clip = Clip::start(SAMPLE.into(), feed, 0.0).unwrap();
        drop(output);
        clip.set_paused(false);
        wait(|| clip.status() == ClipStatus::Ended);
        let (feed, _output) = Feed::pair(1_024);
        let clip = Clip::start(Arc::from(&b"OggS"[..]), feed, 0.0).unwrap();
        wait(|| matches!(clip.status(), ClipStatus::Failed(_)));
    }
}
