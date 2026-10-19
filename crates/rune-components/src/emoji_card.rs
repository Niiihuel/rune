//! The card of an emoji of a message or of a reaction: its picture, its
//! name, whether it is a server's or a standard one, the server it comes
//! from and «Copiar emoji», as Serein shows it (2026-10-18).

use limn::{
    Button, ButtonVariant, Component, Element, FontFamily, Point, PopoverPlacement, RasterImage,
};
use rune_i18n::{t, tr};

use crate::Look;

/// The actions the card reports.
pub mod ids {
    /// Escape or a click outside it.
    pub const DISMISS: &str = "emoji-card:dismiss";
    pub const COPY: &str = "emoji-card:copy";
}

/// The card's width.
pub const WIDTH: f32 = 300.0;
/// Its picture's side.
pub const PICTURE: f32 = 48.0;

/// One emoji, as the card shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct EmojiCard {
    /// `:name:`, or the emoji itself when it has no name.
    pub name: String,
    pub image: Option<RasterImage>,
    /// A server's own, not a standard one.
    pub custom: bool,
    /// The server a server's emoji comes from, when the session knows it.
    pub server: Option<String>,
}

/// What the card says of `card`.
pub fn card(card: &EmojiCard, look: &Look) -> Element {
    let picture = match &card.image {
        Some(image) => Element::raster_image(image.clone()),
        None => crate::centered(crate::text(card.name.clone(), 20.0, look.muted)),
    }
    .size(Some(PICTURE), Some(PICTURE));
    let mut about = vec![Element::truncated_text(card.name.clone())
        .font_family(FontFamily::SansBold)
        .font_size(16.0)
        .foreground(look.text)];
    let wrapped = |value: String| {
        Element::text(value)
            .font_family(FontFamily::Sans)
            .font_size(13.0)
            .foreground(look.muted)
    };
    if card.custom {
        about.push(wrapped(t("Un emoji de un servidor.").into()));
        about.push(wrapped(match &card.server {
            Some(server) => tr!("De {server}", server),
            None => t("Su servidor no está en esta sesión.").into(),
        }));
    } else {
        about.push(wrapped(
            t("Un emoji estándar: se puede usar en cualquier conversación.").into(),
        ));
    }
    let head = Element::row(vec![picture, Element::column(about).gap(2.0).flex(1.0)]).gap(12.0);
    let copy = Button::new(ids::COPY, t("Copiar emoji"), look.menu_palette())
        .variant(ButtonVariant::Secondary)
        .render();
    Element::column(vec![
        head,
        Element::row(vec![Element::spacer().flex(1.0), copy]),
    ])
    .gap(12.0)
    .padding(16.0)
    .accessible(
        limn::Access::new(limn::Role::Group)
            .name(tr!("Detalle del emoji {name}", name = card.name)),
    )
}

/// `card` floating over `anchor` at `at`, under the emoji clicked; Escape
/// and a click outside report [`ids::DISMISS`].
pub fn popout(anchor: Element, card: Option<Element>, at: Point, look: &Look) -> Element {
    let Some(card) = card else {
        return anchor;
    };
    let surface = Element::panel(card)
        .size(Some(WIDTH), None)
        .background(look.menu)
        .border(look.border, 1.0)
        .radius(10.0)
        .shadow(look.shadow, Point::new(0.0, 9.0), 22.0, 0.0);
    Element::popover(anchor, surface, ids::DISMISS, PopoverPlacement::BottomStart)
        .popover_gap(6.0)
        .popover_at(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    #[test]
    fn a_servers_emoji_says_where_it_comes_from_and_a_standard_one_that_it_goes_anywhere() {
        let shown = |custom: bool, server: Option<&str>| {
            let (_, frame) = frame(
                card(
                    &EmojiCard {
                        name: ":wave:".into(),
                        image: None,
                        custom,
                        server: server.map(str::to_owned),
                    },
                    &Look::DARK,
                )
                .size(Some(WIDTH), None),
                Size::new(WIDTH, 200.0),
            );
            frame
        };
        let from = shown(true, Some("Rust en español"));
        let words = texts(&from);
        assert!(words.iter().any(|text| text == ":wave:"), "{words:?}");
        assert!(words.iter().any(|text| text.contains("Rust en español")));
        assert!(controls(&from).contains(&ids::COPY.to_owned()));
        // Its lines wrap: read them as one.
        let read = |frame: &limn::Frame| {
            texts(frame)
                .join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        };
        let unknown = read(&shown(true, None));
        assert!(unknown.contains("no está en esta sesión"), "{unknown}");
        let standard = read(&shown(false, None));
        assert!(standard.contains("emoji estándar"), "{standard}");
    }
}
