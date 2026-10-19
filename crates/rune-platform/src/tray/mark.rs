//! RUNE's icon (`assets/icons/hicolor`: the white rune on its amber tile)
//! as the tray's pixmaps, at whatever size the tray asks for, with a red
//! dot for mentions.

/// The icon as drawn for each size, smallest first: 16 and 24 with a
/// thicker rune, which would turn thinner than a pixel there.
const DRAWN: [(usize, &[u8]); 6] = [
    (
        16,
        include_bytes!("../../../../assets/icons/hicolor/16x16/apps/rune.png"),
    ),
    (
        24,
        include_bytes!("../../../../assets/icons/hicolor/24x24/apps/rune.png"),
    ),
    (
        32,
        include_bytes!("../../../../assets/icons/hicolor/32x32/apps/rune.png"),
    ),
    (
        48,
        include_bytes!("../../../../assets/icons/hicolor/48x48/apps/rune.png"),
    ),
    (
        64,
        include_bytes!("../../../../assets/icons/hicolor/64x64/apps/rune.png"),
    ),
    (
        128,
        include_bytes!("../../../../assets/icons/hicolor/128x128/apps/rune.png"),
    ),
];
const DOT: [u8; 3] = [0xf2, 0x3f, 0x43];

/// One pixmap as StatusNotifierItem carries it: width, height and ARGB32
/// pixels in network byte order.
pub(crate) type Pixmap = (i32, i32, Vec<u8>);

/// The icon at the sizes panels ask for, from 16 to 64: the panel takes
/// the nearest; with the dot when `dot`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn pixmaps(dot: bool) -> Vec<Pixmap> {
    [16, 22, 24, 32, 48, 64]
        .into_iter()
        .map(|size| pixmap(size, dot))
        .collect()
}

/// The icon at `size` pixels a side (the Windows tray asks for its own):
/// the one drawn for it, or the next larger one shrunk.
pub(crate) fn pixmap(size: usize, dot: bool) -> Pixmap {
    let size = size.max(1);
    let (drawn, png) = DRAWN
        .iter()
        .find(|(drawn, _)| *drawn >= size)
        .unwrap_or(&DRAWN[DRAWN.len() - 1]);
    let rgba = decode(png).map_or_else(
        || vec![0; size * size * 4],
        |rgba| shrink(&rgba, *drawn, size),
    );
    let mut pixels = Vec::with_capacity(size * size * 4);
    let scale = 24.0 / size as f32;
    for (index, pixel) in rgba.chunks_exact(4).enumerate() {
        let (x, y) = (index % size, index / size);
        let mut color = [pixel[0], pixel[1], pixel[2]];
        let mut alpha = f32::from(pixel[3]) / 255.0;
        if dot {
            // In the icon's 24-unit box, over the tile's top right corner.
            let point = ((x as f32 + 0.5) * scale, (y as f32 + 0.5) * scale);
            let from = ((point.0 - 19.0).powi(2) + (point.1 - 5.0).powi(2)).sqrt();
            // A ring of the canvas around the dot keeps it apart.
            let ring = coverage(from - 6.0, scale);
            let inside = coverage(from - 4.5, scale);
            alpha = (alpha * (1.0 - ring) + inside).min(1.0);
            color = blend(color, DOT, inside);
        }
        pixels.extend([(alpha * 255.0).round() as u8, color[0], color[1], color[2]]);
    }
    (size as i32, size as i32, pixels)
}

/// An embedded PNG's RGBA pixels, 8 bits each (16 and 24 are drawn with
/// 16); `None` if it has no alpha.
fn decode(png: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut pixels).ok()?;
    (info.color_type == png::ColorType::Rgba && info.bit_depth == png::BitDepth::Eight).then(|| {
        pixels.truncate(info.buffer_size());
        pixels
    })
}

/// `rgba`, `from` pixels a side, at `to`: each pixel the average of what
/// it covers, weighted by its alpha so clear pixels lend no color.
fn shrink(rgba: &[u8], from: usize, to: usize) -> Vec<u8> {
    if from == to {
        return rgba.to_vec();
    }
    let scale = from as f32 / to as f32;
    // The source pixels under [start, end), with how much of each.
    let span = |start: f32, end: f32| {
        let first = start.floor() as usize;
        let last = (end.ceil() as usize).min(from);
        (first..last).map(move |pixel| {
            let covered = end.min(pixel as f32 + 1.0) - start.max(pixel as f32);
            (pixel, covered.max(0.0))
        })
    };
    let mut out = Vec::with_capacity(to * to * 4);
    for y in 0..to {
        for x in 0..to {
            let mut sum = [0.0f32; 4];
            let mut area = 0.0;
            let (left, top) = (x as f32 * scale, y as f32 * scale);
            for (row, height) in span(top, top + scale) {
                for (column, width) in span(left, left + scale) {
                    let at = (row * from + column) * 4;
                    let weight = width * height;
                    let alpha = f32::from(rgba[at + 3]) * weight;
                    for channel in 0..3 {
                        sum[channel] += f32::from(rgba[at + channel]) * alpha;
                    }
                    sum[3] += alpha;
                    area += weight;
                }
            }
            let color = |channel: usize| {
                if sum[3] > 0.0 {
                    (sum[channel] / sum[3]).round() as u8
                } else {
                    0
                }
            };
            out.extend([
                color(0),
                color(1),
                color(2),
                (sum[3] / area.max(f32::EPSILON)).round() as u8,
            ]);
        }
    }
    out
}

/// 1 inside a shape whose signed distance is `distance`, 0 outside, a
/// ramp across one pixel at its edge.
fn coverage(distance: f32, pixel: f32) -> f32 {
    (0.5 - distance / pixel).clamp(0.0, 1.0)
}

fn blend(under: [u8; 3], over: [u8; 3], amount: f32) -> [u8; 3] {
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    [
        mix(under[0], over[0]),
        mix(under[1], over[1]),
        mix(under[2], over[2]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(pixmap: &Pixmap, x: usize, y: usize) -> [u8; 4] {
        let start = (y * pixmap.0 as usize + x) * 4;
        pixmap.2[start..start + 4].try_into().unwrap()
    }

    #[test]
    fn every_size_drawn_is_an_rgba_png_of_its_size() {
        for (size, png) in DRAWN {
            let rgba = decode(png).unwrap_or_else(|| panic!("{size}"));
            assert_eq!(rgba.len(), size * size * 4, "{size}");
        }
    }

    #[test]
    fn the_icon_is_amber_with_a_white_rune_at_any_size_and_the_dot_red() {
        for size in [16, 20, 22, 24, 32, 40, 48, 64, 128] {
            let plain = pixmap(size, false);
            assert_eq!((plain.0, plain.1), (size as i32, size as i32));
            assert_eq!(plain.2.len(), size * size * 4);
            assert!(
                at(&plain, 0, 0)[0] < 16,
                "{size}: a rounded corner is clear"
            );
            // Amber at the tile's lower right, past the rune.
            // The owner's tile lets 1 % through; 16 and 24 are opaque.
            let [alpha, red, green, blue] = at(&plain, size * 3 / 4, size * 3 / 4);
            assert!(alpha > 240, "{size}: {alpha}");
            assert!(
                red > 180 && green > 100 && blue < 90,
                "{size}: {red} {green} {blue}"
            );
            // The rune, white.
            assert!(
                plain
                    .2
                    .chunks_exact(4)
                    .any(|pixel| pixel.iter().all(|value| *value > 235)),
                "{size}: the rune"
            );
        }
        let plain = pixmap(24, false);
        let dotted = pixmap(24, true);
        assert_eq!(&at(&dotted, 19, 5)[1..], &DOT);
        assert_eq!(at(&dotted, 19, 5)[0], 255);
        assert_eq!(at(&dotted, 8, 18), at(&plain, 8, 18), "the icon stays");
    }

    #[test]
    fn shrinking_averages_what_each_pixel_covers_and_clear_lends_no_color() {
        // Two by two: opaque red and clear black on top, opaque blue below.
        let rgba = [
            255, 0, 0, 255, 0, 0, 0, 0, //
            0, 0, 255, 255, 0, 0, 255, 255,
        ];
        assert_eq!(shrink(&rgba, 2, 1), [85, 0, 170, 191]);
        assert_eq!(shrink(&rgba, 2, 2), rgba);
    }
}
