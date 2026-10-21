//! A compact, themed welcome surface. Authorization stays in the UI controller.

use limn::{Access, Color, Component, Element, FontFamily, Icon, Point, Role, ScrollArea, Size};

use crate::{centered, strong_line, text, Look, MARK};

const OUTER_PADDING: f32 = 24.0;
const CARD_PADDING: f32 = 24.0;
const MAX_WIDTH: f32 = 480.0;

fn card_width(window: Size) -> f32 {
    (window.width - OUTER_PADDING * 2.0).clamp(0.0, MAX_WIDTH)
}

/// Shared by the controller's actions and this card's heading and brand.
pub fn content_width(window: Size) -> f32 {
    (card_width(window) - CARD_PADDING * 2.0).max(0.0)
}

/// Compose a complete welcome page using existing limn layout and paint APIs.
pub fn page(
    title: &str,
    subtitle: &str,
    actions: Vec<Element>,
    window: Size,
    look: &Look,
) -> Element {
    let inner = content_width(window);
    let tint = |alpha| Color {
        a: alpha,
        ..look.accent
    };
    let mark = centered(
        Element::icon(Icon::Custom(MARK))
            .large_icon(32.0)
            .foreground(crate::BRAND),
    )
    .size(Some(56.0), Some(56.0))
    .background(tint(18))
    .radius(16.0)
    .border(tint(38), 1.0);
    let brand = Element::column(vec![
        mark.centered_x(),
        strong_line("RUNE", 18.0, look.text)
            .center_content()
            .size(Some(inner), None),
    ])
    .gap(10.0);
    let heading = Element::column(vec![
        text(title, 24.0, look.text)
            .font_family(FontFamily::SansMedium)
            .size(Some(inner), None)
            .center_content()
            .accessible(Access::new(Role::Heading).id("auth:title").name(title)),
        text(subtitle, 14.0, look.muted)
            .size(Some(inner), None)
            .center_content(),
    ])
    .gap(8.0);
    let card = Element::column(vec![brand, heading, Element::column(actions).gap(12.0)])
        .gap(24.0)
        .padding(CARD_PADDING)
        .size(Some(card_width(window)), None)
        .background(look.menu)
        .radius(12.0)
        .border(look.border, 1.0)
        .shadow(look.shadow, Point::new(0.0, 12.0), 32.0, 0.0);
    // Keep long failure messages reachable instead of fixing the card's height.
    let content = ScrollArea::new(
        "auth:page",
        centered(card)
            .padding(OUTER_PADDING)
            .min_size(window.width, window.height),
    )
    .render();
    // A quiet theme-derived wash distinguishes the welcome page even in Onyx.
    // These layers have no hit regions and do not affect the card's layout.
    let background = Element::spacer()
        .size(Some(window.width), Some(window.height))
        .background(look.canvas)
        .vertical_gradient(vec![tint(20), Color::TRANSPARENT, tint(6)]);
    Element::stack(vec![background, content]).size(Some(window.width), Some(window.height))
}
