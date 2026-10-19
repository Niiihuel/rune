//! The conversation while its history loads: groups of messages drawn as
//! shapes, as many as fill the window, held against the list's end as the
//! history is.
//!
//! The official client's (static REA, 2026-10-17): per group an avatar
//! circle, a name bar 80 to 120 wide and, per message (one to four), a line
//! bar 120 to 320 wide, 16 high in a 22 px line; about one group in four
//! carries a picture block 140–400 × 100–320. All in the text's color at
//! low opacity (the avatar 8 %, the name 10–20 %, the lines 6 %, a picture
//! 3 %), with no animation. Here the same, in RUNE's message metrics, and
//! chosen from the conversation's ID rather than at random: a frame and the
//! next draw the same shapes.

use limn::{Color, Component, Element, ScrollArea, ScrollbarVisibility};

use super::{AVATAR, BODY_LINE, GROUP_GAP, GUTTER, LEFT, RIGHT};
use crate::{gap, Look};

/// The most groups drawn, whatever the window.
const MOST: usize = 30;

/// One placeholder group.
#[derive(Clone, Debug, PartialEq)]
struct Group {
    name: f32,
    /// The name bar's opacity, 0 to 255.
    ink: u8,
    lines: Vec<f32>,
    /// A picture under the line at this index: its width and height.
    picture: Option<(usize, f32, f32)>,
}

impl Group {
    fn height(&self) -> f32 {
        GROUP_GAP
            + BODY_LINE * (1 + self.lines.len()) as f32
            + self.picture.map_or(0.0, |(_, _, height)| height + 4.0)
    }
}

/// A small generator of the same numbers for the same seed (xorshift).
struct Dice(u64);

impl Dice {
    fn roll(&mut self, low: f32, high: f32) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        low + (self.0 % 10_000) as f32 / 10_000.0 * (high - low)
    }
}

/// The groups that fill `height`, newest last.
fn groups(seed: u64, height: f32) -> Vec<Group> {
    let mut dice = Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut groups = Vec::new();
    let mut filled = 0.0;
    while filled < height && groups.len() < MOST {
        let count = dice.roll(1.0, 5.0) as usize;
        let group = Group {
            name: dice.roll(80.0, 120.0),
            ink: dice.roll(26.0, 51.0) as u8,
            lines: (0..count).map(|_| dice.roll(120.0, 320.0)).collect(),
            // About one group in four, as the official client.
            picture: (dice.roll(0.0, 1.0) < 0.27).then(|| {
                (
                    dice.roll(0.0, count as f32) as usize,
                    dice.roll(140.0, 400.0),
                    dice.roll(100.0, 320.0),
                )
            }),
        };
        filled += group.height();
        groups.push(group);
    }
    groups
}

/// A bar `width` (at most `room`) by `height`, in `ink`.
fn bar(width: f32, height: f32, room: f32, radius: f32, ink: Color) -> Element {
    Element::panel(Element::spacer())
        .size(Some(width.min(room).max(0.0)), Some(height))
        .radius(radius)
        .background(ink)
}

fn group(group: &Group, room: f32, look: &Look) -> Element {
    let ink = |alpha: u8| Look::alpha(look.text, alpha);
    // A bar 16 high, 3 below the top of its 22 px line.
    let in_line =
        |bar: Element| Element::column(vec![gap(0.0, 3.0), bar]).size(None, Some(BODY_LINE));
    let mut copy = vec![in_line(bar(group.name, 16.0, room, 8.0, ink(group.ink)))];
    for (index, width) in group.lines.iter().enumerate() {
        copy.push(in_line(bar(*width, 16.0, room, 8.0, ink(15))));
        if let Some((_, width, height)) = group.picture.filter(|(at, ..)| *at == index) {
            copy.push(gap(0.0, 4.0));
            copy.push(bar(width, height, room, 6.0, ink(8)));
        }
    }
    Element::column(vec![
        gap(0.0, GROUP_GAP),
        Element::row(vec![
            gap(LEFT, 0.0),
            Element::column(vec![
                gap(0.0, 1.0),
                Element::panel(Element::spacer())
                    .size(Some(AVATAR), Some(AVATAR))
                    .radius(AVATAR / 2.0)
                    .background(ink(20)),
            ]),
            gap(GUTTER - LEFT - AVATAR, 0.0),
            Element::column(copy).flex(1.0),
            gap(RIGHT, 0.0),
        ]),
    ])
}

/// The history of the conversation `seed` names while it loads, `width`
/// wide, enough groups for a window `height` high, held against the end.
/// `id` names the list, so it stays at its end across frames.
pub fn history_skeleton(id: String, seed: u64, width: f32, height: f32, look: &Look) -> Element {
    let room = width - GUTTER - RIGHT;
    let groups = groups(seed, height);
    let heights: Vec<f32> = groups.iter().map(Group::height).collect();
    let look = *look;
    // A list that starts at its end, as the history does: the newest
    // placeholder against the composer, the oldest cut at the top.
    ScrollArea::measured_list(
        id,
        groups.len(),
        heights.iter().copied().fold(0.0, f32::max),
        |index| index as u64,
        move |index| group(&groups[index], room, &look),
    )
    .render()
    .list_follow_end(true)
    .scrollbar_visibility(ScrollbarVisibility::AutoHide {
        hold_ms: 0,
        fade_ms: 0,
    })
    .accessible(
        limn::Access::new(limn::Role::ProgressIndicator).name(rune_i18n::t("Cargando mensajes")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::frame;
    use limn::{DrawCommand, Size};

    fn bars(height: f32) -> Vec<limn::Rect> {
        let (_, frame) = frame(
            history_skeleton("loading".into(), 42, 800.0, height, &Look::DARK).flex(1.0),
            Size::new(800.0, height),
        );
        frame
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::FillRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn it_fills_the_window_against_its_end_with_message_shapes() {
        for height in [400.0, 1000.0] {
            let shapes = bars(height);
            let avatars = shapes.iter().filter(|rect| rect.width == AVATAR).count();
            assert!(avatars >= 1, "{shapes:?}");
            // The newest line sits at the bottom, the top is reached too.
            let bottom = shapes
                .iter()
                .map(|rect| rect.y + rect.height)
                .fold(0.0, f32::max);
            assert!(
                bottom > height - BODY_LINE - 330.0 && bottom <= height + 0.5,
                "{bottom}"
            );
            assert!(
                shapes.iter().any(|rect| rect.y <= GROUP_GAP + BODY_LINE),
                "{shapes:?}"
            );
            // Lines start past the avatars' column and keep clear of the right.
            assert!(shapes
                .iter()
                .filter(|rect| rect.width != AVATAR)
                .all(|rect| rect.x >= GUTTER - 0.5 && rect.x + rect.width <= 800.0 - RIGHT + 0.5));
        }
    }

    #[test]
    fn the_same_conversation_draws_the_same_shapes() {
        assert_eq!(groups(7, 900.0), groups(7, 900.0));
        assert_ne!(groups(7, 900.0), groups(8, 900.0));
        let lines: Vec<usize> = groups(7, 3000.0)
            .iter()
            .map(|group| group.lines.len())
            .collect();
        assert!(
            lines.iter().all(|count| (1..=4).contains(count)),
            "{lines:?}"
        );
        assert!(groups(7, 3000.0)
            .iter()
            .any(|group| group.picture.is_some()));
        assert_eq!(groups(7, 100_000.0).len(), MOST);
    }
}
