//! Animated vector (Lottie) stickers: their first frame as the still and
//! their frames as an animation, drawn by rasterlottie under the same
//! budgets as an animated preview. One the rasterizer does not support
//! (expressions, effects, text) has no picture, as before.

use std::collections::HashMap;
use std::time::Duration;

use rasterlottie::{AnimatedValue, Layer, LayerType, RenderConfig, Renderer, Rgba8, ShapeItem};

use super::animation::{Animation, Keeper, MAX_DECODE_TIME};

/// The largest canvas believed: Discord's stickers are 320 px.
const MAX_SIDE: u32 = 1024;
/// Longer animations keep their first frame only: Discord's last 5 s.
const MAX_SECONDS: f32 = 10.0;
/// Frames drawn a second at most.
const MAX_RATE: f32 = 30.0;
/// Precompositions nested deeper than this, or in a cycle, are refused
/// before the rasterizer, which would follow a cycle past its stack.
const MAX_NESTING: usize = 8;
/// The most copies a repeater makes (and steps it is offset), and the most
/// points a star or polygon has: the rasterizer allocates them as told.
const MAX_COUNT: f32 = 64.0;
/// The most layers and shapes one frame draws, precompositions and
/// repeaters expanded: Discord's stickers draw a few hundred.
const MAX_WEIGHT: u64 = 20_000;

/// The precompositions of `animation` by id, found as the rasterizer finds
/// them: the first asset with an id wins, and one without an id is "".
type Assets<'a> = HashMap<&'a str, &'a [Layer]>;

/// How many layers and shapes a frame of `animation` draws, precompositions
/// and repeaters expanded; `None` past [`MAX_WEIGHT`], for precompositions
/// in a cycle or nested deeper than [`MAX_NESTING`], or for a count past
/// [`MAX_COUNT`]. The rasterizer has no guard of its own for any of them.
fn weight(animation: &rasterlottie::Animation) -> Option<u64> {
    let mut assets = Assets::new();
    for asset in &animation.assets {
        assets
            .entry(asset.id.as_str())
            .or_insert(asset.layers.as_slice());
    }
    layers_weight(&animation.layers, &assets, &mut Vec::new())
}

fn layers_weight<'a>(
    layers: &'a [Layer],
    assets: &Assets<'a>,
    path: &mut Vec<&'a str>,
) -> Option<u64> {
    let mut total = 0;
    for layer in layers {
        total += 1 + shapes_weight(&layer.shapes)?;
        let inner = layer
            .ref_id
            .as_deref()
            .filter(|_| layer.layer_type == LayerType::PRECOMP)
            .and_then(|id| Some((id, *assets.get(id)?)));
        if let Some((id, inner)) = inner {
            if path.contains(&id) || path.len() >= MAX_NESTING {
                return None;
            }
            path.push(id);
            total += layers_weight(inner, assets, path)?;
            path.pop();
        }
        if total > MAX_WEIGHT {
            return None;
        }
    }
    Some(total)
}

/// The shapes a list draws, groups expanded: a repeater copies what comes
/// before it, so the list counts as its shapes times every repeater's copies.
fn shapes_weight(items: &[ShapeItem]) -> Option<u64> {
    let (mut shapes, mut times) = (0_u64, 1_u64);
    for item in items {
        match item.item_type.as_str() {
            "gr" => shapes += 1 + shapes_weight(&item.items)?,
            "rp" => {
                count(item.repeater_offset(), -MAX_COUNT)?;
                let copies = count(item.repeater_copies(), 0.0)?;
                times = times.saturating_mul((copies.ceil() as u64).max(1));
            }
            "sr" => {
                count(item.polystar_points().as_ref(), 0.0)?;
                shapes += 1;
            }
            _ => shapes += 1,
        }
        if shapes.saturating_mul(times) > MAX_WEIGHT {
            return None;
        }
    }
    Some(shapes * times)
}

/// A count the rasterizer allocates by, when it is absent (its default) or
/// a fixed number from `low` to [`MAX_COUNT`]. An animated one is refused:
/// eased keyframes can overshoot their values.
fn count(value: Option<&AnimatedValue>, low: f32) -> Option<f32> {
    match value {
        None => Some(0.0),
        Some(value) => value
            .as_scalar()
            .filter(|count| (low..=MAX_COUNT).contains(count)),
    }
}

/// The first frame of the Lottie animation in `bytes`, sized by `fit`, and
/// its frames when they keep within the budgets; `None` when it cannot be
/// read or drawn.
pub(super) fn decode(
    bytes: &[u8],
    side: u32,
    fit: impl Fn(image::DynamicImage) -> image::RgbaImage,
) -> Option<(image::RgbaImage, Option<Animation>)> {
    let animation =
        rasterlottie::Animation::from_json_str(std::str::from_utf8(bytes).ok()?).ok()?;
    weight(&animation)?;
    let (width, height) = (animation.width, animation.height);
    if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height) {
        return None;
    }
    let prepared = Renderer::default().prepare(&animation).ok()?;
    let config = RenderConfig::new(Rgba8::TRANSPARENT, side as f32 / width.max(height) as f32);
    let mut canvas = prepared.new_scratch_pixmap_for_config(config).ok()?;
    let mut picture = |frame: f32| {
        prepared
            .render_frame_into_pixmap(frame, config, &mut canvas)
            .ok()?;
        // tiny-skia keeps color premultiplied by alpha; an image, straight.
        let rgba = canvas
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let color = pixel.demultiply();
                [color.red(), color.green(), color.blue(), color.alpha()]
            })
            .collect();
        let pixels = image::RgbaImage::from_raw(canvas.width(), canvas.height(), rgba)?;
        Some(fit(image::DynamicImage::ImageRgba8(pixels)))
    };
    let first = picture(animation.in_point)?;
    let (rate, seconds) = (animation.frame_rate, animation.duration_seconds());
    if !(rate.is_finite() && rate > 0.0 && seconds > 0.0 && seconds <= MAX_SECONDS) {
        return Some((first, None));
    }
    // At most MAX_RATE frames a second, each a whole number of its own.
    let step = (rate / MAX_RATE).ceil().max(1.0);
    let delay = Duration::from_secs_f32(step / rate);
    let mut keeper = Keeper::new(MAX_DECODE_TIME);
    let mut at = animation.in_point;
    while at < animation.out_point {
        let drawn = keeper.push(delay, || picture(at).unwrap_or_default());
        if drawn.is_none() {
            return Some((first, None));
        }
        at += step;
    }
    Some((first, keeper.finish()))
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    const SAMPLE: &[u8] = include_bytes!("../../../../assets/sticker-sample.json");

    fn fit(frame: image::DynamicImage) -> image::RgbaImage {
        frame.to_rgba8()
    }

    /// `layers` over `assets`, on a small canvas, as the rasterizer reads it.
    fn vectors(layers: Value, assets: Value) -> rasterlottie::Animation {
        let json = json!({"fr": 30, "ip": 0, "op": 30, "w": 64, "h": 64,
            "layers": layers, "assets": assets});
        rasterlottie::Animation::from_json_str(&json.to_string()).unwrap()
    }

    #[test]
    fn a_sticker_moves_through_its_frames_at_its_rate() {
        let (first, animation) = decode(SAMPLE, 128, fit).unwrap();
        assert_eq!(first.dimensions(), (128, 128));
        // An orange ball near the top, on a clear canvas.
        let ball = first.get_pixel(64, 40).0;
        assert!(ball[0] > 200 && ball[2] < 100 && ball[3] == 255, "{ball:?}");
        assert_eq!(first.get_pixel(2, 2).0, [0, 0, 0, 0]);
        let animation = animation.unwrap();
        assert_eq!(animation.frames.len(), 30);
        let total: Duration = animation.frames.iter().map(|frame| frame.delay).sum();
        assert!((total.as_secs_f32() - 1.0).abs() < 0.01, "{total:?}");
        // Half way, the ball is down.
        let middle = &animation.frames[15].rgba;
        let at = |x: usize, y: usize| &middle[(y * 128 + x) * 4..][..4];
        assert!(at(64, 88)[3] == 255 && at(64, 40)[3] == 0);
    }

    #[test]
    fn strangers_cycles_and_giants_are_refused() {
        assert!(decode(b"{}", 128, fit).is_none());
        assert!(decode(b"not json", 128, fit).is_none());
        let mut json: Value = serde_json::from_slice(SAMPLE).unwrap();
        json["w"] = 100_000.into();
        assert!(decode(json.to_string().as_bytes(), 128, fit).is_none());
        // A precomposition that holds itself, and a chain too deep.
        let precomp = |id: &str| json!({"ty": 0, "refId": id});
        let cycle = vectors(
            json!([precomp("a")]),
            json!([{"id": "a", "layers": [precomp("b")]}, {"id": "b", "layers": [precomp("a")]}]),
        );
        assert_eq!(weight(&cycle), None);
        let chain: Vec<Value> = (0..20)
            .map(|level| json!({"id": format!("{level}"), "layers": [precomp(&format!("{}", level + 1))]}))
            .collect();
        assert_eq!(weight(&vectors(json!([precomp("0")]), chain.into())), None);
        // Found as the rasterizer finds them: an asset without an id is "",
        // and of two with the same id the first one counts.
        let unnamed = json!([{"layers": [precomp("")]}]);
        assert_eq!(weight(&vectors(json!([precomp("")]), unnamed)), None);
        let twice = json!([{"id": "a", "layers": [precomp("a")]}, {"id": "a", "layers": []}]);
        assert_eq!(weight(&vectors(json!([precomp("a")]), twice)), None);
        // Refused before the rasterizer, which would run out of stack.
        let unnamed = br#"{"w":64,"h":64,"fr":30,"ip":0,"op":30,
            "layers":[{"ty":0,"refId":""}],"assets":[{"layers":[{"ty":0,"refId":""}]}]}"#;
        assert!(decode(unnamed, 128, fit).is_none());
        // Shared, not cyclic: fine.
        let shared = vectors(
            json!([precomp("a"), precomp("a")]),
            json!([{"id": "a", "layers": [{"ty": 4}]}]),
        );
        assert_eq!(weight(&shared), Some(4));
        // Fanned out, each level many times the one under it: refused.
        let fan: Vec<Value> = (0..8)
            .map(|level| {
                let next = precomp(&format!("{}", level + 1));
                json!({"id": format!("{level}"), "layers": vec![next; 10]})
            })
            .collect();
        assert_eq!(weight(&vectors(json!([precomp("0")]), fan.into())), None);
    }

    #[test]
    fn repeaters_and_stars_are_believed_only_when_small() {
        let shapes = |repeater: Value| {
            vectors(
                json!([{"ty": 4, "shapes": [{"ty": "el"}, {"ty": "fl"}, repeater]}]),
                json!([]),
            )
        };
        let repeater = |copies: Value| json!({"ty": "rp", "c": copies, "o": {"a": 0, "k": 0}});
        assert_eq!(
            weight(&shapes(repeater(json!({"a": 0, "k": 3})))),
            Some(1 + 2 * 3)
        );
        // A trillion copies would be a trillion allocated.
        assert_eq!(weight(&shapes(repeater(json!({"a": 0, "k": 1e12})))), None);
        let eased = json!({"a": 1, "k": [{"t": 0, "s": [1]}, {"t": 30, "s": [2]}]});
        assert_eq!(weight(&shapes(repeater(eased))), None, "animated");
        let far = json!({"ty": "rp", "c": {"a": 0, "k": 2}, "o": {"a": 0, "k": 1e9}});
        assert_eq!(weight(&shapes(far)), None, "offset");
        // Repeaters one after another multiply.
        let mut items = vec![json!({"ty": "el"})];
        items.extend(vec![repeater(json!({"a": 0, "k": 64})); 3]);
        let nested = vectors(json!([{"ty": 4, "shapes": items}]), json!([]));
        assert_eq!(weight(&nested), None);
        let star = |points: f64| shapes(json!({"ty": "sr", "sy": 1, "pt": {"a": 0, "k": points}}));
        assert!(weight(&star(5.0)).is_some());
        assert_eq!(weight(&star(1e12)), None);
    }

    #[test]
    fn a_sticker_s_json_becomes_its_still_and_animation() {
        let at = "/stickers/1.json";
        let data =
            super::super::normalize_at(at, SAMPLE, rune_model::ImageKind::Artwork, 1.0).unwrap();
        assert_eq!((data.width, data.height), (128, 128));
        assert_eq!(data.animation.as_ref().unwrap().frames.len(), 30);
        // Only a sticker's JSON is read as vectors: not artwork from
        // elsewhere, nor a preview.
        assert!(super::super::normalize(SAMPLE, rune_model::ImageKind::Artwork).is_none());
        assert!(super::super::normalize_at(
            "/emojis/1.webp",
            SAMPLE,
            rune_model::ImageKind::Artwork,
            1.0
        )
        .is_none());
        assert!(super::super::normalize_at(
            at,
            SAMPLE,
            rune_model::ImageKind::AttachmentPreview,
            1.0
        )
        .is_none());
    }

    #[test]
    fn premultiplied_pixels_are_drawn_straight() {
        // Half covered in orange: straight, its color stays orange.
        let (first, _) = decode(SAMPLE, 128, fit).unwrap();
        let edge = first
            .pixels()
            .find(|pixel| pixel[3] > 40 && pixel[3] < 215)
            .expect("an antialiased edge");
        assert!(edge[0] > 200, "{edge:?}");
    }
}
