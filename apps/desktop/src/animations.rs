//! Plays the bounded frames `rune-core` decoded on limn surfaces. Only what the
//! interface calls playable runs; everything else is forgotten, GPU texture too.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use eframe::egui;
use rune_components::message::animation_surface;
use rune_core::Animation;

struct Player {
    animation: Arc<Animation>,
    started: Instant,
    /// Each frame once converted for the GPU; a clone shares its pixels, so
    /// a loop converts nothing again.
    converted: Vec<Option<limn_egui::SurfaceFrame>>,
    /// The frame presented last.
    shown: Option<usize>,
}

impl Player {
    fn new(animation: Arc<Animation>, now: Instant) -> Self {
        Self {
            converted: vec![None; animation.frames.len()],
            animation,
            started: now,
            shown: None,
        }
    }
}

#[derive(Default)]
pub struct Animations {
    /// By the digest of the still first frame the surface stands in for.
    players: HashMap<String, Player>,
}

/// The digests of the still first frames, and of the surfaces, laid out.
/// The surface fits its frames to its box itself (`SurfaceFit::Contain`).
pub fn laid_out(frame: &limn::Frame) -> HashSet<String> {
    let mut digests = HashSet::new();
    for command in &frame.commands {
        match command {
            limn::DrawCommand::RasterImage { image, .. } => {
                digests.insert(image.sha256().to_owned());
            }
            limn::DrawCommand::Surface { id, .. } => {
                if let Some(digest) = id.strip_prefix("animation:") {
                    digests.insert(digest.to_owned());
                }
            }
            _ => {}
        }
    }
    digests
}

/// The digests of the pictures and surfaces seen under `pointer`: with
/// «Animar GIF» off only those play (imagenes-y-lectura). What a clip cuts
/// off there is not seen, nor what something opaque painted later (a menu,
/// a dialog, a bar) covers.
pub fn under(frame: &limn::Frame, pointer: limn::Point) -> HashSet<String> {
    let mut seen = HashSet::new();
    // Whether the pointer is inside every clip in force.
    let mut inside = vec![true];
    for command in &frame.commands {
        let open = inside.last() != Some(&false);
        let here = |rect: &limn::Rect| open && rect.contains(pointer);
        match command {
            limn::DrawCommand::PushClip(rect) => inside.push(here(rect)),
            limn::DrawCommand::PopClip => {
                inside.pop();
            }
            limn::DrawCommand::RasterImage { rect, image, .. } if here(rect) => {
                seen.insert(image.sha256().to_owned());
            }
            limn::DrawCommand::Surface { rect, id, .. } if here(rect) => {
                seen.extend(id.strip_prefix("animation:").map(str::to_owned));
            }
            limn::DrawCommand::FillRect { rect, color, .. }
            | limn::DrawCommand::FillCornerRect { rect, color, .. }
                if color.a == u8::MAX && here(rect) =>
            {
                seen.clear();
            }
            limn::DrawCommand::Glass { rect, .. } if here(rect) => seen.clear(),
            _ => {}
        }
    }
    seen
}

/// Keep of `playing` what plays now: everything with «Animar GIF» on, and
/// with it off only what is seen under `pointer`, if any.
pub fn now_playing<T>(
    animate: bool,
    playing: &mut Vec<(String, T)>,
    frame: &limn::Frame,
    pointer: Option<limn::Point>,
) {
    if !animate {
        let seen = pointer.map(|at| under(frame, at)).unwrap_or_default();
        playing.retain(|(digest, _)| seen.contains(digest));
    }
}

impl Animations {
    /// Present the due frame of each playing animation and drop the rest.
    /// Returns when the next frame is due, for the host's repaint.
    pub fn step(
        &mut self,
        ctx: &egui::Context,
        playing: Vec<(String, Arc<Animation>)>,
        laid_out: &HashSet<String>,
        now: Instant,
    ) -> Option<Instant> {
        let wanted: HashSet<&String> = playing.iter().map(|(digest, _)| digest).collect();
        self.players.retain(|digest, _| {
            let keep = wanted.contains(digest);
            if !keep {
                limn_egui::forget_surface(&animation_surface(digest));
            }
            keep
        });
        let mut next: Option<Instant> = None;
        for (digest, animation) in playing {
            let player = self
                .players
                .entry(digest.clone())
                .or_insert_with(|| Player::new(animation.clone(), now));
            if !Arc::ptr_eq(&player.animation, &animation) {
                *player = Player::new(animation, now);
            }
            // Not laid out this frame: nothing to show it in.
            if !laid_out.contains(&digest) {
                continue;
            }
            let animation = &player.animation;
            let (index, left) = animation.frame_at(now.saturating_duration_since(player.started));
            if player.shown != Some(index) {
                let converted = &mut player.converted[index];
                if converted.is_none() {
                    *converted = limn_egui::SurfaceFrame::from_rgba(
                        [animation.width as usize, animation.height as usize],
                        &animation.frames[index].rgba,
                        1.0,
                    );
                }
                if converted.clone().is_some_and(|frame| {
                    limn_egui::present_surface(ctx, &animation_surface(&digest), frame)
                }) {
                    player.shown = Some(index);
                }
            }
            let due = now + left;
            next = Some(next.map_or(due, |next| next.min(due)));
        }
        next
    }

    /// The digests whose surfaces hold a frame: the interface draws those.
    pub fn shown(&self) -> HashSet<String> {
        self.players
            .iter()
            .filter(|(_, player)| player.shown.is_some())
            .map(|(digest, _)| digest.clone())
            .collect()
    }

    /// Free every frame's texture, as when the account signs out.
    pub fn clear(&mut self) {
        for digest in self.players.keys() {
            limn_egui::forget_surface(&animation_surface(digest));
        }
        self.players.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_core::AnimationFrame;
    use std::time::Duration;

    fn animation() -> Arc<Animation> {
        Arc::new(Animation {
            width: 2,
            height: 2,
            frames: (0..2)
                .map(|_| AnimationFrame {
                    delay: Duration::from_millis(100),
                    rgba: vec![255; 16],
                })
                .collect(),
        })
    }

    #[test]
    fn under_the_pointer_are_the_pictures_and_surfaces_it_is_over() {
        let digest = "c70e30d34f864733efb6c4afbf5270c1be63833a3e2e7648d3c9da1a94e829ec";
        let image = limn::RasterImage::new(
            digest,
            limn::RasterImageFormat::Png,
            5868,
            Arc::from(include_str!("../fixtures/attachment.png.b64")),
            "prueba",
        )
        .unwrap();
        let mut tree = limn::UiTree::new(limn::Element::row(vec![
            limn::Element::raster_image(image).size(Some(40.0), Some(40.0)),
            limn::Element::surface(animation_surface("otra"), "otra").size(Some(40.0), Some(40.0)),
        ]));
        let frame = tree.layout(limn::Size::new(200.0, 100.0));
        let at = |frame: &limn::Frame, x| under(frame, limn::Point::new(x, 20.0));
        assert_eq!(at(&frame, 20.0), HashSet::from([digest.to_owned()]));
        assert_eq!(at(&frame, 60.0), HashSet::from(["otra".to_owned()]));
        assert!(at(&frame, 150.0).is_empty());
        // Clipped out where the pointer is, as a row scrolled past the edge
        // of the timeline: not seen there.
        let mut clipped = frame.clone();
        clipped.commands.insert(
            0,
            limn::DrawCommand::PushClip(limn::Rect::new(0.0, 0.0, 200.0, 10.0)),
        );
        clipped.commands.push(limn::DrawCommand::PopClip);
        assert!(at(&clipped, 20.0).is_empty());
        // Under a menu painted later, opaque: not seen; through a veil, yes.
        let cover = |alpha| limn::DrawCommand::FillRect {
            rect: limn::Rect::new(0.0, 0.0, 30.0, 100.0),
            color: limn::Color::rgba(10, 10, 10, alpha),
            radius: 0.0,
        };
        let mut covered = frame.clone();
        covered.commands.push(cover(255));
        assert!(at(&covered, 20.0).is_empty());
        assert_eq!(at(&covered, 60.0), HashSet::from(["otra".to_owned()]));
        let mut veiled = frame.clone();
        veiled.commands.push(cover(120));
        assert_eq!(at(&veiled, 20.0), HashSet::from([digest.to_owned()]));
        // With «Animar GIF» off only what the pointer is over plays, and
        // nothing when it is away; on, everything does.
        let all = || vec![(digest.to_owned(), 1), ("otra".to_owned(), 2)];
        let mut playing = all();
        now_playing(
            false,
            &mut playing,
            &frame,
            Some(limn::Point::new(60.0, 20.0)),
        );
        assert_eq!(playing, [("otra".to_owned(), 2)]);
        let mut playing = all();
        now_playing(false, &mut playing, &frame, None);
        assert!(playing.is_empty());
        let mut playing = all();
        now_playing(true, &mut playing, &frame, None);
        assert_eq!(playing, all());
    }

    #[test]
    fn only_playable_laid_out_animations_play_and_the_rest_are_freed() {
        let ctx = egui::Context::default();
        let mut animations = Animations::default();
        let start = Instant::now();
        let boxes = HashSet::from(["a".to_owned()]);
        let next = animations.step(
            &ctx,
            vec![("a".into(), animation()), ("b".into(), animation())],
            &boxes,
            start,
        );
        assert_eq!(next, Some(start + Duration::from_millis(100)));
        // `b` is playable but not laid out: it keeps no surface to draw.
        assert_eq!(animations.shown(), HashSet::from(["a".to_owned()]));
        // Scrolled away or paused: the surface is forgotten.
        assert_eq!(animations.step(&ctx, Vec::new(), &boxes, start), None);
        assert!(animations.shown().is_empty());
        let looped = animation();
        animations.step(&ctx, vec![("a".into(), looped.clone())], &boxes, start);
        for later in [150, 250] {
            let later = start + Duration::from_millis(later);
            animations.step(&ctx, vec![("a".into(), looped.clone())], &boxes, later);
        }
        // Each frame was converted once and is presented again as it loops.
        let player = &animations.players["a"];
        assert_eq!(player.shown, Some(0));
        assert!(player.converted.iter().all(Option::is_some));
        animations.clear();
        assert!(animations.players.is_empty());
    }
}
