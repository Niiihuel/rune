//! Badges must contain their text even when a channel name consumes the row.

use super::*;
use crate::rail::{self, RailItem};
use limn::{DrawCommand, Frame, Point, Rect, Size, UiTree};

const GLYPH_WIDTH: f32 = 7.0;
const LINE_HEIGHT: f32 = 15.0;

fn layout(root: Element, size: Size) -> Frame {
    let mut tree = UiTree::new(root);
    tree.set_reduced_motion(true);
    tree.set_text_measurer(|text, _, _| text.chars().count() as f32 * GLYPH_WIDTH);
    tree.set_sans_metrics(limn::FontMetrics {
        char_width: GLYPH_WIDTH,
        line_height: LINE_HEIGHT * limn::DEFAULT_FONT_SIZE / 12.0,
    });
    tree.layout(size)
}

fn badge(frame: &Frame, label: &str) -> (Rect, Point) {
    let rect = frame
        .commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::FillRect { rect, color, .. } if *color == Look::DARK.danger_fill => {
                Some(*rect)
            }
            _ => None,
        })
        .expect("the mention capsule is painted");
    let position = frame
        .commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text { position, text, .. } if text == label => Some(*position),
            _ => None,
        })
        .expect("the mention count is painted");
    (rect, position)
}

fn centered_label(rect: Rect, position: Point, label: &str) {
    let width = label.chars().count() as f32 * GLYPH_WIDTH;
    assert!(
        position.y >= rect.y && position.y + LINE_HEIGHT <= rect.y + rect.height + 0.01,
        "badge text must fit vertically: capsule={rect:?}, text={position:?}"
    );
    assert!(
        (position.y + LINE_HEIGHT / 2.0 - rect.y - rect.height / 2.0).abs() < 0.01,
        "badge text must be centered vertically: capsule={rect:?}, text={position:?}"
    );
    assert!(
        (position.x + width / 2.0 - rect.x - rect.width / 2.0).abs() < 0.01,
        "badge text must be centered horizontally: capsule={rect:?}, text={position:?}"
    );
    assert!(position.x >= rect.x + 4.0 && position.x + width <= rect.x + rect.width - 4.0 + 0.01);
}

#[test]
fn mention_badge_text_stays_centered_and_clear_of_channel_name_and_settings() {
    for (count, label) in [(2, "2"), (22, "22"), (120, "99+")] {
        let frame = layout(
            channel(
                ChannelRow {
                    id: "channel:1".into(),
                    name: "noticias-con-un-nombre-largo-para-truncar".into(),
                    mentions: count,
                    read_only: true,
                    selected: true,
                    settings: Some("settings:1".into()),
                    ..ChannelRow::default()
                },
                &Look::DARK,
            )
            .size(Some(WIDTH), None),
            Size::new(WIDTH, ROW),
        );
        let (rect, position) = badge(&frame, label);
        centered_label(rect, position, label);
        assert!((rect.y + rect.height / 2.0 - ROW / 2.0).abs() < 0.01);
        let gear = frame
            .regions
            .iter()
            .find(|region| region.id == "settings:1")
            .unwrap()
            .rect;
        assert!(
            rect.x + rect.width < gear.x,
            "badge must leave room for channel settings"
        );
        let (name, x) = frame
            .commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Text { position, text, .. } if text.starts_with("noticias") => {
                    Some((text, position.x))
                }
                _ => None,
            })
            .unwrap();
        assert!(
            x + name.chars().count() as f32 * GLYPH_WIDTH < rect.x,
            "a long channel name must not cross the badge"
        );
    }
}

#[test]
fn rail_badge_keeps_its_corner_and_does_not_cross_the_next_server() {
    let item = |id: &str, mentions| RailItem {
        id: id.into(),
        name: id.into(),
        initials: "R".into(),
        accent: Look::DARK.accent,
        glyph: None,
        image: None,
        selected: false,
        unread: mentions > 0,
        mentions,
        muted: false,
        voice: Default::default(),
    };
    for (count, label) in [(2, "2"), (22, "22"), (120, "99+")] {
        let frame = layout(
            rail::server_rail(
                item("home", 0),
                vec![
                    rail::RailEntry::Server(item("server:1", count)),
                    rail::RailEntry::Server(item("server:2", 0)),
                ],
                &rail::RailDrag::default(),
                &Look::DARK,
            ),
            Size::new(rail::WIDTH, 300.0),
        );
        let (ringed, position) = badge(&frame, label);
        // limn's badge fills its ring too and strokes it, 3 px in the rail's
        // color, over the fill: the red that shows is inside it.
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            DrawCommand::StrokeRect { rect, color, width, .. }
                if *rect == ringed && *color == Look::DARK.rail && *width == 3.0
        )));
        let rect = Rect::new(
            ringed.x + 3.0,
            ringed.y + 3.0,
            ringed.width - 6.0,
            ringed.height - 6.0,
        );
        assert_eq!(rect.height, 17.0);
        let width = label.chars().count() as f32 * GLYPH_WIDTH;
        assert!(
            (position.x + width / 2.0 - rect.x - rect.width / 2.0).abs() < 0.01
                && (position.y + LINE_HEIGHT / 2.0 - rect.y - rect.height / 2.0).abs() < 0.01,
            "badge text must be centered: capsule={rect:?}, text={position:?}"
        );
        assert!(
            position.x >= rect.x + 3.0 && position.x + width <= rect.x + rect.width - 3.0 + 0.01
        );
        let mark = frame
            .regions
            .iter()
            .find(|region| region.id == "server:1")
            .unwrap()
            .rect;
        let next = frame
            .regions
            .iter()
            .find(|region| region.id == "server:2")
            .unwrap()
            .rect;
        assert!(
            (rect.x + rect.width - mark.x - mark.width).abs() < 0.01,
            "the badge stays at its own mark's right edge"
        );
        assert!(
            rect.y > mark.y + mark.height / 2.0 && rect.y + rect.height < next.y,
            "the badge occupies the lower corner without entering the next mark"
        );
    }
}
