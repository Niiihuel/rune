//! The picture of a new emoji or sticker, chosen by the person: one image
//! file through the system's selector, read with a bound and made what the
//! kind asks for on a service thread. A GIF that already fits stays as it
//! is, animation and all; anything else becomes a square PNG of the kind's
//! size. The interface gets it and a preview; it reaches the provider only
//! inside the command that adds the expression, when the person confirms.

use std::io::Cursor;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use bytes::Bytes;
use image::{imageops::FilterType, ImageFormat};
use rune_i18n::t;
use rune_provider::{ExpressionImage, ExpressionKind};
use sha2::{Digest, Sha256};
use tokio::{runtime::Handle, sync::watch, task::JoinHandle};

use crate::group_icons::{chosen_bytes, decode};
use crate::ImageData;

/// What came of choosing a picture asked for with `request`.
pub struct PickedExpressionImage {
    pub request: u64,
    pub kind: ExpressionKind,
    /// The picture and its preview; `Ok(None)` when the person cancelled.
    pub result: Result<Option<(ExpressionImage, Arc<ImageData>)>, &'static str>,
}

pub(crate) struct ExpressionImages {
    runtime: Handle,
    wake: Arc<dyn Fn() + Send + Sync>,
    choosing: Option<(JoinHandle<()>, watch::Sender<bool>)>,
    done: Arc<Mutex<Vec<PickedExpressionImage>>>,
}

impl ExpressionImages {
    pub fn new(runtime: Handle, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            runtime,
            wake,
            choosing: None,
            done: Arc::default(),
        }
    }

    /// Let the person choose a picture of `kind`, the dialog over `parent`;
    /// one choice at a time.
    pub fn pick(
        &mut self,
        request: u64,
        kind: ExpressionKind,
        parent: Option<crate::ParentWindow>,
    ) {
        if self
            .choosing
            .as_ref()
            .is_some_and(|(task, _)| !task.is_finished())
        {
            if let Ok(mut done) = self.done.lock() {
                done.push(PickedExpressionImage {
                    request,
                    kind,
                    result: Err(t("Ya se está eligiendo una imagen.")),
                });
            }
            (self.wake)();
            return;
        }
        let (cancel, cancelled) = watch::channel(false);
        let (done, wake) = (self.done.clone(), self.wake.clone());
        let task = self.runtime.spawn(async move {
            let result = match chosen_bytes(cancelled, parent).await {
                Ok(Some(bytes)) => tokio::task::spawn_blocking(move || prepare(&bytes, kind))
                    .await
                    .unwrap_or(Err(t("No se pudo preparar la imagen.")))
                    .map(Some),
                Ok(None) => Ok(None),
                Err(reason) => Err(reason),
            };
            if let Ok(mut done) = done.lock() {
                done.push(PickedExpressionImage {
                    request,
                    kind,
                    result,
                });
            }
            wake();
        });
        self.choosing = Some((task, cancel));
    }

    /// Stop a choice under way: what it finds is dropped.
    pub fn cancel(&mut self) {
        if let Some((task, cancel)) = self.choosing.take() {
            cancel.send_replace(true);
            task.abort();
        }
    }

    /// What was chosen since the last call.
    pub fn poll(&mut self) -> Vec<PickedExpressionImage> {
        self.done
            .lock()
            .map(|mut done| std::mem::take(&mut *done))
            .unwrap_or_default()
    }
}

impl Drop for ExpressionImages {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// `bytes`, a PNG, JPEG, GIF or WebP picture, as the picture of a new
/// expression of `kind`, with its preview. It blocks: off the interface's
/// thread, but for a small local picture.
pub fn prepare(
    bytes: &[u8],
    kind: ExpressionKind,
) -> Result<(ExpressionImage, Arc<ImageData>), &'static str> {
    let decoded = decode(bytes)?;
    // A GIF that fits as it is keeps moving; the preview is its first frame.
    let kept = ExpressionImage::gif(kind, Bytes::copy_from_slice(bytes));
    let edge = kind.edge();
    let square = decoded.resize_to_fill(edge, edge, FilterType::Triangle);
    let mut png = Cursor::new(Vec::new());
    square
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|_| t("No se pudo preparar la imagen."))?;
    let png = png.into_inner();
    let preview = Arc::new(ImageData {
        sha256: format!("{:x}", Sha256::digest(&png)),
        size_bytes: png.len() as u32,
        width: edge,
        height: edge,
        base64: Arc::from(base64::engine::general_purpose::STANDARD.encode(&png)),
        animation: None,
        format: crate::Encoding::Png,
    });
    let image = match kept {
        Some(gif) => gif,
        None => ExpressionImage::png(kind, Bytes::from(png))
            .ok_or(t("La imagen pesa más de lo que admite el servidor."))?,
    };
    Ok((image, preview))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let picture = image::RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255])
        });
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(picture)
            .write_to(&mut bytes, format)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn a_picture_becomes_the_kind_s_square_and_a_small_gif_stays_a_gif() {
        let photo = encoded(900, 600, ImageFormat::Jpeg);
        let (emoji, preview) = prepare(&photo, ExpressionKind::Emoji).unwrap();
        assert!(!emoji.is_gif());
        assert_eq!((preview.width, preview.height), (128, 128));
        let (sticker, _) = prepare(&photo, ExpressionKind::Sticker).unwrap();
        assert!(!sticker.is_gif());
        assert!(sticker.bytes().len() <= ExpressionKind::Sticker.max_bytes());
        let gif = encoded(64, 48, ImageFormat::Gif);
        let (kept, preview) = prepare(&gif, ExpressionKind::Emoji).unwrap();
        assert!(kept.is_gif());
        assert_eq!(kept.bytes(), gif.as_slice());
        assert_eq!(preview.width, 128);
        // A sticker GIF must already be its size: else it becomes a PNG.
        let (made, _) = prepare(&gif, ExpressionKind::Sticker).unwrap();
        assert!(!made.is_gif());
        assert_eq!(
            prepare(b"%PDF-1.7", ExpressionKind::Emoji).unwrap_err(),
            "Elige una imagen PNG, JPEG, GIF o WebP."
        );
    }
}
