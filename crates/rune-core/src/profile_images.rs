//! A new picture for the account's profile, chosen by the person: one image
//! file through the system's selector, read with a bound, framed to the
//! shape of an avatar or a banner and made a PNG on a service thread, or a
//! GIF when it moves and may stay moving. The file is kept so the person
//! can frame it again ([`ProfileImages::frame`]). The interface gets the
//! result and a preview of it; it reaches the provider only inside the
//! profile's command, when the person saves.

use std::io::{Cursor, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use bytes::Bytes;
use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::codecs::webp::WebPDecoder;
use image::{imageops::FilterType, AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat};
use rune_i18n::t;
use rune_provider::{Crop, ProfileImage, ProfileImageKind};
use sha2::{Digest, Sha256};
use tokio::{runtime::Handle, sync::watch, task::JoinHandle};

use crate::group_icons::{chosen_bytes, decode};
use crate::ImageData;

/// The longer side of the whole picture shown for framing.
const SOURCE_SIDE: u32 = 480;
/// A moving picture's canvas is decoded once per frame: past this many
/// pixels, it is framed as a still picture.
const MAX_CANVAS_PIXELS: u64 = 4 * 1024 * 1024;
/// Frames read from a moving picture, and how long making one may take,
/// before it is refused.
const MAX_FRAMES: usize = 1000;
const MAX_ENCODE_TIME: Duration = Duration::from_secs(20);
/// How finely the GIF's colors are chosen: 1 is the finest, 30 the fastest.
const GIF_SPEED: i32 = 10;

/// What came of choosing a picture asked for with `request`, or of framing
/// it again.
pub struct PickedProfileImage {
    pub request: u64,
    pub kind: ProfileImageKind,
    /// The picture and its preview; `Ok(None)` when the person cancelled.
    pub result: Result<Option<(ProfileImage, Arc<ImageData>)>, &'static str>,
    /// The whole picture of a new choice, to frame it; `None` for a frame.
    pub source: Option<PictureSource>,
}

/// The whole of a chosen picture, small, for the person to frame it.
pub struct PictureSource {
    /// Of the picture's shape, at most [`SOURCE_SIDE`] a side.
    pub preview: Arc<ImageData>,
    /// Whether it moves: a frame of it may keep moving.
    pub animated: bool,
}

pub(crate) struct ProfileImages {
    runtime: Handle,
    wake: Arc<dyn Fn() + Send + Sync>,
    /// The choice under way, and how to stop it.
    choosing: Option<(JoinHandle<()>, watch::Sender<bool>)>,
    done: Arc<Mutex<Vec<PickedProfileImage>>>,
    /// The file last chosen, to frame it again.
    // shortcut: up to 8 MB stay in memory until the next choice; forget it
    // when the editor closes if that ever matters.
    chosen: Arc<Mutex<Option<Arc<[u8]>>>>,
    /// The newest request: an older frame stops early and is dropped.
    latest: Arc<AtomicU64>,
}

impl ProfileImages {
    pub fn new(runtime: Handle, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            runtime,
            wake,
            choosing: None,
            done: Arc::default(),
            chosen: Arc::default(),
            latest: Arc::default(),
        }
    }

    /// Let the person choose a picture of `kind`, the dialog over `parent`;
    /// one choice at a time. A moving one stays moving when `animated`
    /// allows it.
    pub fn pick(
        &mut self,
        request: u64,
        kind: ProfileImageKind,
        animated: bool,
        parent: Option<crate::ParentWindow>,
    ) {
        if self
            .choosing
            .as_ref()
            .is_some_and(|(task, _)| !task.is_finished())
        {
            self.finish(PickedProfileImage {
                request,
                kind,
                result: Err(t("Ya se está eligiendo una imagen.")),
                source: None,
            });
            (self.wake)();
            return;
        }
        self.latest.store(request, Ordering::Relaxed);
        let (cancel, cancelled) = watch::channel(false);
        let (done, wake, chosen) = (self.done.clone(), self.wake.clone(), self.chosen.clone());
        let task = self.runtime.spawn(async move {
            let (result, source) = match chosen_bytes(cancelled, parent).await {
                Ok(Some(bytes)) => {
                    let bytes: Arc<[u8]> = Arc::from(bytes);
                    let read = bytes.clone();
                    let made =
                        tokio::task::spawn_blocking(move || first_frame(&read, kind, animated))
                            .await
                            .unwrap_or(Err(t("No se pudo preparar la imagen.")));
                    match made {
                        Ok((picture, source)) => {
                            if let Ok(mut chosen) = chosen.lock() {
                                *chosen = Some(bytes);
                            }
                            (Ok(Some(picture)), Some(source))
                        }
                        Err(reason) => (Err(reason), None),
                    }
                }
                Ok(None) => (Ok(None), None),
                Err(reason) => (Err(reason), None),
            };
            if let Ok(mut done) = done.lock() {
                done.push(PickedProfileImage {
                    request,
                    kind,
                    result,
                    source,
                });
            }
            wake();
        });
        self.choosing = Some((task, cancel));
    }

    /// Frame the picture last chosen again, as `crop` says; it comes back as
    /// a choice does, without its source. A newer request drops this one.
    pub fn frame(&mut self, request: u64, kind: ProfileImageKind, crop: Crop, animated: bool) {
        self.latest.store(request, Ordering::Relaxed);
        let Some(bytes) = self.chosen.lock().ok().and_then(|chosen| chosen.clone()) else {
            self.finish(PickedProfileImage {
                request,
                kind,
                result: Err(t("Elegí la imagen otra vez.")),
                source: None,
            });
            (self.wake)();
            return;
        };
        let (done, wake, latest) = (self.done.clone(), self.wake.clone(), self.latest.clone());
        self.runtime.spawn(async move {
            let current = latest.clone();
            let superseded = move || current.load(Ordering::Relaxed) != request;
            let result = tokio::task::spawn_blocking(move || {
                let decoded = decode(&bytes)?;
                let moving = animated.then(|| moving(&bytes)).flatten();
                framed(&decoded, &bytes, kind, crop, moving, &superseded)
            })
            .await
            .unwrap_or(Err(t("No se pudo preparar la imagen.")));
            if latest.load(Ordering::Relaxed) != request {
                return;
            }
            if let Ok(mut done) = done.lock() {
                done.push(PickedProfileImage {
                    request,
                    kind,
                    result: result.map(Some),
                    source: None,
                });
            }
            wake();
        });
    }

    /// Stop a choice under way: what it finds is dropped.
    pub fn cancel(&mut self) {
        if let Some((task, cancel)) = self.choosing.take() {
            cancel.send_replace(true);
            task.abort();
        }
    }

    /// What was chosen since the last call.
    pub fn poll(&mut self) -> Vec<PickedProfileImage> {
        self.done
            .lock()
            .map(|mut done| std::mem::take(&mut *done))
            .unwrap_or_default()
    }

    fn finish(&self, picked: PickedProfileImage) {
        if let Ok(mut done) = self.done.lock() {
            done.push(picked);
        }
    }
}

impl Drop for ProfileImages {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// `bytes`, a PNG, JPEG, GIF or WebP picture, as a profile's `kind` of
/// picture, framed as `crop` says: a PNG, the largest of its sizes that
/// fits, or a GIF of the same frame of every picture when it moves and
/// `animated` allows it. With its preview, the PNG of the first picture. It
/// blocks: off the interface's thread, but for a small local picture.
pub fn prepare(
    bytes: &[u8],
    kind: ProfileImageKind,
    crop: Crop,
    animated: bool,
) -> Result<(ProfileImage, Arc<ImageData>), &'static str> {
    let decoded = decode(bytes)?;
    let moving = animated.then(|| moving(bytes)).flatten();
    framed(&decoded, bytes, kind, crop, moving, &|| false)
}

/// The whole of `bytes`, a picture, small, for the person to frame it.
pub fn source(bytes: &[u8]) -> Result<PictureSource, &'static str> {
    source_of(&decode(bytes)?, moving(bytes))
}

fn source_of(
    decoded: &DynamicImage,
    moving: Option<ImageFormat>,
) -> Result<PictureSource, &'static str> {
    let whole = decoded.resize(SOURCE_SIDE, SOURCE_SIDE, FilterType::Triangle);
    Ok(PictureSource {
        preview: still(&whole)?.1,
        animated: moving.is_some(),
    })
}

/// A new choice: the picture framed in the middle, and the whole of it.
fn first_frame(
    bytes: &[u8],
    kind: ProfileImageKind,
    animated: bool,
) -> Result<((ProfileImage, Arc<ImageData>), PictureSource), &'static str> {
    let decoded = decode(bytes)?;
    let moving = moving(bytes);
    let source = source_of(&decoded, moving)?;
    let kept = moving.filter(|_| animated);
    let picture = framed(&decoded, bytes, kind, Crop::default(), kept, &|| false)?;
    Ok((picture, source))
}

/// `decoded`, the first picture of `bytes`, framed; every picture when
/// `moving` names the format they are read in, or still, its first one,
/// when they are too many, too slow or too heavy to move. Every file is
/// made here, so nothing of the one chosen (comments, authors, software)
/// leaves with it.
fn framed(
    decoded: &DynamicImage,
    bytes: &[u8],
    kind: ProfileImageKind,
    crop: Crop,
    moving: Option<ImageFormat>,
    superseded: &dyn Fn() -> bool,
) -> Result<(ProfileImage, Arc<ImageData>), &'static str> {
    let (width, height) = (decoded.width(), decoded.height());
    let frame = pixels(crop, width, height, kind);
    let [x, y, w, h] = frame;
    let part = decoded.crop_imm(x, y, w, h);
    if let Some(format) = moving {
        let (wide, high) = kind.sizes()[0];
        let (_, preview) = still(&part.resize_exact(wide, high, FilterType::Triangle))?;
        // Counted first: past the bound, nothing is made only to be dropped.
        let few = frames(bytes, format)
            .is_some_and(|frames| frames.take(MAX_FRAMES + 1).count() <= MAX_FRAMES);
        for &size in kind.sizes().iter().filter(|_| few) {
            if superseded() {
                break;
            }
            // Each size has its own time: a smaller one is not left the rest.
            let started = Instant::now();
            let gif = animation(bytes, format, frame, size, &|| {
                superseded() || started.elapsed() > MAX_ENCODE_TIME
            });
            if let Some(image) = gif.and_then(|gif| ProfileImage::new(Bytes::from(gif))) {
                return Ok((image, preview));
            }
        }
    }
    for &(width, height) in kind.sizes() {
        let (png, preview) = still(&part.resize_exact(width, height, FilterType::Triangle))?;
        if let Some(image) = ProfileImage::new(Bytes::from(png)) {
            return Ok((image, preview));
        }
    }
    Err(t("La imagen no entra en el tamaño de un perfil."))
}

/// `crop`'s frame in a `width` × `height` picture, in whole pixels within it.
fn pixels(crop: Crop, width: u32, height: u32, kind: ProfileImageKind) -> [u32; 4] {
    let [left, top, wide, high] = crop.frame(width as f32, height as f32, kind);
    let x = (left.round() as u32).min(width - 1);
    let y = (top.round() as u32).min(height - 1);
    [
        x,
        y,
        (wide.round() as u32).clamp(1, width - x),
        (high.round() as u32).clamp(1, height - y),
    ]
}

/// The format `bytes` are read in when they hold a moving GIF or WebP of
/// at least two pictures on a bounded canvas.
fn moving(bytes: &[u8]) -> Option<ImageFormat> {
    let format = image::guess_format(bytes).ok()?;
    let frames = frames(bytes, format)?;
    (frames.take(2).filter(Result::is_ok).count() == 2).then_some(format)
}

/// The pictures of a moving GIF or WebP, each the whole canvas.
fn frames(bytes: &[u8], format: ImageFormat) -> Option<image::Frames<'_>> {
    let bounded =
        |(width, height): (u32, u32)| u64::from(width) * u64::from(height) <= MAX_CANVAS_PIXELS;
    match format {
        ImageFormat::Gif => {
            let decoder = GifDecoder::new(Cursor::new(bytes)).ok()?;
            bounded(decoder.dimensions()).then(|| decoder.into_frames())
        }
        ImageFormat::WebP => {
            let decoder = WebPDecoder::new(Cursor::new(bytes)).ok()?;
            (decoder.has_animation() && bounded(decoder.dimensions()))
                .then(|| decoder.into_frames())
        }
        _ => None,
    }
}

/// Every picture of `bytes` framed by `[x, y, w, h]` and made `size`, as
/// one GIF that loops; `None` past the bounds or once `stop` says.
fn animation(
    bytes: &[u8],
    format: ImageFormat,
    [x, y, w, h]: [u32; 4],
    (width, height): (u32, u32),
    stop: &dyn Fn() -> bool,
) -> Option<Vec<u8>> {
    let mut gif = Capped(Vec::new());
    {
        let mut encoder = GifEncoder::new_with_speed(&mut gif, GIF_SPEED);
        encoder.set_repeat(Repeat::Infinite).ok()?;
        for (index, frame) in frames(bytes, format)?.take(MAX_FRAMES + 1).enumerate() {
            if index == MAX_FRAMES || stop() {
                return None;
            }
            let frame = frame.ok()?;
            let delay = frame.delay();
            let canvas = frame.into_buffer();
            let part = image::imageops::crop_imm(&canvas, x, y, w, h).to_image();
            let sized = image::imageops::resize(&part, width, height, FilterType::Triangle);
            encoder
                .encode_frame(image::Frame::from_parts(sized, 0, 0, delay))
                .ok()?;
        }
    }
    Some(gif.0)
}

/// A file that refuses to grow past a profile's moving picture.
struct Capped(Vec<u8>);

impl Write for Capped {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len() + bytes.len() > ProfileImage::MAX_ANIMATED_BYTES {
            return Err(std::io::Error::other("too large for a profile"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// `picture` as a PNG, and as a preview of it.
fn still(picture: &DynamicImage) -> Result<(Vec<u8>, Arc<ImageData>), &'static str> {
    let mut png = Cursor::new(Vec::new());
    picture
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|_| t("No se pudo preparar la imagen."))?;
    let png = png.into_inner();
    let preview = Arc::new(ImageData {
        sha256: format!("{:x}", Sha256::digest(&png)),
        size_bytes: png.len() as u32,
        width: picture.width(),
        height: picture.height(),
        base64: Arc::from(base64::engine::general_purpose::STANDARD.encode(&png)),
        animation: None,
        format: crate::Encoding::Png,
    });
    Ok((png, preview))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(width: u32, height: u32) -> Vec<u8> {
        let picture = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(picture)
            .write_to(&mut bytes, ImageFormat::Jpeg)
            .unwrap();
        bytes.into_inner()
    }

    /// A GIF of `frames` pictures, each a solid color, the left half red.
    fn moving_gif(width: u32, height: u32, frames: usize) -> Vec<u8> {
        let mut gif = Vec::new();
        {
            let mut encoder = GifEncoder::new_with_speed(&mut gif, 30);
            encoder.set_repeat(Repeat::Infinite).unwrap();
            for index in 0..frames {
                let picture = image::RgbaImage::from_fn(width, height, |x, _| {
                    if x < width / 2 {
                        image::Rgba([255, 0, 0, 255])
                    } else {
                        image::Rgba([0, (index.min(3) * 80) as u8, 255, 255])
                    }
                });
                let delay = image::Delay::from_numer_denom_ms(100, 1);
                encoder
                    .encode_frame(image::Frame::from_parts(picture, 0, 0, delay))
                    .unwrap();
            }
        }
        gif
    }

    fn frame_count(gif: &[u8]) -> usize {
        GifDecoder::new(Cursor::new(gif))
            .unwrap()
            .into_frames()
            .count()
    }

    #[test]
    fn a_picture_becomes_an_avatar_square_or_a_banner_strip_with_its_preview() {
        let photo = encoded(1200, 900);
        let (avatar, preview) =
            prepare(&photo, ProfileImageKind::Avatar, Crop::default(), true).unwrap();
        assert!(avatar.bytes().len() <= ProfileImage::MAX_BYTES && !avatar.animated());
        assert_eq!((preview.width, preview.height), (512, 512));
        assert_eq!(preview.size_bytes as usize, avatar.bytes().len());
        let (banner, preview) =
            prepare(&photo, ProfileImageKind::Banner, Crop::default(), false).unwrap();
        let (width, height) =
            image::ImageReader::with_format(Cursor::new(banner.bytes()), ImageFormat::Png)
                .into_dimensions()
                .unwrap();
        assert_eq!((width, height), (preview.width, preview.height));
        assert_eq!(width * 2, height * 5, "5:2");
        assert_eq!(
            prepare(
                b"%PDF-1.7",
                ProfileImageKind::Avatar,
                Crop::default(),
                false
            )
            .unwrap_err(),
            "Elegí una imagen PNG, JPEG, GIF o WebP."
        );
    }

    #[test]
    fn the_frame_chosen_is_the_part_kept() {
        // Red on the left half, blue on the right: framed at the left edge,
        // the avatar is red; at the right edge, blue.
        let picture = image::RgbImage::from_fn(400, 200, |x, _| {
            if x < 200 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            }
        });
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(picture)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let color = |x: f32| {
            let crop = Crop {
                x,
                ..Crop::default()
            };
            let (avatar, _) =
                prepare(png.get_ref(), ProfileImageKind::Avatar, crop, false).unwrap();
            let avatar = image::load_from_memory(avatar.bytes()).unwrap().to_rgb8();
            *avatar.get_pixel(256, 256)
        };
        assert_eq!(color(0.0), image::Rgb([255, 0, 0]));
        assert_eq!(color(1.0), image::Rgb([0, 0, 255]));
        assert_eq!(
            pixels(Crop::default(), 400, 200, ProfileImageKind::Avatar),
            [100, 0, 200, 200]
        );
    }

    #[test]
    fn a_moving_picture_stays_moving_only_when_allowed_and_framed_alike() {
        // Square and uncut, it is made again all the same.
        let square = moving_gif(64, 64, 3);
        let (avatar, preview) =
            prepare(&square, ProfileImageKind::Avatar, Crop::default(), true).unwrap();
        assert!(avatar.animated() && frame_count(avatar.bytes()) == 3);
        assert_eq!((preview.width, preview.height), (512, 512));
        // Not allowed: its first picture, still.
        let (still, _) =
            prepare(&square, ProfileImageKind::Avatar, Crop::default(), false).unwrap();
        assert!(!still.animated());
        // Framed on its left half, every picture is cut alike.
        let wide = moving_gif(64, 32, 3);
        let left = Crop {
            x: 0.0,
            ..Crop::default()
        };
        let (avatar, _) = prepare(&wide, ProfileImageKind::Avatar, left, true).unwrap();
        assert!(avatar.animated() && frame_count(avatar.bytes()) == 3);
        let frames: Vec<_> = GifDecoder::new(Cursor::new(avatar.bytes()))
            .unwrap()
            .into_frames()
            .map(|frame| frame.unwrap().into_buffer())
            .collect();
        assert!(frames.iter().all(|frame| frame.dimensions() == (512, 512)));
        // Red, give or take the GIF's palette.
        assert!(frames.iter().all(|frame| {
            let [red, green, blue, _] = frame.get_pixel(256, 256).0;
            red > 200 && green < 50 && blue < 50
        }));
        // A still picture never moves.
        assert!(moving(&encoded(64, 64)).is_none() && moving(&square).is_some());
    }

    #[test]
    fn a_moving_picture_past_its_bounds_keeps_its_first_picture_instead_of_failing() {
        let long = moving_gif(4, 4, MAX_FRAMES + 1);
        assert!(moving(&long).is_some());
        let ((avatar, preview), source) =
            first_frame(&long, ProfileImageKind::Avatar, true).unwrap();
        assert!(source.animated && !avatar.animated());
        assert_eq!((preview.width, preview.height), (512, 512));
    }

    #[test]
    fn nothing_of_the_file_chosen_leaves_with_a_moving_picture() {
        // A comment, right after the screen's description and its colors.
        let mut gif = moving_gif(64, 64, 2);
        let flags = gif[10];
        let colors = if flags & 0x80 != 0 {
            3 << ((flags & 7) + 1)
        } else {
            0
        };
        let at = 13 + colors;
        let comment = b"autora: Ana, C:/Users/ana/gato.gif";
        let rest = gif.split_off(at);
        gif.extend([0x21, 0xFE, comment.len() as u8]);
        gif.extend_from_slice(comment);
        gif.push(0);
        gif.extend(rest);
        assert!(moving(&gif).is_some(), "still a moving GIF");
        let (avatar, _) = prepare(&gif, ProfileImageKind::Avatar, Crop::default(), true).unwrap();
        assert!(avatar.animated());
        assert!(!avatar.bytes().windows(6).any(|part| part == b"autora"));
    }

    #[tokio::test]
    async fn a_newer_frame_drops_the_one_on_its_way_and_without_a_file_it_says_so() {
        let (wake, mut woken) = tokio::sync::mpsc::unbounded_channel();
        let mut images = ProfileImages::new(
            Handle::current(),
            Arc::new(move || {
                let _ = wake.send(());
            }),
        );
        let avatar = ProfileImageKind::Avatar;
        images.frame(1, avatar, Crop::default(), false);
        let [picked] = &images.poll()[..] else {
            panic!("one answer");
        };
        assert!(picked.request == 1 && matches!(picked.result, Err("Elegí la imagen otra vez.")));
        while woken.try_recv().is_ok() {}
        *images.chosen.lock().unwrap() = Some(Arc::from(encoded(400, 200)));
        let at = |x: f32| Crop {
            x,
            ..Crop::default()
        };
        // Both start once this test waits: the first finds itself replaced.
        images.frame(2, avatar, at(0.0), false);
        images.frame(3, avatar, at(1.0), false);
        tokio::time::timeout(Duration::from_secs(30), woken.recv())
            .await
            .unwrap();
        let [picked] = &images.poll()[..] else {
            panic!("only the newest");
        };
        assert!(picked.request == 3 && picked.source.is_none());
        assert!(matches!(&picked.result, Ok(Some((image, _))) if !image.animated()));
    }

    #[test]
    fn the_source_of_a_choice_is_small_and_says_whether_it_moves() {
        let (_, source) = first_frame(&encoded(1200, 600), ProfileImageKind::Banner, true).unwrap();
        assert_eq!((source.preview.width, source.preview.height), (480, 240));
        assert!(!source.animated);
        let ((avatar, _), source) =
            first_frame(&moving_gif(64, 64, 2), ProfileImageKind::Avatar, false).unwrap();
        assert!(
            source.animated && !avatar.animated(),
            "kept still: not allowed"
        );
    }
}
