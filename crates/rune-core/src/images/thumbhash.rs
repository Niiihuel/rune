//! ThumbHash: the few bytes a service sends for a picture still loading,
//! decoded into the small blurry picture shown in its place (Evan Wallace's
//! public format; reimplemented from its description, contenido-de-mensajes).

use std::f32::consts::PI;

use base64::Engine as _;
use rune_model::ImageKind;

use super::ImageData;

/// The longest hash read: a ThumbHash has at most 25 bytes.
const MAX_BYTES: usize = 48;

/// `text`, a ThumbHash in base64, as a normalized PNG at most 32 pixels on
/// its longer side; `None` when it is malformed or cut short.
pub(super) fn image(text: &str) -> Option<ImageData> {
    let text = text.trim_end_matches('=');
    if text.len() > MAX_BYTES.div_ceil(3) * 4 {
        return None;
    }
    let hash = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(text)
        .ok()?;
    let (width, height, rgba) = decode(&hash)?;
    super::still(
        image::RgbaImage::from_raw(width, height, rgba)?,
        None,
        ImageKind::ThumbHash,
    )
}

/// The picture `hash` describes as 8-bit RGBA rows, with its width and height.
pub(super) fn decode(hash: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let byte = |index: usize| hash.get(index).copied().map(u32::from);
    let header24 = byte(0)? | byte(1)? << 8 | byte(2)? << 16;
    let header16 = byte(3)? | byte(4)? << 8;
    let l_dc = (header24 & 63) as f32 / 63.0;
    let p_dc = ((header24 >> 6) & 63) as f32 / 31.5 - 1.0;
    let q_dc = ((header24 >> 12) & 63) as f32 / 31.5 - 1.0;
    let l_scale = ((header24 >> 18) & 31) as f32 / 31.0;
    let alpha = header24 >> 23 != 0;
    let p_scale = ((header16 >> 3) & 63) as f32 / 63.0;
    let q_scale = ((header16 >> 9) & 63) as f32 / 63.0;
    let landscape = header16 >> 15 != 0;
    // The longer side's count of luminance terms, fewer with alpha.
    let long = if alpha { 5 } else { 7 };
    let short = header16 & 7;
    let (rx, ry) = if landscape {
        (long, short)
    } else {
        (short, long)
    };
    let (lx, ly) = (rx.max(3) as usize, ry.max(3) as usize);
    let (a_dc, a_scale) = if alpha {
        let packed = byte(5)?;
        ((packed & 15) as f32 / 15.0, (packed >> 4) as f32 / 15.0)
    } else {
        (1.0, 0.0)
    };
    // The terms after the header, four bits each, low half first.
    let mut next = 0;
    let start = if alpha { 6 } else { 5 };
    let mut terms = |nx: usize, ny: usize, scale: f32| -> Option<Vec<f32>> {
        let mut terms = Vec::new();
        for cy in 0..ny {
            let mut cx = usize::from(cy == 0);
            while cx * ny < nx * (ny - cy) {
                let nibble = (byte(start + next / 2)? >> ((next & 1) * 4)) & 15;
                terms.push((nibble as f32 / 7.5 - 1.0) * scale);
                next += 1;
                cx += 1;
            }
        }
        Some(terms)
    };
    // Color is boosted a quarter, as the encoder quantized it.
    let l_ac = terms(lx, ly, l_scale)?;
    let p_ac = terms(3, 3, p_scale * 1.25)?;
    let q_ac = terms(3, 3, q_scale * 1.25)?;
    let a_ac = if alpha {
        terms(5, 5, a_scale)?
    } else {
        Vec::new()
    };
    // Its shape from the counts as written, before the minimum of three.
    let ratio = rx as f32 / ry as f32;
    let (width, height) = if ratio > 1.0 {
        (32.0, (32.0 / ratio).round())
    } else {
        ((32.0 * ratio).round(), 32.0)
    };
    let (width, height) = (width as usize, height as usize);
    if width == 0 || height == 0 {
        return None;
    }
    let least = if alpha { 5 } else { 3 };
    let mut fx = vec![0.0; lx.max(least)];
    let mut fy = vec![0.0; ly.max(least)];
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for (cy, term) in fy.iter_mut().enumerate() {
            *term = (PI / height as f32 * (y as f32 + 0.5) * cy as f32).cos();
        }
        for x in 0..width {
            for (cx, term) in fx.iter_mut().enumerate() {
                *term = (PI / width as f32 * (x as f32 + 0.5) * cx as f32).cos();
            }
            let l = l_dc + sum(&l_ac, lx, ly, &fx, &fy);
            let p = p_dc + sum(&p_ac, 3, 3, &fx, &fy);
            let q = q_dc + sum(&q_ac, 3, 3, &fx, &fy);
            let a = if alpha {
                a_dc + sum(&a_ac, 5, 5, &fx, &fy)
            } else {
                a_dc
            };
            let b = l - 2.0 / 3.0 * p;
            let r = (3.0 * l - b + q) / 2.0;
            let g = r - q;
            rgba.extend([r, g, b, a].map(|value| (255.0 * value.clamp(0.0, 1.0)) as u8));
        }
    }
    Some((width as u32, height as u32, rgba))
}

/// A channel's `terms` at one pixel, whose cosines are `fx` and `fy`: they
/// cover the triangle of an `nx` by `ny` grid the encoder kept, in its
/// order. A channel without terms adds nothing.
fn sum(terms: &[f32], nx: usize, ny: usize, fx: &[f32], fy: &[f32]) -> f32 {
    let mut terms = terms.iter();
    let mut total = 0.0;
    for (cy, fy) in fy.iter().enumerate().take(ny) {
        let mut cx = usize::from(cy == 0);
        while cx * ny < nx * (ny - cy) {
            total += terms.next().copied().unwrap_or(0.0) * fx[cx] * fy * 2.0;
            cx += 1;
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hashes of two drawn pictures and what the format's reference
    /// implementation (the `thumbhash` package, 0.1.1) decodes them into.
    #[test]
    fn hashes_decode_as_the_reference_decodes_them() {
        let cases = [
            // Landscape, opaque.
            (
                "oPcNHZpwd3dxeHiHh3eHh9jQDPh4",
                (32, 23),
                472_370,
                [[21, 16, 0, 255], [118, 116, 170, 255], [204, 200, 124, 255]],
            ),
            // Portrait, with transparent corners.
            (
                "XwiCAwIzkISK/neQiQepeDd4h3iAi7c=",
                (19, 32),
                264_894,
                [[129, 129, 117, 0], [130, 128, 117, 255], [135, 123, 117, 0]],
            ),
        ];
        for (text, (w, h), sum, pixels) in cases {
            let hash = base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(text.trim_end_matches('='))
                .unwrap();
            let (width, height, rgba) = decode(&hash).unwrap();
            assert_eq!((width, height), (w, h), "{text}");
            let at = |x: u32, y: u32| {
                let i = ((y * width + x) * 4) as usize;
                [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
            };
            // One step of rounding apart at most, as float math may differ.
            let near = |a: [u8; 4], b: [u8; 4]| a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 1);
            let found = [at(0, 0), at(w / 2, h / 2), at(w - 1, h - 1)];
            for (found, expected) in found.into_iter().zip(pixels) {
                assert!(near(found, expected), "{text}: {found:?} {expected:?}");
            }
            let total: i64 = rgba.iter().map(|value| i64::from(*value)).sum();
            assert!(
                (total - sum).abs() <= rgba.len() as i64 / 8,
                "{text}: {total}"
            );
            assert!(image(text).is_some_and(|data| (data.width, data.height) == (w, h)));
        }
    }

    #[test]
    fn short_or_malformed_hashes_draw_nothing() {
        assert!(decode(&[]).is_none());
        assert!(decode(&[0x9f, 0x08, 0x82, 0x03]).is_none());
        // A header that promises more terms than it carries.
        assert!(decode(&[0xa0, 0xf7, 0x0d, 0x1d, 0x9a]).is_none());
        // Without terms along a side: no shape to draw.
        assert!(decode(&[0, 0, 0, 0x08, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_none());
        assert!(image("not base64!").is_none());
        assert!(image(&"A".repeat(200)).is_none());
    }
}
