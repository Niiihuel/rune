//! The account's screen and camera previews: holds the sources the interface
//! wants, presents their newest frames on limn surfaces, and lets go of them
//! when they are no longer wanted, when the account changes, when they end
//! or when the window closes. A screen shared in the call is captured at its
//! quality, and a camera on in it at the camera's preset, each encoded on
//! its own thread into the provider's [`VideoLink`]; what goes out is told
//! to the interface.
//!
//! Also the streams the account watches and the others' cameras on the
//! stage: their units from the provider's [`VideoLink`], sorted by source,
//! decoded each on its own thread, or, with the simulated provider,
//! made-up frames; presented the same way, and let go of as soon as they
//! are no longer shown. The provider is told which cameras are on screen.
//!
//! A stream shown alone makes the window full screen, and one asked for in a
//! window of its own gets a viewport of the host's: limn's frame for it
//! painted there, its pictures on a surface of their own.

// Screen, camera and watched streams are Linux's and Windows'
// (windows-medios): elsewhere this keeps its shape with nothing to show,
// and what only they use is left unused.
#![cfg_attr(
    not(any(target_os = "linux", windows)),
    allow(dead_code, unused_imports, unused_mut)
)]

use std::collections::{HashMap, VecDeque};

use limn::{Element, UiTree};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use eframe::egui;
use rune_components::voice::ids::{self, CAMERA_SURFACE, SCREEN_SURFACE};
use rune_model::{StreamPhase, StreamQuality, UserId, VideoKind};
use rune_provider::video::{VideoLink, VideoRequest, CAMERA, MAX_CAMERAS, MAX_STREAMS};
use rune_provider::VoiceSupport;
use rune_state::Store;
use rune_ui::{Intent, Live, Ui};

#[cfg(any(target_os = "linux", windows))]
use rune_media::video::encode::{Engine, HARDWARE};
#[cfg(any(target_os = "linux", windows))]
use rune_media::video::{encode::EncodeStats, CaptureStats, Frame, Video as Source, VideoState};

/// One source held for the interface.
#[cfg(any(target_os = "linux", windows))]
struct Held {
    source: Source,
    /// The account it was asked for: another one never inherits it.
    account: Option<rune_model::UserId>,
    /// The camera asked for; the screen's is always `None`.
    device: Option<String>,
    /// The quality a shared screen is captured and sent at.
    quality: Option<StreamQuality>,
    /// Its encoder, while it goes to the call.
    encoding: Option<rune_media::video::encode::Encoding>,
    /// The last frame, kept to present again at another size.
    last: Option<Frame>,
    /// The pixels-per-point it was presented at.
    shown: Option<f32>,
}

/// How often a watched stream is looked at: up to 60 frames a second.
const STREAM_POLL: Duration = Duration::from_millis(16);
/// The simulated stream: its size and pace.
const SIMULATED: (u32, u32) = (640, 360);
const SIMULATED_PACE: Duration = Duration::from_millis(66);
/// Units waiting for one source's decoder.
const QUEUE: usize = 8;
/// How often a queue waiting for a key frame asks for it again: a lost
/// request would otherwise freeze the picture until the sender's next one.
const KEY_AGAIN: Duration = Duration::from_millis(500);
/// How often the shared screen's diagnostic line is written.
const DIAG_EVERY: Duration = Duration::from_secs(5);
/// What goes to the call, frames and kilobits a second, and who encodes it.
#[cfg(any(target_os = "linux", windows))]
type Going = ((u32, u32), &'static str);

/// One source's units waiting for its decoder, and whether the next one
/// must be a key frame.
#[derive(Default)]
struct Queue {
    #[cfg(any(target_os = "linux", windows))]
    units: VecDeque<rune_media::video::decode::Unit>,
    skip: bool,
    /// When the key frame waited for was last asked for.
    #[cfg(any(target_os = "linux", windows))]
    asked: Option<Instant>,
}

/// The provider's end, its units sorted by source: each decoder takes its
/// own, and those of a source nobody decodes are dropped.
struct Router {
    link: VideoLink,
    queues: HashMap<(UserId, VideoKind), Weak<Mutex<Queue>>>,
}

#[cfg(any(target_os = "linux", windows))]
impl Router {
    /// Sort what the provider sent into the queues of the sources decoded.
    fn route(&mut self) {
        while let Ok(unit) = self.link.incoming.try_recv() {
            let key = (unit.user, unit.kind);
            let Some(queue) = self.queues.get(&key).and_then(Weak::upgrade) else {
                self.queues.remove(&key);
                continue;
            };
            let Ok(mut queue) = queue.lock() else {
                continue;
            };
            if queue.units.len() >= QUEUE {
                // Behind: what waits goes, and the picture starts again at a key frame.
                queue.units.clear();
                queue.skip = true;
            }
            if queue.skip && !unit.keyframe {
                if queue
                    .asked
                    .is_none_or(|at| unit.at.saturating_duration_since(at) >= KEY_AGAIN)
                {
                    let _ = self.link.requests.try_send(VideoRequest::KeyFrame {
                        user: unit.user,
                        kind: unit.kind,
                    });
                    queue.asked = Some(unit.at);
                }
                continue;
            }
            queue.skip = false;
            queue.asked = None;
            queue.units.push_back(rune_media::video::decode::Unit {
                keyframe: unit.keyframe,
                at: unit.at,
                data: unit.data,
            });
        }
    }
}

/// Where a watched stream's pictures come from.
#[cfg(any(target_os = "linux", windows))]
enum Pictures {
    Decoded(rune_media::video::decode::Decoding),
    /// The simulated provider's: made up here, from when it started.
    Simulated(Instant, Option<Instant>),
}

/// Someone's stream or camera, as held.
#[cfg(any(target_os = "linux", windows))]
struct Watched {
    owner: UserId,
    pictures: Pictures,
    last: Option<Frame>,
    shown: Option<f32>,
    /// The density it was presented at in its own window.
    window: Option<f32>,
    /// Who decoded its last picture, as the log was told, and whether the
    /// GPU was allowed to.
    decoder: Option<rune_media::video::decode::DecodeEngine>,
    hardware: bool,
}

#[cfg(any(target_os = "linux", windows))]
impl Watched {
    /// From `router`, or made up while the provider is simulated; `None`
    /// when this provider carries no pictures.
    fn start(
        router: Option<&Arc<Mutex<Router>>>,
        simulated: bool,
        owner: UserId,
        kind: VideoKind,
        engines: &'static [Engine],
        now: Instant,
    ) -> Option<Self> {
        let pictures = match (router, simulated) {
            (Some(router), _) => Pictures::Decoded(decoding(router.clone(), owner, kind, engines)),
            (None, true) => Pictures::Simulated(now, None),
            (None, false) => return None,
        };
        Some(Self {
            owner,
            pictures,
            last: None,
            shown: None,
            window: None,
            decoder: None,
            hardware: !engines.is_empty(),
        })
    }

    /// Its newest picture on the surface `id`: what the interface shows.
    fn show(
        &mut self,
        ctx: &egui::Context,
        id: &str,
        laid_out: &limn::Frame,
        now: Instant,
    ) -> Live {
        let fresh = match &mut self.pictures {
            Pictures::Decoded(decoding) => decoding.take_frame(),
            Pictures::Simulated(started, made) => {
                let due = made.is_none_or(|at| now.duration_since(at) >= SIMULATED_PACE);
                due.then(|| {
                    *made = Some(now);
                    simulated_frame(now.duration_since(*started))
                })
            }
        };
        if let (Some(_), Pictures::Decoded(decoding)) = (&fresh, &self.pictures) {
            // Told once, and again when a GPU hands over to the next engine.
            let decoder = decoding.engine();
            if self.decoder != Some(decoder) {
                self.decoder = Some(decoder);
                eprintln!(
                    "rune: voice watch: video decoder {}",
                    engine_note(
                        Engine::decoder,
                        decoder.engine,
                        decoder.failed,
                        self.hardware
                    )
                );
            }
        }
        if let Some(frame) = fresh {
            self.last = Some(frame);
            self.shown = None;
            self.window = None;
        }
        let Some(frame) = &self.last else {
            return Live::Waiting;
        };
        if present(ctx, id, frame, laid_out, &mut self.shown) {
            Live::Showing(frame.width, frame.height)
        } else {
            Live::Waiting
        }
    }
}

#[derive(Default)]
pub struct Video {
    #[cfg(any(target_os = "linux", windows))]
    screen: Option<Held>,
    #[cfg(any(target_os = "linux", windows))]
    camera: Option<Held>,
    /// The provider's end of watched streams and cameras, when it carries them.
    link: Option<Arc<Mutex<Router>>>,
    /// The streams watched, at most [`MAX_STREAMS`].
    #[cfg(any(target_os = "linux", windows))]
    streams: Vec<Watched>,
    /// The others' cameras on the stage, at most [`MAX_CAMERAS`].
    #[cfg(any(target_os = "linux", windows))]
    cameras: Vec<Watched>,
    /// The cameras the provider was told are on screen.
    #[cfg(any(target_os = "linux", windows))]
    told: Vec<UserId>,
    /// The window was made full screen for a stream shown alone.
    fullscreen: bool,
    /// The trees of the streams in windows of their own.
    windows: HashMap<UserId, UiTree>,
    /// The applications' sound, while it goes along with the shared screen.
    #[cfg(any(target_os = "linux", windows))]
    sound: Option<rune_media::video::sound::ScreenSound>,
    /// When the shared screen's last diagnostic line was written, and what
    /// its source and encoder had counted then.
    #[cfg(any(target_os = "linux", windows))]
    diag: Option<(Instant, CaptureStats, EncodeStats)>,
    /// The GPU's engines the account lets encode and decode video.
    #[cfg(any(target_os = "linux", windows))]
    engines: &'static [Engine],
    // RUNE's own screen picker (windows-medios).
    /// While the share's picker is open: what it offers, being listed off
    /// this thread (`Some(Some)`), or listed already for this opening.
    #[cfg(any(target_os = "linux", windows))]
    sources: Option<Option<std::sync::mpsc::Receiver<Vec<rune_media::video::ScreenSource>>>>,
    // Streams (transmisiones).
    /// The picture of the account's own stream being made off this thread,
    /// and where it goes.
    #[cfg(any(target_os = "linux", windows))]
    thumbnail: Option<(Thumbnail, std::sync::mpsc::Receiver<Option<Vec<u8>>>)>,
}

/// Where the picture of the account's own stream goes: its server, its
/// channel and the account.
type Thumbnail = (Option<rune_model::ServerId>, rune_model::ChannelId, UserId);

/// The picture of a stream that goes up: the frame as large as fits
/// 512×288, as a JPEG, as the official client makes it (REA); `None` for a
/// frame with nothing on it yet (all black).
fn thumbnail_jpeg(frame: &rune_media::video::Frame) -> Option<Vec<u8>> {
    let lit = frame
        .rgba
        .chunks_exact(4)
        .any(|pixel| pixel[..3].iter().any(|channel| *channel != 0));
    let picture = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())?;
    if !lit || frame.width == 0 || frame.height == 0 {
        return None;
    }
    let scale = (512.0 / frame.width as f32).min(288.0 / frame.height as f32);
    let width = ((frame.width as f32 * scale).round() as u32).max(1);
    let height = ((frame.height as f32 * scale).round() as u32).max(1);
    let small = image::imageops::resize(
        &picture,
        width,
        height,
        image::imageops::FilterType::Triangle,
    );
    let rgb = image::DynamicImage::ImageRgba8(small).into_rgb8();
    let mut jpeg = Vec::new();
    // A browser canvas's own JPEG quality, which the official client keeps.
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 92)
        .encode_image(&rgb)
        .ok()?;
    Some(jpeg)
}

/// Pixels per point that fill `area` with a frame `width` pixels wide: the
/// interface already gave the surface the frame's shape.
fn density(width: u32, area: Option<limn::Rect>) -> f32 {
    let density = area.map_or(1.0, |area| width as f32 / area.width);
    if density.is_finite() && density > 0.0 {
        density
    } else {
        1.0
    }
}

fn surface(frame: &limn::Frame, id: &str) -> Option<limn::Rect> {
    frame.commands.iter().find_map(|command| match command {
        limn::DrawCommand::Surface {
            rect, id: shown, ..
        } if shown == id => Some(*rect),
        _ => None,
    })
}

impl Video {
    /// With the provider's end of watched streams, if it carries them.
    pub fn with_link(link: Option<VideoLink>) -> Self {
        Self {
            link: link.map(|link| {
                Arc::new(Mutex::new(Router {
                    link,
                    queues: HashMap::new(),
                }))
            }),
            ..Self::default()
        }
    }

    /// A source is held: the host polls its frames at the preview's rate.
    pub fn active(&self) -> bool {
        #[cfg(any(target_os = "linux", windows))]
        {
            self.screen.is_some()
                || self.camera.is_some()
                || !self.streams.is_empty()
                || !self.cameras.is_empty()
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        false
    }

    /// When to look for the next frame, while a source is held.
    pub fn next_poll(&self) -> Option<std::time::Duration> {
        #[cfg(any(target_os = "linux", windows))]
        {
            if !self.streams.is_empty() || !self.cameras.is_empty() {
                return Some(STREAM_POLL);
            }
            self.active().then_some(rune_media::video::PREVIEW_INTERVAL)
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        None
    }

    /// Follow what the interface wants, after `laid_out` placed the
    /// surfaces. Returns whether the interface changed and needs a frame.
    /// What the interface asks for in turn goes in the intents. `parent`
    /// names the window for the screen's dialog (escritorio-linux), asked
    /// only when it opens.
    pub fn step(
        &mut self,
        ctx: &egui::Context,
        ui: &mut Ui,
        store: &mut Store,
        laid_out: &limn::Frame,
        now: Instant,
        parent: &mut dyn FnMut() -> Option<rune_platform::file_dialogs::ParentWindow>,
    ) -> (bool, Vec<Intent>) {
        let mut intents = Vec::new();
        let before = (
            ui.video.screen,
            ui.video.camera.live,
            ui.video.cameras.len(),
            ui.video.streams.clone(),
            ui.video.watched.clone(),
            ui.video.sources.clone(),
        );
        #[cfg(any(target_os = "linux", windows))]
        {
            self.set_engines(if ui.prefs.video_hardware {
                HARDWARE
            } else {
                &[]
            });
            let (screen, camera) = ui.video.wanted(store);
            let account = store.me();
            let device = ui.video.device.clone();
            let sending = rune_ui::video_sending(ui, store);
            self.step_sources(ctx, ui);
            // What RUNE's own picker chose (Windows), or the portal's token
            // of what its dialog chose last time (Linux).
            let chosen = ui
                .video
                .source
                .clone()
                .or_else(|| ui.prefs.screen_restore.clone());
            // Captured at the quality to send from the dialog on: the stream
            // comes after, and must not ask for the source again.
            let screen = step_one(
                ctx,
                &mut self.screen,
                screen.then_some(None),
                chosen,
                rune_ui::video_capture_quality(ui, store),
                account,
                SCREEN_SURFACE,
                laid_out,
                parent,
            )
            .and_then(|(live, _)| {
                let sent = self.send(VideoKind::Screen, account, sending)?;
                ui.video.screen.sent = sent.map(|(sent, _)| sent);
                ui.video.screen.encoder = sent.map(|(_, encoder)| encoder);
                Ok(live)
            });
            match screen {
                Err(reason) => intents = rune_ui::video_ended(ui, store, true, reason, now),
                Ok(live) => ui.video.screen.live = live,
            }
            // The person chose in the dialog and frames arrive: now the stream.
            let running = self
                .screen
                .as_ref()
                .is_some_and(|held| held.source.snapshot().state == VideoState::Running);
            intents.extend(rune_ui::video_screen_ready(ui, store, running));
            // The system remembered the screen chosen: asked for it next time.
            let chosen = self
                .screen
                .as_ref()
                .and_then(|held| held.source.snapshot().restore_token);
            if let Some(token) = chosen {
                intents.extend(rune_ui::video_screen_chosen(ui, token));
            }
            let camera_sending = rune_ui::video_camera_sending(ui, store).then_some(CAMERA);
            let camera = step_one(
                ctx,
                &mut self.camera,
                camera.then_some(device),
                None,
                camera_sending,
                account,
                CAMERA_SURFACE,
                laid_out,
                &mut || None,
            )
            .and_then(|shown| {
                let sent = self.send(VideoKind::Camera, account, camera_sending)?;
                ui.video.camera.sent = sent.map(|(sent, _)| sent);
                ui.video.camera.encoder = sent.map(|(_, encoder)| encoder);
                Ok(shown)
            });
            match camera {
                Err(reason) => intents.extend(rune_ui::video_ended(ui, store, false, reason, now)),
                Ok((live, cameras)) => {
                    ui.video.camera.live = live;
                    if let Some(cameras) = cameras {
                        ui.video.cameras = cameras;
                    }
                }
            }
            self.diagnose(now);
            self.step_sound(ui, store, now);
            ui.video.streams = self.step_streams(ctx, store, laid_out, now);
            intents.extend(rune_ui::video_previews(ui, store, now));
            intents.extend(self.step_thumbnail(ui, store, now));
            let wanted = rune_ui::video_cameras(ui, store);
            ui.video.watched = self.step_cameras(ctx, &wanted, simulated(store), laid_out, now);
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = (ctx, &store, laid_out, now, parent);
            ui.video.screen.live = Live::Off;
            ui.video.camera.live = Live::Off;
            ui.video.streams.clear();
            ui.video.watched.clear();
        }
        let changed = before
            != (
                ui.video.screen,
                ui.video.camera.live,
                ui.video.cameras.len(),
                ui.video.streams.clone(),
                ui.video.watched.clone(),
                ui.video.sources.clone(),
            );
        (changed, intents)
    }

    /// The picture of the account's own stream, when the interface says it
    /// is time: made off this thread from the screen's latest frame, and
    /// sent once ready.
    #[cfg(any(target_os = "linux", windows))]
    fn step_thumbnail(&mut self, ui: &mut Ui, store: &Store, now: Instant) -> Vec<Intent> {
        use std::sync::mpsc::TryRecvError;
        if let Some((target, made)) = &self.thumbnail {
            let jpeg = match made.try_recv() {
                Err(TryRecvError::Empty) => return Vec::new(),
                Ok(jpeg) => jpeg,
                Err(TryRecvError::Disconnected) => None,
            };
            let (server, channel, owner) = *target;
            self.thumbnail = None;
            return match jpeg {
                Some(jpeg) => vec![Intent::Command(
                    rune_provider::Command::UploadStreamPreview {
                        server,
                        channel,
                        owner,
                        jpeg,
                    },
                )],
                None => {
                    rune_ui::video_own_preview_failed(ui, now);
                    Vec::new()
                }
            };
        }
        let Some(target) = rune_ui::video_own_preview(ui, store, now) else {
            return Vec::new();
        };
        let Some(frame) = self.screen.as_ref().and_then(|held| held.last.clone()) else {
            rune_ui::video_own_preview_failed(ui, now);
            return Vec::new();
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(thumbnail_jpeg(&frame));
        });
        self.thumbnail = Some((target, receiver));
        Vec::new()
    }

    /// While the share's picker is open, list what RUNE's own picker offers
    /// (Windows has no portal; Linux lists nothing), off this thread, once
    /// each time it opens.
    #[cfg(any(target_os = "linux", windows))]
    fn step_sources(&mut self, ctx: &egui::Context, ui: &mut Ui) {
        if !ui.video.picker {
            self.sources = None;
            return;
        }
        let listing = self.sources.get_or_insert_with(|| {
            let (sender, receiver) = std::sync::mpsc::channel();
            let ctx = ctx.clone();
            let spawned = std::thread::Builder::new()
                .name("rune-screens".into())
                .spawn(move || {
                    let _ = sender.send(rune_media::video::sources());
                    ctx.request_repaint();
                });
            spawned.is_ok().then_some(receiver)
        });
        let listed = listing
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        if let Some(listed) = listed {
            *listing = None;
            let listed = listed
                .into_iter()
                .map(|source| (source.id, source.name, source.window))
                .collect();
            rune_ui::video_sources_listed(ui, listed);
        }
    }

    /// Let the GPU's `engines` encode and decode, at once: what is sent and
    /// watched starts again with them from a key frame, keeping its source
    /// and last picture.
    #[cfg(any(target_os = "linux", windows))]
    fn set_engines(&mut self, engines: &'static [Engine]) {
        if engines == self.engines {
            return;
        }
        self.engines = engines;
        for held in [&mut self.screen, &mut self.camera].into_iter().flatten() {
            held.encoding = None;
        }
        // Its counts start again with the new encoder.
        self.diag = None;
        let Some(router) = self.link.clone() else {
            return;
        };
        let streams = self
            .streams
            .iter_mut()
            .map(|held| (held, VideoKind::Screen));
        let cameras = self
            .cameras
            .iter_mut()
            .map(|held| (held, VideoKind::Camera));
        for (held, kind) in streams.chain(cameras) {
            if let Pictures::Decoded(_) = held.pictures {
                held.pictures =
                    Pictures::Decoded(decoding(router.clone(), held.owner, kind, engines));
                held.hardware = !engines.is_empty();
                held.decoder = None;
            }
        }
    }

    /// Encode the shared screen, or the camera, for the provider while it
    /// goes to the call at `sending`: what goes out and who encodes it, or
    /// why the encoder stopped.
    #[cfg(any(target_os = "linux", windows))]
    fn send(
        &mut self,
        kind: VideoKind,
        account: Option<UserId>,
        sending: Option<StreamQuality>,
    ) -> Result<Option<Going>, Option<String>> {
        use rune_media::video::encode::{Ask, Encoding, Settings, MIN_BITRATE};
        use rune_provider::video::{Feedback, VideoUnit, CAMERA_MAX_BITRATE, CAMERA_MIN_BITRATE};
        let (slot, surface) = match kind {
            VideoKind::Screen => (&mut self.screen, SCREEN_SURFACE),
            VideoKind::Camera => (&mut self.camera, CAMERA_SURFACE),
        };
        let (Some(held), Some(quality), Some(me), Some(router)) =
            (slot.as_mut(), sending, account, self.link.as_ref())
        else {
            return Ok(None);
        };
        let camera = kind == VideoKind::Camera;
        if held.encoding.is_none() {
            let Ok(mut guard) = router.lock() else {
                return Ok(None);
            };
            // What was said about an earlier one is not about this one.
            let link = &mut guard.link;
            let (feedback, outgoing) = if camera {
                (&mut link.camera_feedback, link.camera_outgoing.clone())
            } else {
                (&mut link.feedback, link.outgoing.clone())
            };
            while feedback.try_recv().is_ok() {}
            drop(guard);
            let room = outgoing.clone();
            let feedback = router.clone();
            let (min_bitrate, max_bitrate) = if camera {
                (CAMERA_MIN_BITRATE, CAMERA_MAX_BITRATE)
            } else {
                (MIN_BITRATE, quality.max_bitrate())
            };
            held.encoding = Some(Encoding::start(
                Settings {
                    fps: quality.fps,
                    // Where the provider's rate starts too: at the most.
                    bitrate: max_bitrate,
                    min_bitrate,
                    max_bitrate,
                    camera,
                    engines: self.engines,
                },
                held.source.frames(),
                move || room.capacity() > 0,
                move |unit| {
                    outgoing
                        .try_send(VideoUnit {
                            user: me,
                            kind,
                            keyframe: unit.keyframe,
                            at: unit.at,
                            data: unit.data,
                        })
                        .is_ok()
                },
                move || {
                    let mut router = feedback.try_lock().ok()?;
                    let link = &mut router.link;
                    let asked = if camera {
                        link.camera_feedback.try_recv()
                    } else {
                        link.feedback.try_recv()
                    };
                    match asked.ok()? {
                        Feedback::KeyFrame => Some(Ask::KeyFrame),
                        Feedback::Bitrate(bitrate) => Some(Ask::Bitrate(bitrate)),
                    }
                },
            ));
        }
        let Some(encoding) = &held.encoding else {
            return Ok(None);
        };
        if let VideoState::Ended(reason) = encoding.state() {
            *slot = None;
            limn_egui::forget_surface(surface);
            return Err(reason);
        }
        let sent = encoding.sent();
        Ok(Some((
            (sent.fps, sent.bitrate / 1000),
            sent.engine.encoder(),
        )))
    }

    /// Every [`DIAG_EVERY`] while the screen goes to the call: what its
    /// source delivered and its encoder made, on stderr, beside the
    /// provider's `network` line, for the owner to send when a stream is
    /// not fluid; and the applications its sound mixes. Sizes, rates,
    /// counts and applications' names only.
    #[cfg(any(target_os = "linux", windows))]
    fn diagnose(&mut self, now: Instant) {
        let counted = self.screen.as_ref().and_then(|held| {
            let encoding = held.encoding.as_ref()?;
            Some((
                held.source.capture_stats(),
                encoding.stats(),
                encoding.sent().bitrate,
            ))
        });
        let Some((capture, encode, bitrate)) = counted else {
            self.diag = None;
            return;
        };
        match self.diag {
            Some((at, ..)) if now.duration_since(at) < DIAG_EVERY => {}
            Some((at, before, made)) => {
                let hardware = !self.engines.is_empty();
                let line = video_line(
                    &before,
                    &capture,
                    &made,
                    &encode,
                    bitrate,
                    hardware,
                    now - at,
                );
                eprintln!("rune: voice share: {line}");
                // Which applications the screen's sound mixes, and which it
                // leaves out and why: names and roles only.
                if let Some(sound) = &self.sound {
                    let heard = rune_media::video::sound::summary(&sound.heard());
                    eprintln!("rune: voice share: sound: {heard}");
                }
                self.diag = Some((now, capture, encode));
            }
            None => self.diag = Some((now, capture, encode)),
        }
    }

    /// Read the applications' sound into the provider while the interface
    /// wants it along with the shared screen; one that fails says why and
    /// the screen goes on without it.
    #[cfg(any(target_os = "linux", windows))]
    fn step_sound(&mut self, ui: &mut Ui, store: &Store, now: Instant) {
        let link = self
            .link
            .as_ref()
            .filter(|_| rune_ui::video_sound(ui, store));
        let Some(link) = link else {
            self.sound = None;
            return;
        };
        if self.sound.is_none() {
            let Some(sound) = link.try_lock().ok().map(|router| router.link.sound.clone()) else {
                return;
            };
            self.sound = Some(rune_media::video::sound::ScreenSound::start(
                move |packet| sound.try_send(packet).is_ok(),
            ));
        }
        let state = self.sound.as_ref().map(|sound| sound.state());
        if let Some(VideoState::Ended(reason)) = state {
            self.sound = None;
            let reason = reason
                .unwrap_or_else(|| rune_i18n::t("El sonido dejó de estar disponible.").into());
            rune_ui::video_sound_failed(ui, reason, now);
        }
    }

    /// Hold the cameras `wanted` (at most [`MAX_CAMERAS`]) and nobody else's,
    /// tell the provider which are on screen, and present their newest
    /// pictures: what each shows.
    #[cfg(any(target_os = "linux", windows))]
    fn step_cameras(
        &mut self,
        ctx: &egui::Context,
        wanted: &[UserId],
        simulated: bool,
        laid_out: &limn::Frame,
        now: Instant,
    ) -> Vec<(UserId, Live)> {
        let wanted = &wanted[..wanted.len().min(MAX_CAMERAS)];
        // Told only once the provider took the request: a full queue tries again.
        if let Some(router) = self.link.as_ref().and_then(|router| router.try_lock().ok()) {
            let wants = |user, on| VideoRequest::Wants {
                user,
                kind: VideoKind::Camera,
                on,
            };
            self.told.retain(|user| {
                wanted.contains(user) || router.link.requests.try_send(wants(*user, false)).is_err()
            });
            for user in wanted {
                if !self.told.contains(user)
                    && router.link.requests.try_send(wants(*user, true)).is_ok()
                {
                    self.told.push(*user);
                }
            }
        }
        self.cameras.retain(|held| {
            let keep = wanted.contains(&held.owner);
            if !keep {
                limn_egui::forget_surface(&ids::camera_surface(held.owner.0));
            }
            keep
        });
        for user in wanted {
            if self.cameras.iter().all(|held| held.owner != *user) {
                let started = Watched::start(
                    self.link.as_ref(),
                    simulated,
                    *user,
                    VideoKind::Camera,
                    self.engines,
                    now,
                );
                self.cameras.extend(started);
            }
        }
        self.cameras
            .iter_mut()
            .map(|held| {
                let id = ids::camera_surface(held.owner.0);
                (held.owner, held.show(ctx, &id, laid_out, now))
            })
            .collect()
    }

    /// Hold the streams the store says are watched (at most
    /// [`MAX_STREAMS`]), if they carry pictures here, and nobody else's;
    /// present their newest pictures: what each shows.
    #[cfg(any(target_os = "linux", windows))]
    fn step_streams(
        &mut self,
        ctx: &egui::Context,
        store: &Store,
        laid_out: &limn::Frame,
        now: Instant,
    ) -> Vec<(UserId, Live)> {
        let wanted: Vec<UserId> = store
            .watching()
            .iter()
            .filter(|watching| !matches!(watching.phase, StreamPhase::Ended(_)))
            .map(|watching| watching.owner)
            .take(MAX_STREAMS)
            .collect();
        self.streams.retain(|held| {
            let keep = wanted.contains(&held.owner);
            if !keep {
                limn_egui::forget_surface(&ids::stream_surface(held.owner.0));
                limn_egui::forget_surface(&ids::window_surface(held.owner.0));
            }
            keep
        });
        let simulated = simulated(store);
        for owner in &wanted {
            if self.streams.iter().all(|held| held.owner != *owner) {
                let started = Watched::start(
                    self.link.as_ref(),
                    simulated,
                    *owner,
                    VideoKind::Screen,
                    self.engines,
                    now,
                );
                self.streams.extend(started);
            }
        }
        self.streams
            .iter_mut()
            .map(|held| {
                let id = ids::stream_surface(held.owner.0);
                (held.owner, held.show(ctx, &id, laid_out, now))
            })
            .collect()
    }

    /// Full screen while a stream watched is shown alone; each stream asked
    /// for in a window of its own drawn in a viewport, until it is closed.
    pub fn show_windows(&mut self, ctx: &egui::Context, ui: &mut Ui, store: &Store) {
        let fullscreen = rune_ui::video_fullscreen(ui, store);
        if fullscreen != self.fullscreen {
            self.fullscreen = fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
        }
        let windows = rune_ui::video_windows(ui, store);
        self.windows.retain(|owner, _| {
            let keep = windows.iter().any(|(shown, _)| shown == owner);
            if !keep {
                limn_egui::forget_surface(&ids::window_surface(owner.0));
            }
            keep
        });
        for (owner, title) in windows {
            let tree = self.windows.entry(owner).or_insert_with(|| {
                let mut tree = UiTree::new(Element::spacer());
                limn_egui::attach_fonts(&mut tree, ctx);
                tree
            });
            #[cfg(any(target_os = "linux", windows))]
            let mut held = self.streams.iter_mut().find(|held| held.owner == owner);
            let viewport = egui::ViewportId::from_hash_of(("rune-stream", owner.0));
            let builder = egui::ViewportBuilder::default()
                .with_title(title)
                .with_icon(crate::frame::icon())
                .with_inner_size([960.0, 540.0]);
            let closed = ctx.show_viewport_immediate(viewport, builder, |inner, _| {
                let area = inner.max_rect();
                let size = limn::Size::new(area.width(), area.height());
                if let Some(view) = rune_ui::video_window(ui, store, owner, size) {
                    tree.set_root(view);
                    let laid_out = tree.layout(size);
                    #[cfg(any(target_os = "linux", windows))]
                    if let Some(held) = held.as_mut() {
                        if let Some(frame) = &held.last {
                            let id = ids::window_surface(owner.0);
                            present(inner.ctx(), &id, frame, &laid_out, &mut held.window);
                        }
                    }
                    limn_egui::paint_at(inner, &laid_out, area.min);
                }
                inner.input(|input| input.viewport().close_requested())
            });
            if closed {
                rune_ui::video_window_closed(ui, owner);
            }
        }
    }

    /// Let go of every source, as when the window closes.
    pub fn clear(&mut self) {
        #[cfg(any(target_os = "linux", windows))]
        {
            for (held, id) in [
                (&mut self.screen, SCREEN_SURFACE),
                (&mut self.camera, CAMERA_SURFACE),
            ] {
                if held.take().is_some() {
                    limn_egui::forget_surface(id);
                }
            }
            for held in self.streams.drain(..) {
                limn_egui::forget_surface(&ids::stream_surface(held.owner.0));
            }
            self.sound = None;
            for held in self.cameras.drain(..) {
                limn_egui::forget_surface(&ids::camera_surface(held.owner.0));
            }
        }
    }
}

fn simulated(store: &Store) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.voice == VoiceSupport::Simulated)
}

/// Decode `owner`'s video of `kind` on its own thread, from its queue in
/// `router`; the queue goes with the thread.
#[cfg(any(target_os = "linux", windows))]
fn decoding(
    router: Arc<Mutex<Router>>,
    owner: UserId,
    kind: VideoKind,
    engines: &'static [Engine],
) -> rune_media::video::decode::Decoding {
    let queue = Arc::new(Mutex::new(Queue::default()));
    let requests = router.clone();
    let mut joined = false;
    rune_media::video::decode::Decoding::start(
        engines,
        move || {
            let mut router = router.try_lock().ok()?;
            if !joined {
                // The newest decoder of a source is the one it feeds.
                router.queues.insert((owner, kind), Arc::downgrade(&queue));
                joined = true;
            }
            router.route();
            drop(router);
            queue.lock().ok()?.units.pop_front()
        },
        move || {
            if let Ok(router) = requests.try_lock() {
                // Full: the provider has requests to answer already.
                let _ = router
                    .link
                    .requests
                    .try_send(VideoRequest::KeyFrame { user: owner, kind });
            }
        },
    )
}

/// A frame of the simulated stream `elapsed` in: color bands drifting
/// sideways and a white bar sweeping across, so it reads as moving video.
#[cfg(any(target_os = "linux", windows))]
fn simulated_frame(elapsed: Duration) -> Frame {
    let (width, height) = SIMULATED;
    let shift = (elapsed.as_millis() / 20) as u32;
    let bar = shift * 4 % width;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let band = ((x + shift) / 80) % 6;
            let pixel = if x.abs_diff(bar) < 6 {
                [240, 240, 240]
            } else {
                let shade = (40 + y * 120 / height) as u8;
                match band {
                    0 => [shade + 60, 40, 60],
                    1 => [40, shade + 50, 80],
                    2 => [50, 70, shade + 70],
                    3 => [shade + 40, shade + 30, 40],
                    4 => [40, shade + 40, shade + 30],
                    _ => [shade + 20, 50, shade + 50],
                }
            };
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
    }
    Frame {
        width,
        height,
        rgba,
    }
}

/// Present `frame` on the surface `id` at the density its laid-out area
/// asks for, unless already shown at it. Whether it is on the surface.
#[cfg(any(target_os = "linux", windows))]
fn present(
    ctx: &egui::Context,
    id: &str,
    frame: &Frame,
    laid_out: &limn::Frame,
    shown: &mut Option<f32>,
) -> bool {
    let density = density(frame.width, surface(laid_out, id));
    if *shown != Some(density) {
        let presented = limn_egui::SurfaceFrame::from_rgba(
            [frame.width as usize, frame.height as usize],
            &frame.rgba,
            density,
        )
        .is_some_and(|surface| limn_egui::present_surface(ctx, id, surface));
        if presented {
            *shown = Some(density);
        }
    }
    shown.is_some()
}

/// Hold the source `wanted` asks for (`Some(device)`), or none; present its
/// newest frame. `Err` when it ended, with why unless RUNE stopped it.
#[cfg(any(target_os = "linux", windows))]
#[allow(clippy::type_complexity)]
fn step_one(
    ctx: &egui::Context,
    slot: &mut Option<Held>,
    wanted: Option<Option<String>>,
    restore: Option<String>,
    sending: Option<StreamQuality>,
    account: Option<rune_model::UserId>,
    id: &str,
    laid_out: &limn::Frame,
    parent: &mut dyn FnMut() -> Option<rune_platform::file_dialogs::ParentWindow>,
) -> Result<(Live, Option<Vec<(String, String)>>), Option<String>> {
    let camera = id == CAMERA_SURFACE;
    let keep = slot.as_ref().is_some_and(|held| {
        wanted.as_ref() == Some(&held.device) && held.account == account && held.quality == sending
    });
    if !keep && slot.take().is_some() {
        limn_egui::forget_surface(id);
    }
    let Some(device) = wanted else {
        return Ok((Live::Off, None));
    };
    let held = slot.get_or_insert_with(|| Held {
        source: match (camera, sending) {
            (true, Some(quality)) => {
                Source::send_camera(device.clone(), quality.size(), quality.fps)
            }
            (true, None) => Source::camera(device.clone()),
            (false, Some(quality)) => Source::share(parent(), restore, quality.size(), quality.fps),
            (false, None) => Source::screen(parent(), restore),
        },
        account,
        device,
        quality: sending,
        encoding: None,
        last: None,
        shown: None,
    });
    let snapshot = held.source.snapshot();
    let cameras = camera.then(|| {
        snapshot
            .cameras
            .iter()
            .map(|camera| (camera.id.clone(), camera.name.clone()))
            .collect()
    });
    if let VideoState::Ended(reason) = snapshot.state {
        *slot = None;
        limn_egui::forget_surface(id);
        return Err(reason);
    }
    if let Some(frame) = held.source.take_frame() {
        held.last = Some(frame);
        held.shown = None;
    }
    let Some(frame) = &held.last else {
        return Ok((Live::Waiting, cameras));
    };
    let live = if present(ctx, id, frame, laid_out, &mut held.shown) {
        Live::Showing(frame.width, frame.height)
    } else {
        Live::Waiting
    };
    Ok((live, cameras))
}

/// What the shared screen's source and encoder did between two counts,
/// over `elapsed`: see [`Video::diagnose`].
#[cfg(any(target_os = "linux", windows))]
fn video_line(
    before: &CaptureStats,
    capture: &CaptureStats,
    made: &EncodeStats,
    encode: &EncodeStats,
    bitrate: u32,
    hardware: bool,
    elapsed: Duration,
) -> String {
    let seconds = elapsed.as_secs_f64().max(0.001);
    let rate = |now: u64, then: u64| now.saturating_sub(then) as f64 / seconds;
    let average = |micros: u64, then: u64, count: u64| {
        micros.saturating_sub(then) as f64 / 1000.0 / count.max(1) as f64
    };
    let kept = capture.kept.saturating_sub(before.kept);
    let encoded = (encode.encoded + encode.skipped).saturating_sub(made.encoded + made.skipped);
    format!(
        "video: encoder {}; source {}x{} {} {} at {}/{} (most {}/{}); delivered {:.1}/s, kept {:.1}/s; \
         encoded {}x{} at {:.1}/s ({:.1}/s skipped by rate control, {} key, {} refused); \
         {:.1}/s replaced before the encoder took them; {:.1} ms converting + {:.1} ms \
         encoding a frame; {:.0} kbit/s, encoder at {} kbit/s ({} rate changes, {} restarts)",
        engine_note(Engine::encoder, encode.engine, encode.failed, hardware),
        capture.size.0,
        capture.size.1,
        capture.format,
        capture.memory,
        capture.rate.0,
        capture.rate.1,
        capture.max_rate.0,
        capture.max_rate.1,
        rate(capture.delivered, before.delivered),
        rate(capture.kept, before.kept),
        encode.size.0,
        encode.size.1,
        rate(encode.encoded, made.encoded),
        rate(encode.skipped, made.skipped),
        encode.keyframes.saturating_sub(made.keyframes),
        encode.refused.saturating_sub(made.refused),
        rate(capture.replaced, before.replaced),
        average(capture.converting_micros, before.converting_micros, kept),
        average(encode.encoding_micros, made.encoding_micros, encoded),
        rate(encode.bytes, made.bytes) * 8.0 / 1000.0,
        bitrate / 1000,
        encode.rate_changes.saturating_sub(made.rate_changes),
        encode.restarts.saturating_sub(made.restarts),
    )
}

/// Who encodes or decodes (`name` says it either way), for the diagnostic
/// lines: the GPU's engine that failed before it, or why OpenH264 does it.
/// `hardware` is whether the account lets the GPU do it.
#[cfg(any(target_os = "linux", windows))]
fn engine_note(
    name: fn(Engine) -> &'static str,
    engine: Engine,
    failed: Option<Engine>,
    hardware: bool,
) -> String {
    match (engine, failed) {
        (_, Some(failed)) => format!("{} after {} failed", name(engine), name(failed)),
        (Engine::Software, None) if hardware => format!("{} (no GPU one started)", name(engine)),
        (Engine::Software, None) => format!("{} (GPU off)", name(engine)),
        _ => name(engine).to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Streams (transmisiones).
    #[test]
    fn a_streams_picture_fits_512_by_288_as_a_jpeg_and_a_black_frame_has_none() {
        let frame = |width: u32, height: u32, value: u8| rune_media::video::Frame {
            width,
            height,
            rgba: [value, value / 2, 0, 255].repeat((width * height) as usize),
        };
        for (width, height, fits) in [(1920, 1080, (512, 288)), (1000, 1000, (288, 288))] {
            let jpeg = thumbnail_jpeg(&frame(width, height, 200)).unwrap();
            assert!(jpeg.starts_with(&[0xFF, 0xD8, 0xFF]));
            let read =
                image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg).unwrap();
            assert_eq!((read.width(), read.height()), fits);
        }
        // Black, as a capture that has not started: nothing yet.
        assert_eq!(thumbnail_jpeg(&frame(64, 36, 0)), None);
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn the_share_line_says_what_the_source_and_encoder_did_each_second() {
        let before = CaptureStats {
            delivered: 100,
            kept: 50,
            converting_micros: 50_000,
            replaced: 1,
            ..CaptureStats::default()
        };
        let capture = CaptureStats {
            size: (2560, 1600),
            format: "BGRx",
            memory: "memfd",
            rate: (0, 1),
            max_rate: (60, 1),
            delivered: 400,
            kept: 200,
            converting_micros: 245_000,
            replaced: 6,
        };
        let made = EncodeStats {
            encoded: 40,
            bytes: 100_000,
            encoding_micros: 300_000,
            ..EncodeStats::default()
        };
        let encode = EncodeStats {
            size: (1152, 720),
            encoded: 188,
            keyframes: 1,
            skipped: 2,
            sent: 150,
            bytes: 1_600_000,
            refused: 0,
            encoding_micros: 1_410_000,
            rate_changes: 1,
            restarts: 0,
            engine: Engine::VaApi,
            failed: None,
        };
        let line = video_line(
            &before,
            &capture,
            &made,
            &encode,
            3_500_000,
            true,
            Duration::from_secs(5),
        );
        assert_eq!(
            line,
            "video: encoder VA-API; source 2560x1600 BGRx memfd at 0/1 (most 60/1); delivered 60.0/s, \
             kept 30.0/s; encoded 1152x720 at 29.6/s (0.4/s skipped by rate control, \
             1 key, 0 refused); 1.0/s replaced before the encoder took them; 1.3 ms \
             converting + 7.4 ms encoding a frame; 2400 kbit/s, encoder at 3500 kbit/s \
             (1 rate changes, 0 restarts)"
        );
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn the_lines_say_who_encodes_and_why_openh264_does() {
        let note =
            |engine, failed, hardware| engine_note(Engine::encoder, engine, failed, hardware);
        assert_eq!(note(Engine::Nvidia, None, true), "NVENC");
        assert_eq!(
            note(Engine::Software, Some(Engine::VaApi), true),
            "OpenH264 after VA-API failed"
        );
        assert_eq!(
            note(Engine::Software, None, true),
            "OpenH264 (no GPU one started)"
        );
        assert_eq!(note(Engine::Software, None, false), "OpenH264 (GPU off)");
        assert_eq!(
            engine_note(Engine::decoder, Engine::Nvidia, Some(Engine::VaApi), true),
            "NVDEC after VA-API failed"
        );
    }

    #[test]
    fn a_frame_fills_the_surface_the_interface_shaped_for_it() {
        let area = |width| Some(limn::Rect::new(0.0, 0.0, width, width * 9.0 / 16.0));
        assert_eq!(density(1280, area(640.0)), 2.0);
        assert_eq!(density(640, area(1280.0)), 0.5);
        assert_eq!(density(640, None), 1.0);
        assert_eq!(density(640, area(0.0)), 1.0);
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn the_simulated_stream_moves_and_is_opaque() {
        let first = simulated_frame(Duration::ZERO);
        let later = simulated_frame(Duration::from_millis(500));
        assert_eq!((first.width, first.height), SIMULATED);
        assert_eq!(first.rgba.len(), (SIMULATED.0 * SIMULATED.1 * 4) as usize);
        assert!(first.rgba.chunks(4).all(|pixel| pixel[3] == 255));
        assert_ne!(first.rgba, later.rgba, "it reads as moving video");
    }

    fn unit(user: u64, kind: VideoKind, keyframe: bool) -> rune_provider::video::VideoUnit {
        rune_provider::video::VideoUnit {
            user: UserId(user),
            kind,
            keyframe,
            at: Instant::now(),
            data: vec![0, 0, 0, 1, if keyframe { 0x65 } else { 0x41 }],
        }
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn units_reach_the_decoder_of_their_source_and_a_full_queue_waits_for_a_key_frame() {
        let (port, link) = rune_provider::video::video_link();
        let mut router = Router {
            link,
            queues: HashMap::new(),
        };
        let camera = Arc::new(Mutex::new(Queue::default()));
        router
            .queues
            .insert((UserId(2), VideoKind::Camera), Arc::downgrade(&camera));
        // Another source, or the same person's screen, nobody decodes.
        for sent in [
            unit(2, VideoKind::Camera, true),
            unit(2, VideoKind::Screen, true),
            unit(3, VideoKind::Camera, true),
        ] {
            port.incoming.try_send(sent).unwrap();
        }
        router.route();
        for _ in 1..QUEUE {
            port.incoming
                .try_send(unit(2, VideoKind::Camera, false))
                .unwrap();
        }
        router.route();
        assert_eq!(camera.lock().unwrap().units.len(), QUEUE);
        assert!(router.link.incoming.try_recv().is_err());
        // One more: behind, so it restarts at the next key frame, asked for.
        port.incoming
            .try_send(unit(2, VideoKind::Camera, false))
            .unwrap();
        router.route();
        assert!(camera.lock().unwrap().units.is_empty());
        assert_eq!(
            port.requests.try_lock().unwrap().try_recv(),
            Ok(VideoRequest::KeyFrame {
                user: UserId(2),
                kind: VideoKind::Camera
            })
        );
        // Still no key frame: asked again once the first request is old,
        // in case it was lost, and not before.
        let mut late = unit(2, VideoKind::Camera, false);
        port.incoming.try_send(late.clone()).unwrap();
        late.at += KEY_AGAIN;
        port.incoming.try_send(late).unwrap();
        router.route();
        let mut requests = port.requests.try_lock().unwrap();
        assert!(requests.try_recv().is_ok());
        assert!(requests.try_recv().is_err());
        drop(requests);
        port.incoming
            .try_send(unit(2, VideoKind::Camera, true))
            .unwrap();
        router.route();
        {
            let units = &camera.lock().unwrap().units;
            assert!(units.len() == 1 && units[0].keyframe);
        }
        // Its decoder gone, its queue goes too.
        drop(camera);
        port.incoming
            .try_send(unit(2, VideoKind::Camera, true))
            .unwrap();
        router.route();
        assert!(router.queues.is_empty());
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn at_most_four_cameras_are_decoded_and_the_provider_is_told_which() {
        use rune_provider::video::VideoRequest::Wants;
        let ctx = egui::Context::default();
        let (port, link) = rune_provider::video::video_link();
        let mut video = Video::with_link(Some(link));
        let people: Vec<UserId> = (2..8).map(UserId).collect();
        let now = Instant::now();
        let shown = video.step_cameras(&ctx, &people, false, &limn::Frame::default(), now);
        assert_eq!(shown.len(), MAX_CAMERAS);
        assert!(shown.iter().all(|(_, live)| *live == Live::Waiting));
        assert_eq!(video.cameras.len(), MAX_CAMERAS);
        assert!(video.active());
        let mut requests = port.requests.try_lock().unwrap();
        let mut told = Vec::new();
        while let Ok(request) = requests.try_recv() {
            if let Wants { user, kind, on } = request {
                told.push((user.0, kind, on));
            }
        }
        let on: Vec<_> = (2..6).map(|user| (user, VideoKind::Camera, true)).collect();
        assert_eq!(told, on);
        // Only one stays on screen: the others are let go and told off.
        video.step_cameras(&ctx, &people[1..2], false, &limn::Frame::default(), now);
        assert_eq!(video.cameras.len(), 1);
        let mut off = Vec::new();
        while let Ok(request) = requests.try_recv() {
            if let Wants {
                user, on: false, ..
            } = request
            {
                off.push(user.0);
            }
        }
        assert_eq!(off, [2, 4, 5]);
        video.clear();
        assert!(!video.active());
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn another_decoder_choice_restarts_what_is_watched_from_a_key_frame() {
        use rune_provider::video::VideoRequest::KeyFrame;
        let ctx = egui::Context::default();
        let (port, link) = rune_provider::video::video_link();
        let mut video = Video::with_link(Some(link));
        let now = Instant::now();
        video.step_cameras(&ctx, &[UserId(2)], false, &limn::Frame::default(), now);
        assert!(!video.cameras[0].hardware);
        let key_frame = || {
            let started = Instant::now();
            loop {
                let asked = port.requests.try_lock().ok().and_then(|mut requests| {
                    std::iter::from_fn(|| requests.try_recv().ok())
                        .any(|request| {
                            matches!(
                                request,
                                KeyFrame {
                                    user: UserId(2),
                                    ..
                                }
                            )
                        })
                        .then_some(())
                });
                if asked.is_some() {
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(5), "no key frame");
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        key_frame();
        // OpenH264 either way, so no GPU is opened: only the choice differs.
        video.set_engines(&[Engine::Software]);
        assert!(video.cameras[0].hardware);
        key_frame();
        video.clear();
    }

    #[test]
    fn nothing_is_held_while_nothing_is_wanted() {
        let ctx = egui::Context::default();
        let mut video = Video::default();
        let mut ui = Ui::new();
        let mut store = Store::new();
        ui.video.screen.on = true;
        // Not in a call: wanting the screen asks the system nothing.
        video.step(
            &ctx,
            &mut ui,
            &mut store,
            &limn::Frame::default(),
            Instant::now(),
            &mut || None,
        );
        assert!(!video.active());
        assert_eq!(ui.video.screen.live, Live::Off);
    }
}
