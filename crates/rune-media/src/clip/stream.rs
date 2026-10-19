//! The pictures of a Matroska or WebM video, read and decoded by GStreamer
//! a little ahead of the clip's clock and handed over as each is due, as
//! [`super::pictures`] does for MP4; a jump seeks the pipeline. The file
//! stays in memory: GStreamer pulls the parts it asks for.

use std::collections::VecDeque;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use rune_i18n::t;

use super::Shared;
use crate::video::{gpu, Frame};
use crate::{MediaError, PLAYBACK_RATE};

/// Pictures decoded ahead of the clock.
const AHEAD: usize = 2;
/// How long one look for a decoded picture waits.
const IDLE: Duration = Duration::from_millis(5);
/// A picture this close to the clock is due: the clock counts whole
/// samples, so a jump to a picture lands a hair before it.
const SLACK: f64 = 0.001;
/// What GStreamer is asked for when it does not say how much.
const PART: usize = 64 * 1024;
/// Tries at a jump the pipeline refuses before the video is given up: some
/// half a second of looks.
const MAX_MISSES: u32 = 100;

/// A part of the file lent to GStreamer without a copy.
struct Part(Arc<[u8]>, Range<usize>);

impl AsRef<[u8]> for Part {
    fn as_ref(&self) -> &[u8] {
        &self.0[self.1.clone()]
    }
}

/// The pipeline from the file to RGBA pictures, stopped when dropped.
struct Pipe {
    pipeline: gst::Pipeline,
    sink: gst_app::AppSink,
    failed: Arc<AtomicBool>,
}

impl Pipe {
    /// `bytes` demuxed and decoded by the first of `decoders` there is.
    fn start(bytes: Arc<[u8]>, decoders: &[&str]) -> Result<Self, MediaError> {
        let unavailable = || MediaError(t("No se pudo iniciar el decodificador de video.").into());
        if !gpu::ready() {
            return Err(unavailable());
        }
        let decoder = decoders
            .iter()
            .find(|chain| {
                chain
                    .split(" ! ")
                    .all(|element| gst::ElementFactory::find(element).is_some())
            })
            .ok_or_else(|| {
                MediaError(
                    t("Este equipo no tiene el decodificador de video que hace falta.").into(),
                )
            })?;
        let pipeline = gst::parse::launch(&format!(
            "appsrc name=src ! matroskademux ! {decoder} ! videoconvert ! \
             video/x-raw,format=RGBA ! \
             appsink name=sink sync=false max-buffers=2 enable-last-sample=false"
        ))
        .ok()
        .and_then(|pipeline| pipeline.downcast::<gst::Pipeline>().ok())
        .ok_or_else(unavailable)?;
        let failed = Arc::new(AtomicBool::new(false));
        let flag = failed.clone();
        pipeline
            .bus()
            .ok_or_else(unavailable)?
            .set_sync_handler(move |_, message| {
                if let gst::MessageView::Error(_) = message.view() {
                    flag.store(true, Ordering::Release);
                }
                gst::BusSyncReply::Drop
            });
        let element = |name| pipeline.by_name(name).ok_or_else(unavailable);
        let src = element("src")?
            .downcast::<gst_app::AppSrc>()
            .map_err(|_| unavailable())?;
        let sink = element("sink")?
            .downcast::<gst_app::AppSink>()
            .map_err(|_| unavailable())?;
        // Random access: the demuxer pulls each part where it wants it,
        // which is what lets it jump through the file's index.
        src.set_stream_type(gst_app::AppStreamType::RandomAccess);
        src.set_size(bytes.len() as i64);
        let at = Arc::new(AtomicUsize::new(0));
        let sought = at.clone();
        src.set_callbacks(
            gst_app::AppSrcCallbacks::builder()
                .need_data(move |src, length| {
                    let from = at.load(Ordering::Acquire);
                    if from >= bytes.len() {
                        let _ = src.end_of_stream();
                        return;
                    }
                    let length = match length {
                        u32::MAX => PART,
                        length => length as usize,
                    };
                    let to = from.saturating_add(length).min(bytes.len());
                    at.store(to, Ordering::Release);
                    let mut buffer = gst::Buffer::from_slice(Part(bytes.clone(), from..to));
                    if let Some(buffer) = buffer.get_mut() {
                        buffer.set_offset(from as u64);
                    }
                    let _ = src.push_buffer(buffer);
                })
                .seek_data(move |_, to| {
                    sought.store(usize::try_from(to).unwrap_or(usize::MAX), Ordering::Release);
                    true
                })
                .build(),
        );
        gpu::RUNNING.fetch_add(1, Ordering::AcqRel);
        let pipe = Self {
            pipeline,
            sink,
            failed,
        };
        pipe.pipeline
            .set_state(gst::State::Playing)
            .map_err(|_| unavailable())?;
        Ok(pipe)
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        gpu::RUNNING.fetch_sub(1, Ordering::AcqRel);
    }
}

/// When `sample` shows, in seconds into the file.
fn shows(sample: &gst::Sample) -> Option<f64> {
    let pts = sample.buffer()?.pts()?;
    let segment = sample.segment()?.downcast_ref::<gst::ClockTime>()?;
    Some(segment.to_stream_time(pts)?.nseconds() as f64 / 1e9)
}

pub(super) fn run(bytes: Arc<[u8]>, decoders: &[&str], shared: &Shared) {
    let pipe = match Pipe::start(bytes, decoders) {
        Ok(pipe) => pipe,
        Err(error) => return shared.fail(error),
    };
    // The clock, in seconds into the file.
    let now = || shared.position.load(Ordering::Acquire) as f64 / f64::from(PLAYBACK_RATE);
    let mut seeks = shared.seeks.load(Ordering::Acquire);
    // Where to jump once the pipeline gave a first picture: before it, the
    // demuxer may not know the file yet.
    let mut jump = Some(now()).filter(|at| *at > 0.0);
    let (mut primed, mut misses) = (false, 0);
    let mut ready: VecDeque<(f64, Frame)> = VecDeque::new();
    let broken = || MediaError(t("RUNE no pudo decodificar este video.").into());
    while !shared.stopped() {
        if pipe.failed.load(Ordering::Acquire) {
            return shared.fail(broken());
        }
        let jumps = shared.seeks.load(Ordering::Acquire);
        if jumps != seeks {
            seeks = jumps;
            jump = Some(now());
            // Pictures from before the jump are not shown after it.
            ready.clear();
        }
        if let Some(to) = jump.filter(|_| primed) {
            let to = gst::ClockTime::from_nseconds((to * 1e9) as u64);
            // Exact: decoded from the key picture before it, none shown before it.
            let flags = gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE;
            if pipe.pipeline.seek_simple(flags, to).is_ok() {
                (jump, misses) = (None, 0);
            } else {
                misses += 1;
                if misses > MAX_MISSES {
                    return shared.fail(broken());
                }
                thread::park_timeout(IDLE);
                continue;
            }
        }
        // Hand over the newest picture that is due; earlier due ones are late.
        let clock = now() + SLACK;
        let mut due = None;
        while ready.front().is_some_and(|(show, _)| *show <= clock) {
            due = ready.pop_front();
        }
        if let Some((_, frame)) = due {
            *shared
                .picture
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(frame);
        }
        if ready.len() >= AHEAD {
            thread::park_timeout(IDLE);
            continue;
        }
        // Waits a little while nothing is decoded yet, or the file ended.
        let Some(sample) = pipe.sink.try_pull_sample(gst::ClockTime::from_mseconds(5)) else {
            if pipe.sink.is_eos() {
                thread::park_timeout(IDLE);
            }
            continue;
        };
        primed = true;
        // A picture from before a jump still to make is not shown.
        if jump.is_some() {
            continue;
        }
        // A picture that cannot be shown (one larger than its header said,
        // past what is shown) ends the video with why, not a silent one.
        let Some(frame) = gpu::picture(&sample) else {
            return shared.fail(broken());
        };
        if let Some(show) = shows(&sample) {
            ready.push_back((show, frame));
        }
    }
}
