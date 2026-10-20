//! A message search's results: a card for each message with its author's
//! face and name, when it was written and where it came from, its text with
//! the words searched marked, and its pictures and files; the headings that
//! group the cards by channel or by day; and the pills of the filters.

use limn::{
    Access, Button, ButtonVariant, Chip, Color, Component, ControlSize, Element, FontFamily, Icon,
    RasterImage, Role, TextSpan,
};
use rune_i18n::t;

use crate::{avatar_with_image, gap, icon, line, strong_line, Look};

/// The side of a result's face.
const FACE: f32 = 36.0;
/// The side of a picture's thumbnail under a result's text.
const THUMB: f32 = 64.0;
/// The most lines of a result's text before «…».
const LINES: usize = 4;

/// What starts a group of results.
#[derive(Clone, Debug, PartialEq)]
pub enum Heading {
    /// A channel's name, without `#`.
    Channel(String),
    /// A day, as `time::day_label` says it.
    Day(String),
}

/// One result, as its card shows it.
#[derive(Clone, Debug)]
pub struct ResultCard {
    /// What choosing the card reports: it opens the message.
    pub id: String,
    pub name: String,
    /// The color of the author's top role, when the account shows them.
    pub name_ink: Option<Color>,
    /// The author's own color, behind their initial while there is no picture.
    pub accent: Color,
    pub face: Option<RasterImage>,
    /// When it was written: "Hoy a las 14:05".
    pub when: String,
    /// Where it came from, when the search mixes sources.
    pub source: Option<String>,
    pub text: String,
    pub pinned: bool,
    /// The pictures it carries, in order; `None` while one loads.
    pub pictures: Vec<Option<RasterImage>>,
    /// The names of its other files, or a word for files not listed.
    pub files: Vec<String>,
    /// The keyboard's choice: Enter opens it.
    pub selected: bool,
    /// The group it starts, if it is the first of one.
    pub heading: Option<Heading>,
}

/// `text` as runs of `size` in `ink`, with every place one of `terms` is
/// found, in any case, on `mark`.
pub fn marked(text: &str, terms: &[String], size: f32, ink: Color, mark: Color) -> Vec<TextSpan> {
    let terms: Vec<Vec<char>> = terms
        .iter()
        .map(|term| term.chars().flat_map(char::to_lowercase).collect())
        .filter(|term: &Vec<char>| !term.is_empty())
        .collect();
    // Where a term starts at `at`, the byte it ends at in `text`.
    let found = |at: usize| -> Option<usize> {
        terms
            .iter()
            .filter_map(|term| {
                let mut wanted = term.iter();
                let mut end = at;
                let mut pending = Vec::new();
                for (offset, found) in text[at..].char_indices() {
                    pending.extend(found.to_lowercase());
                    for lowered in pending.drain(..) {
                        match wanted.next() {
                            Some(expected) if *expected == lowered => {}
                            Some(_) => return None,
                            None => return Some(end),
                        }
                    }
                    end = at + offset + found.len_utf8();
                    if wanted.len() == 0 {
                        return Some(end);
                    }
                }
                None
            })
            .max()
    };
    let span = |piece: &str| TextSpan::new(piece, ink, size, FontFamily::Sans);
    let mut spans = Vec::new();
    let (mut plain, mut at) = (0, 0);
    while at < text.len() {
        match found(at) {
            Some(end) => {
                if plain < at {
                    spans.push(span(&text[plain..at]));
                }
                spans.push(span(&text[at..end]).background(mark));
                (plain, at) = (end, end);
            }
            None => at += text[at..].chars().next().map_or(1, char::len_utf8),
        }
    }
    if plain < text.len() {
        spans.push(span(&text[plain..]));
    }
    spans
}

/// The line over the first result of a group.
fn heading(heading: &Heading, look: &Look) -> Element {
    let (glyph, label) = match heading {
        Heading::Channel(name) => (Icon::Hash, name.clone()),
        Heading::Day(day) => (Icon::Clock, day.clone()),
    };
    Element::row(vec![
        icon(glyph, look.muted)
            .size(Some(14.0), Some(14.0))
            .centered_y(),
        strong_line(label, 12.0, look.muted).flex(1.0).centered_y(),
    ])
    .gap(6.0)
    .padding(4.0)
    .accessible(Access::new(Role::Heading))
}

/// A result's card. The card is one control: choosing it opens the message,
/// as its «Ir» says.
pub fn card(card: &ResultCard, terms: &[String], look: &Look) -> Element {
    let name_ink = card.name_ink.unwrap_or(look.text);
    let mut top = vec![
        strong_line(&card.name, 14.0, name_ink).centered_y(),
        line(&card.when, 12.0, look.muted).centered_y(),
    ];
    if let Some(source) = &card.source {
        top.push(
            Chip::new(source, look.menu_palette())
                .size(ControlSize::Dense)
                .outline()
                .render()
                .centered_y(),
        );
    }
    if card.pinned {
        top.push(
            icon(Icon::Pin, look.muted)
                .size(Some(12.0), Some(12.0))
                .centered_y(),
        );
    }
    top.push(Element::spacer().flex(1.0));
    let go = Chip::new(t("Ir"), look.menu_palette())
        .size(ControlSize::Compact)
        .icon(Icon::ArrowRight);
    top.push(
        if card.selected {
            go.color(look.accent)
        } else {
            go.outline().color(look.faint)
        }
        .render()
        .centered_y(),
    );
    let mut body = vec![
        Element::row(top).gap(8.0).size(None, Some(20.0)),
        if card.text.trim().is_empty() {
            line(t("Sin texto"), 13.0, look.faint)
        } else {
            Element::formatted_text(marked(
                &card.text,
                terms,
                14.0,
                look.text,
                Look::alpha(look.accent, 90),
            ))
            .max_lines(LINES)
        },
    ];
    if !card.pictures.is_empty() || !card.files.is_empty() {
        let mut media: Vec<Element> = card
            .pictures
            .iter()
            .map(|picture| {
                match picture {
                    Some(picture) => Element::raster_image(picture.clone().cover()),
                    None => Element::panel(Element::spacer()).background(look.card),
                }
                .size(Some(THUMB), Some(THUMB))
                .radius(6.0)
            })
            .collect();
        media.extend(card.files.iter().map(|file| {
            Chip::new(file, look.menu_palette())
                .icon(Icon::Paperclip)
                .render()
                .centered_y()
        }));
        body.push(Element::row(media).gap(6.0).wrap());
    }
    let said = format!("{}, {}: {}", card.name, card.when, card.text);
    let tile = Element::clickable(
        &card.id,
        Element::row(vec![
            avatar_with_image(&card.name, card.accent, FACE, card.face.clone()),
            Element::column(body).gap(4.0).flex(1.0),
        ])
        .gap(12.0),
    )
    .padding(10.0)
    .radius(8.0)
    .background(if card.selected {
        look.selected
    } else {
        look.card
    })
    .hover_background(if card.selected {
        look.selected
    } else {
        look.hover
    })
    .border(
        if card.selected {
            Look::alpha(look.accent, 160)
        } else {
            look.border
        },
        1.0,
    )
    .accessible(Access::new(Role::Button).name(said).selected(card.selected));
    let mut parts = Vec::new();
    if let Some(found) = &card.heading {
        parts.push(heading(found, look));
    }
    parts.push(tile);
    parts.push(gap(0.0, 6.0));
    Element::column(parts)
}

/// A filter as a pill: its icon, what it says now, and whether it is on.
/// `menu` adds the arrow of a pill that opens a choice.
pub fn pill(id: &str, glyph: Icon, label: &str, on: bool, menu: bool, look: &Look) -> Element {
    let mut button = Button::new(id, label, look.menu_palette())
        .variant(ButtonVariant::Outline)
        .size(ControlSize::Compact)
        .icon(glyph)
        .active(on);
    if menu {
        button = button.trailing_icon(Icon::ChevronDown);
    }
    if on {
        button = button.ink(look.accent);
    }
    button.render()
}

/// [`pill`] with a picture in place of its icon, such as the face of the
/// person a search is filtered by.
pub fn face_pill(id: &str, face: RasterImage, label: &str, look: &Look) -> Element {
    Button::new(id, label, look.menu_palette())
        .variant(ButtonVariant::Outline)
        .size(ControlSize::Compact)
        .image(face.cover())
        .trailing_icon(Icon::ChevronDown)
        .active(true)
        .ink(look.accent)
        .render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    fn sample(selected: bool, heading: Option<Heading>) -> ResultCard {
        ResultCard {
            id: "message-search:go:1:2".into(),
            name: "Ana".into(),
            name_ink: Some(Color::rgb(200, 40, 40)),
            accent: Color::rgb(10, 20, 30),
            face: None,
            when: "Hoy a las 14:05".into(),
            source: Some("cargado".into()),
            text: "Una AGUJA en el pajar, otra aguja".into(),
            pinned: true,
            pictures: vec![None],
            files: vec!["notas.txt".into()],
            selected,
            heading,
        }
    }

    #[test]
    fn marked_finds_every_term_in_any_case_and_keeps_the_text() {
        let ink = Color::rgb(1, 1, 1);
        let mark = Color::rgb(9, 9, 9);
        let spans = marked(
            "Una AGUJA en el pajar, otra aguja",
            &["aguja".into()],
            14.0,
            ink,
            mark,
        );
        let joined: String = spans.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(joined, "Una AGUJA en el pajar, otra aguja");
        let shown: Vec<_> = spans
            .iter()
            .filter(|span| span.background == Some(mark))
            .map(|span| span.text.as_str())
            .collect();
        assert_eq!(shown, ["AGUJA", "aguja"]);
        // Several words, accents and letters whose lower case is longer.
        let spans = marked(
            "Árbol İzmir canción",
            &["árbol".into(), "CANCIÓN".into(), "i̇z".into()],
            14.0,
            ink,
            mark,
        );
        let shown: Vec<_> = spans
            .iter()
            .filter(|span| span.background.is_some())
            .map(|span| span.text.as_str())
            .collect();
        assert_eq!(shown, ["Árbol", "İz", "canción"]);
        assert_eq!(marked("nada", &[], 14.0, ink, mark).len(), 1);
        assert!(marked("", &["x".into()], 14.0, ink, mark).is_empty());
    }

    #[test]
    fn a_card_shows_face_name_time_source_text_media_and_its_action() {
        let look = Look::ONYX;
        let (_, laid) = frame(
            card(
                &sample(true, Some(Heading::Channel("general".into()))),
                &["aguja".into()],
                &look,
            ),
            Size::new(560.0, 240.0),
        );
        let painted = texts(&laid);
        for expected in [
            "general",
            "A",
            "Ana",
            "Hoy a las 14:05",
            "cargado",
            "AGUJA",
            "Ir",
            "notas.txt",
        ] {
            assert!(
                painted.iter().any(|text| text == expected),
                "{expected}: {painted:?}"
            );
        }
        assert_eq!(controls(&laid), ["message-search:go:1:2"]);
        let name = laid
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::Text { text, color, .. } if text == "Ana" => Some(*color),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            name,
            Color::rgb(200, 40, 40),
            "the name takes its role's color"
        );
    }

    #[test]
    fn pills_say_their_state_and_open_menus() {
        let look = Look::ONYX;
        let (_, laid) = frame(
            Element::row(vec![
                pill("f:author", Icon::Users, "De: Ana", true, true, &look),
                pill("f:pinned", Icon::Pin, "Fijados", false, false, &look),
            ]),
            Size::new(400.0, 40.0),
        );
        assert_eq!(controls(&laid), ["f:author", "f:pinned"]);
        assert!(texts(&laid).contains(&"De: Ana".to_owned()));
    }
}
