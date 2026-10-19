//! The GPU's H264 encoder and decoder, through GStreamer: VA-API (Mesa: the
//! integrated AMD or Intel GPU) first, then NVIDIA's NVENC and NVDEC
//! (`nvcodec`). Each is one private pipeline from an `appsrc` to an
//! `appsink`, fed and drained without waiting by the thread of
//! [`super::encode`] or [`super::decode`], which go on with OpenH264 when
//! none starts or one fails.
//!
//! The plugins are found at run time: through the shell's
//! `GST_PLUGIN_SYSTEM_PATH`, or the directories `shell.nix` compiled in
//! (`RUNE_GST_PLUGINS`) for a binary run outside it. Errors are only
//! counted: no native message, element or device name leaves here.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use gst::prelude::*;
use gst_video::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

use super::decode::Unit;
use super::Frame;

/// Who encodes or decodes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Engine {
    /// OpenH264, on the CPU: always there.
    #[default]
    Software,
    /// VA-API, through Mesa.
    VaApi,
    /// NVENC to encode, NVDEC to decode.
    Nvidia,
}

/// The GPU's engines in the order they are tried, OpenH264 after them: the
/// integrated GPU's VA-API first, as its pictures need not cross the bus.
pub const HARDWARE: &[Engine] = &[Engine::VaApi, Engine::Nvidia];

/// The engines after `engine` in `engines`: what is left once it failed.
pub(crate) fn after(engines: &'static [Engine], engine: Engine) -> &'static [Engine] {
    engines
        .iter()
        .position(|next| *next == engine)
        .map_or(engines, |at| &engines[at + 1..])
}

impl Engine {
    /// Its name as an encoder, for the preview and the diagnostic line.
    pub fn encoder(self) -> &'static str {
        match self {
            Self::Software => "OpenH264",
            Self::VaApi => "VA-API",
            Self::Nvidia => "NVENC",
        }
    }

    /// Its name as a decoder.
    pub fn decoder(self) -> &'static str {
        match self {
            Self::Software => "OpenH264",
            Self::VaApi => "VA-API",
            Self::Nvidia => "NVDEC",
        }
    }
}

/// A GPU's encoder does not take a picture after this many wait in it, and
/// one that keeps a picture this long failed; the first may take longer, as
/// a GPU that slept wakes up (NVIDIA's on a laptop).
const IN_FLIGHT: usize = 2;
const STUCK: Duration = Duration::from_secs(1);
const WAKING: Duration = Duration::from_secs(5);
/// Units waiting in a decoder before it counts as behind; while it starts
/// (its first [`WARM_UP`] pictures: NVDEC takes a moment), what waited for
/// it may drain first, up to [`STARTING_QUEUE`].
const DECODE_QUEUE: u64 = 4;
const STARTING_QUEUE: u64 = 120;
const WARM_UP: u64 = 30;
/// A key frame is forced at most this often while one is wanted: the one
/// asked for may still be on its way.
const FORCE_GAP: Duration = Duration::from_millis(500);
/// What every encoder's output goes through: parameter sets before each key
/// frame, whole access units in Annex B, Constrained Baseline as OpenH264's.
const H264_OUT: &str = "h264parse config-interval=-1 ! \
     video/x-h264,stream-format=byte-stream,alignment=au,profile=constrained-baseline ! \
     appsink name=sink sync=false enable-last-sample=false";

/// How every decoder runs: each picture out as soon as it is decoded (no
/// sender of a call reorders them), and none whose references were lost:
/// as with OpenH264, nothing until the next key frame.
const LIVE: &str = "compliance=flexible discard-corrupted-frames=true";

/// GStreamer, once, with the plugins `shell.nix` names; `false` without it.
pub(crate) fn ready() -> bool {
    static READY: OnceLock<bool> = OnceLock::new();
    *READY.get_or_init(|| {
        if gst::init().is_err() {
            return false;
        }
        let registry = gst::Registry::get();
        if let Some(paths) = option_env!("RUNE_GST_PLUGINS") {
            for path in std::env::split_paths(paths) {
                registry.scan_path(path);
            }
        }
        // The registry is shared with every GStreamer program of the account:
        // one that scanned these plugins where it could not reach the GPU (a
        // sandboxed web view) left them without elements. Loaded again here.
        for name in ["va", "nvcodec"] {
            let plugin = registry.find_plugin(name);
            if let Some(file) = plugin
                .filter(|_| registry.features_by_plugin(name).is_empty())
                .and_then(|plugin| plugin.filename())
            {
                let _ = gst::Plugin::load_file(file);
            }
        }
        true
    })
}

/// The VA-API elements named `va{device}{kind}`, NVIDIA's left out (NVENC
/// and NVDEC reach that GPU), as their device's prefix and their name: `va`
/// is the first device, `varenderD129` another.
fn va_elements(kinds: &[&str]) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = gst::Registry::get()
        .features(gst::ElementFactory::static_type())
        .into_iter()
        .filter_map(|feature| feature.downcast::<gst::ElementFactory>().ok())
        .filter(|factory| {
            let long = factory.longname().to_lowercase();
            !long.contains("nvidia") && !long.contains("nvdec")
        })
        .filter_map(|factory| {
            let name = factory.name().to_string();
            let device = kinds
                .iter()
                .find_map(|kind| name.strip_prefix("va")?.strip_suffix(kind))?
                .to_owned();
            Some((format!("va{device}"), name))
        })
        .collect();
    // The first device, then the others; a full-power encoder before a low-power one.
    found.sort_by(|a, b| (a.0.len(), &a.0, a.1.len()).cmp(&(b.0.len(), &b.0, b.1.len())));
    found
}

/// Pipelines running now, on the threads of their encoders and decoders
/// (and of the videos played in conversations).
pub(crate) static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// Wait, at most `within`, until no pipeline runs: their threads end them
/// after their handles are dropped, and a driver torn down under one as
/// the process exits may crash it. For the end of `main`.
pub fn settle(within: Duration) {
    let until = Instant::now() + within;
    while RUNNING.load(Ordering::Acquire) > 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// One pipeline from `src` to `sink`, stopped when dropped. Its bus is never
/// read: an error or the end only sets `failed`.
struct Pipe {
    pipeline: gst::Pipeline,
    src: gst_app::AppSrc,
    sink: gst_app::AppSink,
    failed: Arc<AtomicBool>,
}

impl Pipe {
    fn start(description: &str, caps: &gst::Caps) -> Option<Self> {
        let pipeline = gst::parse::launch(description)
            .ok()?
            .downcast::<gst::Pipeline>()
            .ok()?;
        let failed = Arc::new(AtomicBool::new(false));
        let flag = failed.clone();
        pipeline.bus()?.set_sync_handler(move |_, message| {
            if matches!(
                message.view(),
                gst::MessageView::Error(_) | gst::MessageView::Eos(_)
            ) {
                flag.store(true, Ordering::Release);
            }
            gst::BusSyncReply::Drop
        });
        let src = pipeline
            .by_name("src")?
            .downcast::<gst_app::AppSrc>()
            .ok()?;
        let sink = pipeline
            .by_name("sink")?
            .downcast::<gst_app::AppSink>()
            .ok()?;
        src.set_caps(Some(caps));
        src.set_is_live(true);
        src.set_format(gst::Format::Time);
        src.set_stream_type(gst_app::AppStreamType::Stream);
        RUNNING.fetch_add(1, Ordering::AcqRel);
        let pipe = Self {
            pipeline,
            src,
            sink,
            failed,
        };
        pipe.pipeline.set_state(gst::State::Playing).ok()?;
        Some(pipe)
    }

    fn failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    fn push(&self, buffer: gst::Buffer) -> bool {
        !self.failed() && self.src.push_buffer(buffer).is_ok()
    }

    /// The next sample out, without waiting.
    fn pull(&self) -> Option<gst::Sample> {
        self.sink.try_pull_sample(gst::ClockTime::ZERO)
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        RUNNING.fetch_sub(1, Ordering::AcqRel);
    }
}

/// A frame's pixels lent to GStreamer without a copy.
struct Pixels(Arc<Frame>);

impl AsRef<[u8]> for Pixels {
    fn as_ref(&self) -> &[u8] {
        &self.0.rgba
    }
}

/// `frame` at an even size, as H264 pictures are: as is when it already is,
/// its last column or row left out otherwise. `None` when nothing is left.
fn even(frame: &Arc<Frame>) -> Option<((u32, u32), gst::Buffer)> {
    let (width, height) = (frame.width & !1, frame.height & !1);
    let row = frame.width as usize * 4;
    if width == 0 || height == 0 || frame.rgba.len() < row * frame.height as usize {
        return None;
    }
    if (width, height) == (frame.width, frame.height) {
        return Some((
            (width, height),
            gst::Buffer::from_slice(Pixels(frame.clone())),
        ));
    }
    let even: Vec<u8> = frame
        .rgba
        .chunks_exact(row)
        .take(height as usize)
        .flat_map(|line| &line[..width as usize * 4])
        .copied()
        .collect();
    Some(((width, height), gst::Buffer::from_mut_slice(even)))
}

/// One GPU encoder at one picture size.
pub(crate) struct Encoder {
    pipe: Pipe,
    pub(crate) engine: Engine,
    pub(crate) size: (u32, u32),
    epoch: Instant,
    /// The pictures in it, oldest first: their time stamp and when they went in.
    in_flight: VecDeque<(u64, Instant)>,
    forced: Option<Instant>,
    /// Whether a unit came out yet.
    gave: bool,
}

/// What came out of an encoder: a unit and how long its picture took.
pub(crate) type Made = (Unit, Duration);

impl Encoder {
    /// `engine`'s encoder for pictures of `size`, at most `fps` a second,
    /// at `bitrate`; `None` when it is not there or does not start.
    pub(crate) fn start(engine: Engine, size: (u32, u32), fps: u32, bitrate: u32) -> Option<Self> {
        if engine == Engine::Software || !ready() {
            return None;
        }
        let caps = gst_video::VideoInfo::builder(gst_video::VideoFormat::Rgba, size.0, size.1)
            .fps(gst::Fraction::new(fps.clamp(1, 240) as i32, 1))
            .build()
            .ok()?
            .to_caps()
            .ok()?;
        let kbps = (bitrate / 1000).max(1);
        let pipe =
            match engine {
                // Variable rate at the target as the most: VA-API's constant rate
                // pads a still screen to the full rate. No B-frames; a key frame
                // on its own at most every 1024 pictures (VA-API's longest GOP).
                Engine::VaApi => va_elements(&["h264enc", "h264lpenc"]).into_iter().find_map(
                    |(device, element)| {
                        Pipe::start(
                            &format!(
                                "appsrc name=src ! {device}postproc ! \
                             video/x-raw(memory:VAMemory),format=NV12 ! \
                             {element} name=encoder rate-control=vbr target-percentage=100 \
                             bitrate={kbps} key-int-max=1024 b-frames=0 ! {H264_OUT}"
                            ),
                            &caps,
                        )
                    },
                ),
                // NVENC takes RGBA as it is and converts on the GPU.
                Engine::Nvidia => Pipe::start(
                    &format!(
                        "appsrc name=src ! nvh264enc name=encoder rc-mode=cbr bitrate={kbps} \
                     gop-size=-1 bframes=0 rc-lookahead=0 zerolatency=true aud=false \
                     repeat-sequence-header=true ! {H264_OUT}"
                    ),
                    &caps,
                ),
                Engine::Software => None,
            }?;
        Some(Self {
            pipe,
            engine,
            size,
            epoch: Instant::now(),
            in_flight: VecDeque::new(),
            forced: None,
            gave: false,
        })
    }

    /// A new bit rate for what follows, in place: both take it while running.
    pub(crate) fn set_bitrate(&self, bitrate: u32) {
        if let Some(encoder) = self.pipe.pipeline.by_name("encoder") {
            encoder.set_property("bitrate", (bitrate / 1000).max(1));
        }
    }

    /// Whether it holds as many pictures as it takes.
    pub(crate) fn busy(&self) -> bool {
        self.in_flight.len() >= IN_FLIGHT
    }

    /// Encode `frame`, taken `at`, a key frame when `key`. `false` when the
    /// encoder failed.
    pub(crate) fn push(&mut self, frame: &Arc<Frame>, key: bool, at: Instant) -> bool {
        let Some((size, mut buffer)) = even(frame) else {
            return true;
        };
        if size != self.size {
            return false;
        }
        if key
            && self
                .forced
                .is_none_or(|forced| forced.elapsed() >= FORCE_GAP)
        {
            let event = gst_video::UpstreamForceKeyUnitEvent::builder()
                .all_headers(true)
                .build();
            let sent = self
                .pipe
                .pipeline
                .by_name("encoder")
                .and_then(|encoder| encoder.static_pad("src"))
                .is_some_and(|pad| pad.send_event(event));
            self.forced = sent.then(Instant::now);
        }
        let pts = at.saturating_duration_since(self.epoch).as_nanos() as u64;
        if let Some(buffer) = buffer.get_mut() {
            buffer.set_pts(gst::ClockTime::from_nseconds(pts));
        }
        self.in_flight.push_back((pts, Instant::now()));
        self.pipe.push(buffer)
    }

    /// The next unit out, if one is; `Err` when the encoder failed or
    /// stopped giving them back.
    pub(crate) fn pull(&mut self) -> Result<Option<Made>, ()> {
        let patience = if self.gave { STUCK } else { WAKING };
        let stuck = self
            .in_flight
            .front()
            .is_some_and(|(_, pushed)| pushed.elapsed() > patience);
        if self.pipe.failed() || stuck {
            return Err(());
        }
        let Some(sample) = self.pipe.pull() else {
            return Ok(None);
        };
        let buffer = sample.buffer().ok_or(())?;
        let pts = buffer.pts().map_or(0, gst::ClockTime::nseconds);
        let mut took = Duration::ZERO;
        // Pictures before it that never came out are let go of with it.
        while let Some(&(pushed_pts, pushed)) = self.in_flight.front() {
            if pushed_pts > pts {
                break;
            }
            took = pushed.elapsed();
            self.in_flight.pop_front();
        }
        let keyframe = !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT);
        if keyframe {
            self.forced = None;
        }
        self.gave = true;
        let data = buffer.map_readable().map_err(|_| ())?.to_vec();
        Ok(Some((
            Unit {
                keyframe,
                at: self.epoch + Duration::from_nanos(pts),
                data,
            },
            took,
        )))
    }
}

/// One GPU decoder.
pub(crate) struct Decoder {
    pipe: Pipe,
    pub(crate) engine: Engine,
    /// Pictures it gave back so far.
    pub(crate) pictures: u64,
}

impl Decoder {
    /// `engine`'s decoder, RGBA out; `None` when it is not there or does not start.
    pub(crate) fn start(engine: Engine) -> Option<Self> {
        if engine == Engine::Software || !ready() {
            return None;
        }
        let caps = gst::Caps::builder("video/x-h264")
            .field("stream-format", "byte-stream")
            .field("alignment", "au")
            .build();
        let pipe = match engine {
            Engine::VaApi => va_elements(&["h264dec"])
                .into_iter()
                .find_map(|(device, element)| {
                    Pipe::start(
                        &format!(
                            "appsrc name=src ! h264parse ! {element} {LIVE} ! \
                             {device}postproc ! video/x-raw,format=RGBA ! \
                             appsink name=sink sync=false enable-last-sample=false"
                        ),
                        &caps,
                    )
                }),
            // NVDEC gives NV12 back; turned into RGBA on the CPU.
            Engine::Nvidia => Pipe::start(
                &format!(
                    "appsrc name=src ! h264parse ! nvh264dec {LIVE} max-display-delay=0 ! \
                     videoconvert ! video/x-raw,format=RGBA ! \
                     appsink name=sink sync=false enable-last-sample=false"
                ),
                &caps,
            ),
            Engine::Software => None,
        }?;
        Some(Self {
            pipe,
            engine,
            pictures: 0,
        })
    }

    /// Whether units wait in it already: it, or the GPU, is behind.
    pub(crate) fn behind(&self) -> bool {
        let most = if self.pictures >= WARM_UP {
            DECODE_QUEUE
        } else {
            STARTING_QUEUE
        };
        self.pipe.src.property::<u64>("current-level-buffers") >= most
    }

    /// Decode `unit`; `false` when the decoder failed.
    pub(crate) fn push(&mut self, unit: Vec<u8>) -> bool {
        self.pipe
            .push(gst::Buffer::from_mut_slice(super::sps::roomy(unit)))
    }

    /// The next picture out, if one is; `Err` when the decoder failed.
    pub(crate) fn pull(&mut self) -> Result<Option<Frame>, ()> {
        if self.pipe.failed() {
            return Err(());
        }
        let Some(sample) = self.pipe.pull() else {
            return Ok(None);
        };
        let frame = picture(&sample).ok_or(())?;
        self.pictures += 1;
        Ok(Some(frame))
    }
}

/// An RGBA sample from an `appsink` as a frame within
/// [`super::decode::STREAM_MAX`].
pub(crate) fn picture(sample: &gst::Sample) -> Option<Frame> {
    let info = sample
        .caps()
        .and_then(|caps| gst_video::VideoInfo::from_caps(caps).ok())?;
    let buffer = sample.buffer()?;
    let picture = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info).ok()?;
    let plane = super::Plane {
        bytes: picture.plane_data(0).ok()?,
        stride: usize::try_from(picture.plane_stride()[0]).ok()?,
    };
    super::decode::within(info.width(), info.height(), plane)
}
