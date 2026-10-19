//! RUNE's interface pieces, built with limn. Each is a function or a small
//! struct from plain props to an `Element`: none reads the state or talks to
//! a provider, so each can be laid out and tested on its own. `rune-ui`
//! composes them into the window.

pub mod auth;
pub mod blocked;
pub mod challenge;
pub mod channel_menu;
pub mod channel_settings;
pub mod chrome;
pub mod composer;
pub mod emoji;
pub mod folders;
pub mod forum;
pub mod friends;
pub mod group;
// Commands and interactions (P3).
pub mod interactions;
// Presence, identity and expressions (P5).
pub mod identity;
pub mod look;
pub mod members;
pub mod message;
// Message actions (P4).
pub mod message_actions;
pub mod profile;
pub mod rail;
pub mod server_menu;
pub mod server_settings;
pub mod sidebar;
pub mod stage;
pub mod text;
pub mod threads;
pub mod time;
pub mod uploads;
pub mod user_settings;
pub mod voice;
// Desktop, language and local voice (P6).
pub mod desktop;
// Server administration, rest (P7).
pub mod server_admin;
// Private calls.
pub mod private_call;
// The quick voice panel, Serein-style (2026-10-18).
pub mod quick_voice;
// Emoji and states (2026-10-18): moved from the desktop, which still hands
// its pictures to limn-egui's color emoji; and an emoji's card.
pub mod emoji_card;
pub mod twemoji;
// Rules and onboarding (2026-10-18).
pub mod onboarding;
// Editable shortcuts (atajos-y-navegacion).
pub mod keybinds;
// Own themes, main color and transparency (temas-propios).
pub mod theme;
// Scheduled events (eventos-programados).
pub mod events;
// AutoMod (automod).
pub mod automod;
// RUNE's own updates (actualizaciones).
pub mod updates;

use limn::{Color, Element, FontFamily, Icon};

pub use look::Look;

/// RUNE's mark: an original rune-like glyph drawn with the current color.
pub const MARK: &str = include_str!("../../../assets/rune-mark.svg");
/// Headphones with a slash, for a deafened account: Bootstrap has none.
pub const HEADPHONES_OFF: &str = include_str!("../../../assets/headphones-off.svg");

/// Interface text in the sans face.
pub fn text(value: impl Into<String>, size: f32, color: Color) -> Element {
    Element::text(value)
        .font_family(FontFamily::Sans)
        .font_size(size)
        .foreground(color)
}

/// One line of interface text, cut with an ellipsis when it does not fit.
pub fn line(value: impl Into<String>, size: f32, color: Color) -> Element {
    Element::truncated_text(value)
        .font_family(FontFamily::Sans)
        .font_size(size)
        .foreground(color)
}

/// One line in the medium face, for names and titles.
pub fn strong_line(value: impl Into<String>, size: f32, color: Color) -> Element {
    Element::truncated_text(value)
        .font_family(FontFamily::SansMedium)
        .font_size(size)
        .foreground(color)
}

/// An icon in `color`.
pub fn icon(icon: Icon, color: Color) -> Element {
    Element::icon(icon).foreground(color)
}

/// `child` centered on both axes in the space it is given.
pub fn centered(child: Element) -> Element {
    Element::column(vec![
        Element::spacer().flex(1.0),
        Element::row(vec![
            Element::spacer().flex(1.0),
            child,
            Element::spacer().flex(1.0),
        ]),
        Element::spacer().flex(1.0),
    ])
}

/// A fixed empty space.
pub fn gap(width: f32, height: f32) -> Element {
    Element::spacer().size(Some(width), Some(height))
}

/// A round mark for a person: their picture, or the initial of `name` on
/// their `accent`.
pub fn avatar(name: &str, accent: Color, size: f32) -> Element {
    avatar_with_image(name, accent, size, None)
}

/// Keep the initial underneath a validated picture while its texture loads.
pub fn avatar_with_image(
    name: &str,
    accent: Color,
    size: f32,
    image: Option<limn::RasterImage>,
) -> Element {
    // The initial in white on the person's color.
    let palette = limn::ComponentPalette {
        text: Color::rgb(255, 255, 255),
        ..limn::ComponentPalette::neutral_dark()
    };
    let initial =
        limn::Component::render(limn::Avatar::new(name, palette).color(accent).size(size));
    match image {
        Some(image) => Element::stack(vec![
            initial,
            limn::Component::render(
                limn::Avatar::new(name, palette)
                    .size(size)
                    .image(image.cover()),
            ),
        ])
        .size(Some(size), Some(size)),
        None => initial,
    }
}

/// `child` with a status dot over its bottom right corner, ringed in `ring`
/// so it reads on any ground: about 10 px of color and 2 of ring on a 34 px
/// avatar.
pub fn with_status(child: Element, size: f32, status: Color, ring: Color) -> Element {
    let dot = (size * 0.42).max(10.0);
    let mark = Element::panel(Element::spacer())
        .size(Some(dot), Some(dot))
        .radius(dot / 2.0)
        .background(status)
        .border(ring, (dot * 0.16).max(2.0));
    Element::stack(vec![
        child,
        Element::column(vec![
            Element::spacer().flex(1.0),
            Element::row(vec![Element::spacer().flex(1.0), mark]),
        ]),
    ])
    .size(Some(size), Some(size))
}

/// How many bytes, as a person reads it.
pub fn file_size(bytes: u64) -> String {
    match bytes {
        0..1_024 => format!("{bytes} B"),
        1_024..1_048_576 => format!("{:.1} KB", bytes as f64 / 1_024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use limn::{Element, Frame, Size, UiTree};

    /// Lay `element` out alone at `size`, settled, with 7 px characters.
    pub fn frame(element: Element, size: Size) -> (UiTree, Frame) {
        let mut tree = UiTree::new(element);
        tree.set_reduced_motion(true);
        tree.set_text_measurer(|text, _, _| text.chars().count() as f32 * 7.0);
        let frame = tree.layout(size);
        (tree, frame)
    }

    /// The IDs of every control laid out that can be used: buttons and
    /// fields, not the hover regions of hints.
    pub fn controls(frame: &Frame) -> Vec<String> {
        frame
            .regions
            .iter()
            .filter(|region| {
                matches!(
                    region.kind,
                    limn::HitKind::Button | limn::HitKind::TextInput | limn::HitKind::TextArea
                )
            })
            .map(|region| region.id.clone())
            .collect()
    }

    /// Every piece of text painted.
    pub fn texts(frame: &Frame) -> Vec<String> {
        frame
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn avatar_keeps_initial_under_a_picture_until_its_texture_arrives() {
        let image = limn::RasterImage::new(
            "0".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "Ada",
        )
        .unwrap();
        let mut tree = limn::UiTree::new(super::avatar_with_image(
            "Ada",
            limn::Color::rgb(40, 50, 60),
            32.0,
            Some(image),
        ));
        let frame = tree.layout(limn::Size::new(32.0, 32.0));
        assert!(frame.commands.iter().any(
            |command| matches!(command,limn::DrawCommand::RasterImage{radius,..} if *radius==16.0)
        ));
        assert!(frame
            .commands
            .iter()
            .any(|command| matches!(command,limn::DrawCommand::Text{text,..} if text=="A")));
    }
    #[test]
    fn sizes_read_in_the_unit_that_fits() {
        assert_eq!(super::file_size(512), "512 B");
        assert_eq!(super::file_size(18_402), "18.0 KB");
        assert_eq!(super::file_size(3_204_871), "3.1 MB");
    }
}
