//! Bounded animation frames for previews already normalized to their first frame.

use std::{
    io::Cursor,
    time::{Duration, Instant},
};

use image::{
    codecs::{gif::GifDecoder, png::PngDecoder, webp::WebPDecoder},
    AnimationDecoder, ImageDecoder, ImageFormat,
};
use rune_model::ImageKind;

/// Frames kept for one animation; more are merged by dropping every other one.
pub const MAX_FRAMES: usize = 120;
/// Straight RGBA bytes kept for one animation after downscaling.
pub const MAX_ANIMATION_BYTES: usize = 16 * 1024 * 1024;
/// Frames read from the file, kept or not, before the animation is refused.
const MAX_INPUT_FRAMES: usize = 1000;
/// A decoder canvas is allocated more than once per frame: keep it small.
const MAX_CANVAS_PIXELS: u64 = 4 * 1024 * 1024;
pub(super) const MAX_DECODE_TIME: Duration = Duration::from_secs(2);
/// A clip's pictures cost more to decode than a GIF's frames.
#[cfg(target_os = "linux")]
const MAX_CLIP_TIME: Duration = Duration::from_secs(3);
/// Browsers show near-zero GIF delays at 100 ms; doing the same keeps old
/// files from asking for a repaint every frame.
const SHORTEST_DELAY: Duration = Duration::from_millis(20);
const DEFAULT_DELAY: Duration = Duration::from_millis(100);

/// One displayed frame: straight-alpha RGBA rows of the animation's size.
pub struct AnimationFrame {
    pub delay: Duration,
    pub rgba: Vec<u8>,
}

/// Every frame has the same size, the size of the static preview.
pub struct Animation {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<AnimationFrame>,
}

impl std::fmt::Debug for Animation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Animation")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("frames", &self.frames.len())
            .finish()
    }
}

impl Animation {
    pub fn bytes(&self) -> usize {
        self.frames.iter().map(|frame| frame.rgba.len()).sum()
    }

    /// The frame shown `elapsed` after playback started, looping forever, and
    /// how long until the next one.
    pub fn frame_at(&self, elapsed: Duration) -> (usize, Duration) {
        let total: u128 = self.frames.iter().map(|f| f.delay.as_nanos()).sum();
        if total == 0 {
            return (0, DEFAULT_DELAY);
        }
        let mut left = Duration::from_nanos((elapsed.as_nanos() % total) as u64);
        for (index, frame) in self.frames.iter().enumerate() {
            if left < frame.delay {
                return (index, frame.delay - left);
            }
            left -= frame.delay;
        }
        (0, self.frames[0].delay)
    }
}

fn delay(frame: &image::Frame) -> Duration {
    let (numerator, denominator) = frame.delay().numer_denom_ms();
    let delay = Duration::from_millis(u64::from(numerator) / u64::from(denominator.max(1)));
    if delay < SHORTEST_DELAY {
        DEFAULT_DELAY
    } else {
        delay
    }
}

/// Frames kept as they are read, the frame rate halved past the budgets:
/// every delay is kept, so the animation lasts as long as the file says.
pub(super) struct Keeper {
    kept: Vec<AnimationFrame>,
    size: Option<(u32, u32)>,
    stride: usize,
    read: usize,
    started: Instant,
    within: Duration,
}

impl Keeper {
    pub(super) fn new(within: Duration) -> Self {
        Self {
            kept: Vec::new(),
            size: None,
            stride: 1,
            read: 0,
            started: Instant::now(),
            within,
        }
    }

    /// One more frame shown for `delay`; `frame` sizes it, only when kept.
    /// `None` refuses the animation.
    pub(super) fn push(
        &mut self,
        delay: Duration,
        frame: impl FnOnce() -> image::RgbaImage,
    ) -> Option<()> {
        if self.read >= MAX_INPUT_FRAMES || self.started.elapsed() > self.within {
            return None;
        }
        let index = self.read;
        self.read += 1;
        if !index.is_multiple_of(self.stride) {
            self.kept.last_mut()?.delay += delay;
            return Some(());
        }
        let rgba = frame();
        if *self.size.get_or_insert(rgba.dimensions()) != rgba.dimensions() {
            return None;
        }
        self.kept.push(AnimationFrame {
            delay,
            rgba: rgba.into_raw(),
        });
        while self.kept.len() > MAX_FRAMES
            || self
                .kept
                .iter()
                .map(|frame| frame.rgba.len())
                .sum::<usize>()
                > MAX_ANIMATION_BYTES
        {
            if self.kept.len() < 2 {
                return None;
            }
            let mut merged = Vec::with_capacity(self.kept.len().div_ceil(2));
            let mut frames = std::mem::take(&mut self.kept).into_iter();
            while let Some(mut first) = frames.next() {
                if let Some(second) = frames.next() {
                    first.delay += second.delay;
                }
                merged.push(first);
            }
            self.kept = merged;
            self.stride *= 2;
        }
        Some(())
    }

    pub(super) fn finish(self) -> Option<Animation> {
        let (width, height) = self.size?;
        (self.kept.len() >= 2).then_some(Animation {
            width,
            height,
            frames: self.kept,
        })
    }
}

/// `None` for a still picture or anything over the bounds above: the first
/// frame stays the preview. `fit` sizes a decoded canvas like that frame.
pub(super) fn decode(
    bytes: &[u8],
    format: ImageFormat,
    kind: ImageKind,
    limits: image::Limits,
    fit: impl Fn(image::DynamicImage, ImageKind) -> image::RgbaImage,
) -> Option<Animation> {
    let frames = match format {
        ImageFormat::Gif => {
            let mut decoder = GifDecoder::new(Cursor::new(bytes)).ok()?;
            let (width, height) = decoder.dimensions();
            if u64::from(width) * u64::from(height) > MAX_CANVAS_PIXELS {
                return None;
            }
            decoder.set_limits(limits).ok()?;
            decoder.into_frames()
        }
        ImageFormat::WebP => {
            let decoder = WebPDecoder::new(Cursor::new(bytes)).ok()?;
            // The WebP frame iterator ignores limits: check the canvas here.
            let (width, height) = decoder.dimensions();
            if !decoder.has_animation()
                || u64::from(width) * u64::from(height) > MAX_CANVAS_PIXELS
                || limits
                    .max_image_width
                    .is_some_and(|side| width > side || height > side)
            {
                return None;
            }
            decoder.into_frames()
        }
        // An animated PNG: an avatar decoration (P5) or a sticker.
        ImageFormat::Png => {
            let mut decoder = PngDecoder::new(Cursor::new(bytes)).ok()?;
            let (width, height) = decoder.dimensions();
            if !decoder.is_apng().ok()? || u64::from(width) * u64::from(height) > MAX_CANVAS_PIXELS
            {
                return None;
            }
            decoder.set_limits(limits).ok()?;
            decoder.apng().ok()?.into_frames()
        }
        _ => return None,
    };
    let mut keeper = Keeper::new(MAX_DECODE_TIME);
    for frame in frames {
        // A broken later frame refuses the animation, not the still preview.
        let frame = frame.ok()?;
        let delay = delay(&frame);
        keeper.push(delay, || {
            fit(image::DynamicImage::ImageRgba8(frame.into_buffer()), kind)
        })?;
    }
    keeper.finish()
}

/// A clip's first picture, sized by `fit`, and its pictures as an
/// animation when they keep within the budgets; `None` when it cannot be
/// read at all. Decoding stops past [`MAX_CLIP_TIME`].
#[cfg(target_os = "linux")]
pub(super) fn clip(
    bytes: &[u8],
    fit: impl Fn(image::DynamicImage) -> image::RgbaImage,
) -> Option<(image::RgbaImage, Option<Animation>)> {
    let picture = |frame: rune_media::video::Frame| {
        image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba)
            .map(|image| fit(image::DynamicImage::ImageRgba8(image)))
    };
    let mut keeper = Some(Keeper::new(MAX_CLIP_TIME));
    let mut first = None;
    // Each picture waits until the next says how long it shows.
    let mut held: Option<(Duration, rune_media::video::Frame)> = None;
    let length = rune_media::video::decode::clip_pictures(bytes, MAX_CLIP_TIME, |at, frame| {
        if first.is_none() {
            first = picture(frame.clone());
        }
        if let Some((shown, previous)) = held.replace((at, frame)) {
            let kept = keeper.as_mut().and_then(|keeper| {
                keeper.push(at.saturating_sub(shown), || {
                    picture(previous).unwrap_or_default()
                })
            });
            if kept.is_none() {
                keeper = None;
            }
        }
        // Past the budgets, only the first picture is still needed.
        keeper.is_some() || first.is_none()
    })
    .ok()?;
    if let (Some(keeper), Some((shown, last))) = (keeper.as_mut(), held) {
        let last_delay = length.saturating_sub(shown).max(SHORTEST_DELAY);
        if keeper
            .push(last_delay, || picture(last).unwrap_or_default())
            .is_none()
        {
            return Some((first?, None));
        }
    }
    Some((first?, keeper.and_then(Keeper::finish)))
}

#[cfg(not(target_os = "linux"))]
pub(super) fn clip(
    _: &[u8],
    _: impl Fn(image::DynamicImage) -> image::RgbaImage,
) -> Option<(image::RgbaImage, Option<Animation>)> {
    None
}
