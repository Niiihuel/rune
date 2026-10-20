//! A forum in place of a timeline: its tools (a new post, the order, list or
//! gallery, a filter by tag), then its posts as cards, each with its title,
//! tags, who started it, how many replies and its last activity, and the
//! archived ones read so far. A click on a card opens the post, which is a
//! thread: its own timeline and composer.

use limn::{
    Button, ButtonVariant, Chip, Component, ControlSize, Element, Empty, FontFamily, Icon,
    ScrollArea, Toggle, ToggleGroup, ToggleItem, ToggleVariant,
};

use rune_i18n::{t, tr};

use crate::{gap, icon, line, strong_line, text, Look};

/// The widest a card gets.
const CARD_WIDTH: f32 = 760.0;

/// The action that opens the post `thread`.
pub fn post_id(thread: u64) -> String {
    format!("post:{thread}")
}

/// A post as the forum lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PostCard {
    /// The thread it is: its card opens it.
    pub thread: u64,
    pub title: String,
    /// Its tags as shown: emoji and name.
    pub tags: Vec<String>,
    pub author: String,
    pub replies: u32,
    /// When something last happened in it, as a person reads it.
    pub activity: String,
    pub archived: bool,
    pub locked: bool,
    pub unread: bool,
    // Threads, forums and stages (2026-10-18): what it shows of its first
    // message, as Serein reads it.
    /// The first words; empty for none.
    pub excerpt: String,
    /// The first picture, once loaded.
    pub image: Option<limn::RasterImage>,
    /// The forum's own reaction or the one most given: emoji and count.
    pub reaction: Option<(String, u32)>,
}

/// The actions of a forum's tools; they start with `th:`, like every action
/// about threads.
pub mod ids {
    /// Order by last activity (`activity`) or by creation (`created`).
    pub fn sort(forum: u64, created: bool) -> String {
        format!(
            "th:sort:{forum}:{}",
            if created { "created" } else { "activity" }
        )
    }

    /// Show the posts as a list or as a gallery.
    pub fn layout(forum: u64, gallery: bool) -> String {
        format!(
            "th:layout:{forum}:{}",
            if gallery { "gallery" } else { "list" }
        )
    }

    /// Show only posts with `tag`, or take it out of the filter.
    pub fn filter(forum: u64, tag: u64) -> String {
        format!("th:filter:{forum}:{tag}")
    }

    /// Show every post again.
    pub fn clear(forum: u64) -> String {
        format!("th:filter:{forum}:clear")
    }
}

/// A forum's tools: start a post, order and lay out its posts, and filter
/// them by tag.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ForumBar {
    pub forum: u64,
    /// Why no post can be started, when none can.
    pub start_denied: Option<String>,
    pub by_creation: bool,
    pub gallery: bool,
    /// Each tag: its id, emoji and name, and whether the filter shows it.
    pub tags: Vec<(u64, String, bool)>,
}

/// A forum as it opens: its tools, its active posts, and the archived ones
/// read so far with the button that asks for more.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ForumView {
    pub id: String,
    pub bar: ForumBar,
    pub cards: Vec<PostCard>,
    pub archived: Vec<PostCard>,
    /// The button for more archived posts: its label and why not, when not.
    pub more: (String, Option<String>),
    /// Said instead of the cards when there are none.
    pub empty: String,
}

fn toolbar(bar: &ForumBar, look: &Look) -> Element {
    let palette = look.menu_palette();
    let start = Button::new(
        crate::threads::ids::start(bar.forum),
        t("Nueva publicación"),
        look.accent_palette(),
    )
    .size(ControlSize::Compact)
    .enabled(bar.start_denied.is_none())
    .render();
    let order = ToggleGroup::new(
        format!("th:sort:{}", bar.forum),
        vec![
            ToggleItem::new(
                ids::sort(bar.forum, false),
                t("Actividad reciente"),
                !bar.by_creation,
            ),
            ToggleItem::new(
                ids::sort(bar.forum, true),
                t("Fecha de creación"),
                bar.by_creation,
            ),
        ],
        palette,
    )
    .size(ControlSize::Compact)
    .accessibility_label(t("Ordenar publicaciones"))
    .render();
    let layout = ToggleGroup::new(
        format!("th:layout:{}", bar.forum),
        vec![
            ToggleItem::icon_only(ids::layout(bar.forum, false), Icon::Menu, !bar.gallery)
                .tooltip(t("Lista")),
            ToggleItem::icon_only(ids::layout(bar.forum, true), Icon::Grid, bar.gallery)
                .tooltip(t("Galería")),
        ],
        palette,
    )
    .size(ControlSize::Compact)
    .accessibility_label(t("Vista de las publicaciones"))
    .render();
    // Wrapped, so a narrow forum keeps every tool in view.
    let mut rows = vec![Element::row(vec![start, order, layout]).gap(8.0).wrap()];
    if let Some(reason) = &bar.start_denied {
        rows.push(text(reason.clone(), 12.0, look.muted));
    }
    if !bar.tags.is_empty() {
        let mut chips: Vec<Element> = bar
            .tags
            .iter()
            .map(|(tag, label, on)| {
                Toggle::new(ids::filter(bar.forum, *tag), label.clone(), *on, palette)
                    .variant(ToggleVariant::Outline)
                    .size(ControlSize::Compact)
                    .render()
            })
            .collect();
        if bar.tags.iter().any(|(_, _, on)| *on) {
            chips.push(
                Button::new(ids::clear(bar.forum), t("Ver todas"), palette)
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Compact)
                    .render(),
            );
        }
        rows.push(Element::row(chips).gap(6.0).wrap());
    }
    Element::column(rows).gap(8.0)
}

/// Cards side by side, as many as fit `width`, each at least 240 wide.
fn gallery(cards: Vec<PostCard>, width: f32, look: &Look) -> Element {
    const GAP: f32 = 8.0;
    let columns = (((width + GAP) / (240.0 + GAP)).floor() as usize).clamp(1, 4);
    let card_width = (width - GAP * (columns - 1) as f32) / columns as f32;
    let mut rows = Vec::new();
    let mut cards = cards.into_iter().peekable();
    while cards.peek().is_some() {
        let mut row: Vec<Element> = cards
            .by_ref()
            .take(columns)
            .map(|post| card(post, card_width, true, look))
            .collect();
        while row.len() < columns {
            row.push(gap(card_width, 0.0));
        }
        rows.push(Element::row(row).gap(GAP));
    }
    Element::column(rows).gap(GAP)
}

/// A forum `width` wide: its tools, then its posts as cards.
pub fn view(view: ForumView, width: f32, look: &Look) -> Element {
    let content_width = (width - 32.0).clamp(200.0, CARD_WIDTH.max(width - 32.0));
    let content_width = if view.bar.gallery {
        content_width
    } else {
        content_width.min(CARD_WIDTH)
    };
    let gallery_on = view.bar.gallery;
    let list = |cards: Vec<PostCard>| {
        if gallery_on {
            gallery(cards, content_width, look)
        } else {
            Element::column(
                cards
                    .into_iter()
                    .map(|post| card(post, content_width, false, look))
                    .collect(),
            )
            .gap(8.0)
        }
    };
    let mut parts = vec![toolbar(&view.bar, look)];
    if view.cards.is_empty() && view.archived.is_empty() {
        parts.push(gap(0.0, 24.0));
        parts.push(
            Empty::new(t("Sin publicaciones"), view.empty.clone(), look.palette())
                .icon(Icon::Chat)
                .render(),
        );
    } else if !view.cards.is_empty() {
        parts.push(list(view.cards));
    }
    if !view.archived.is_empty() {
        parts.push(gap(0.0, 8.0));
        parts.push(strong_line(
            tr!("Archivadas — {count}", count = view.archived.len()),
            13.0,
            look.muted,
        ));
        parts.push(list(view.archived));
    }
    let (label, denied) = &view.more;
    let mut more = vec![Button::new(
        crate::threads::ids::more(view.bar.forum),
        label.clone(),
        look.menu_palette(),
    )
    .variant(ButtonVariant::Secondary)
    .size(ControlSize::Compact)
    .enabled(denied.is_none())
    .render()];
    if let Some(reason) = denied {
        more.push(
            text(reason.clone(), 12.0, look.muted)
                .shrink_to_fit()
                .centered_y(),
        );
    }
    parts.push(Element::row(more).gap(10.0));
    ScrollArea::new(
        view.id,
        Element::column(vec![
            gap(0.0, 16.0),
            Element::row(vec![
                gap(16.0, 0.0),
                Element::column(parts)
                    .gap(12.0)
                    .size(Some(content_width), None),
                Element::spacer().flex(1.0),
            ]),
            gap(0.0, 16.0),
        ]),
    )
    .appearance(look.chat_scrollbar())
    .render()
}

/// The side of a list card's picture.
const THUMBNAIL: f32 = 72.0;

/// One post's card, `width` wide: in a gallery its picture goes on top,
/// in a list beside the text.
pub fn card(post: PostCard, width: f32, gallery: bool, look: &Look) -> Element {
    let ink = if post.unread || !post.archived {
        look.text
    } else {
        look.muted
    };
    let mut title = Vec::new();
    if post.locked {
        title.push(icon(Icon::Lock, look.faint).centered_y());
        title.push(gap(6.0, 0.0));
    }
    if post.archived {
        title.push(icon(Icon::Archive, look.faint).centered_y());
        title.push(gap(6.0, 0.0));
    }
    title.push(strong_line(&post.title, 16.0, ink).flex(1.0).centered_y());
    let state = match (post.locked, post.archived) {
        (true, _) => Some(t("Bloqueada")),
        (false, true) => Some(t("Archivada")),
        _ => None,
    };
    if let Some(state) = state {
        title.push(gap(8.0, 0.0));
        title.push(line(state, 13.0, look.faint).centered_y());
    }
    if post.unread {
        title.push(gap(8.0, 0.0));
        title.push(
            Element::panel(Element::spacer())
                .size(Some(8.0), Some(8.0))
                .radius(4.0)
                .background(look.text)
                .centered_y(),
        );
    }
    let mut lines = vec![Element::row(title).size(None, Some(22.0))];
    if !post.tags.is_empty() {
        lines.push(
            Element::row(
                post.tags
                    .iter()
                    .map(|tag| {
                        Chip::new(tag.clone(), look.palette())
                            .size(ControlSize::Dense)
                            .render()
                    })
                    .collect(),
            )
            .gap(6.0),
        );
    }
    let replies = match post.replies {
        1 => t("1 mensaje").to_owned(),
        n => tr!("{n} mensajes", n),
    };
    if !post.excerpt.is_empty() {
        lines.push(text(post.excerpt.clone(), 14.0, look.muted).max_lines(2));
    }
    // Who and how many keep their room; the time gives way first.
    let mut meta = vec![
        line(&post.author, 13.0, look.muted).font_family(FontFamily::SansMedium),
        gap(10.0, 0.0),
        icon(Icon::Chat, look.faint).centered_y(),
        gap(4.0, 0.0),
        line(replies, 13.0, look.muted),
        gap(10.0, 0.0),
        line(&post.activity, 13.0, look.faint).flex(1.0),
    ];
    if let Some((emoji, count)) = &post.reaction {
        meta.push(gap(8.0, 0.0));
        meta.push(reaction(emoji, *count, look));
    }
    lines.push(Element::row(meta).size(None, Some(22.0)));
    let body = Element::column(vec![
        gap(0.0, 12.0),
        Element::column(lines).gap(8.0),
        gap(0.0, 12.0),
    ])
    .flex(1.0);
    let picture = |image: limn::RasterImage, width: f32, height: f32| {
        Element::panel(Element::raster_image(image.cover()).size(Some(width), Some(height)))
            .size(Some(width), Some(height))
            .radius(6.0)
    };
    let content = match post.image {
        Some(image) if gallery => {
            let inner = (width - 16.0).max(0.0);
            Element::column(vec![
                gap(0.0, 8.0),
                Element::row(vec![
                    gap(8.0, 0.0),
                    picture(image, inner, (inner * 9.0 / 16.0).min(180.0)),
                ]),
                Element::row(vec![gap(16.0, 0.0), body, gap(16.0, 0.0)]),
            ])
        }
        Some(image) => Element::row(vec![
            gap(16.0, 0.0),
            body,
            gap(12.0, 0.0),
            picture(image, THUMBNAIL, THUMBNAIL).centered_y(),
            gap(12.0, 0.0),
        ]),
        None => Element::row(vec![gap(16.0, 0.0), body, gap(16.0, 0.0)]),
    };
    Element::clickable(post_id(post.thread), content)
        .size(Some(width), None)
        .radius(8.0)
        .background(look.raised)
        .hover_background(look.hover)
        .border(Look::alpha(look.border, 160), 1.0)
        .accessible(limn::Access::new(limn::Role::ListItem).name(post.title.clone()))
}

/// The reaction a card shows: its emoji and how many gave it; with none
/// (0), the emoji alone.
fn reaction(emoji: &str, count: u32, look: &Look) -> Element {
    let mut parts = vec![Element::text(emoji.to_owned())
        .font_family(FontFamily::Sans)
        .font_size(14.0)
        .foreground(look.text)
        .centered_y()];
    if count > 0 {
        parts.push(
            Element::text(count.to_string())
                .font_family(FontFamily::SansMedium)
                .font_size(13.0)
                .foreground(look.muted)
                .centered_y(),
        );
    }
    Element::panel(Element::row(parts).gap(4.0).padding(4.0))
        .size(None, Some(22.0))
        .radius(6.0)
        .background(look.hover)
        .accessible(limn::Access::new(limn::Role::Image).name(if count > 0 {
            format!("{emoji} {count}")
        } else {
            emoji.to_owned()
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    fn post(thread: u64, title: &str) -> PostCard {
        PostCard {
            thread,
            title: title.into(),
            tags: vec!["❓ Pregunta".into(), "📷 Equipo".into()],
            author: "Ada".into(),
            replies: 24,
            activity: "Hoy a las 14:05".into(),
            ..PostCard::default()
        }
    }

    fn forum(cards: Vec<PostCard>, gallery: bool) -> ForumView {
        ForumView {
            id: "forum:5".into(),
            bar: ForumBar {
                forum: 5,
                gallery,
                tags: vec![
                    (70, "❓ Pregunta".into(), true),
                    (71, "📷 Equipo".into(), false),
                ],
                ..ForumBar::default()
            },
            cards,
            archived: Vec::new(),
            more: ("Ver archivadas".into(), None),
            empty: "Todavía no hay nada.".into(),
        }
    }

    #[test]
    fn each_post_is_a_control_with_its_title_tags_author_and_replies() {
        let locked = PostCard {
            locked: true,
            ..post(8, "Fotos movidas con poca luz")
        };
        let (_, frame) = frame(
            view(
                forum(vec![post(7, "¿Qué lente?"), locked], false),
                900.0,
                &Look::DARK,
            ),
            Size::new(900.0, 600.0),
        );
        let ids = controls(&frame);
        for id in [
            "post:7",
            "post:8",
            "th:start:5",
            "th:more:5",
            "th:sort:5:created",
            "th:layout:5:gallery",
            "th:filter:5:71",
            "th:filter:5:clear",
        ] {
            assert!(ids.contains(&id.to_owned()), "{id}: {ids:?}");
        }
        let painted = texts(&frame);
        for text in [
            "¿Qué lente?",
            "❓ Pregunta",
            "Ada",
            "24 mensajes",
            "Hoy a las 14:05",
            "Bloqueada",
        ] {
            assert!(painted.contains(&text.to_owned()), "{text}: {painted:?}");
        }
    }

    #[test]
    fn a_gallery_puts_cards_side_by_side_and_archived_ones_come_after() {
        let mut view_props = forum(vec![post(7, "Uno"), post(8, "Dos"), post(9, "Tres")], true);
        view_props.archived = vec![PostCard {
            archived: true,
            ..post(10, "Vieja")
        }];
        let (_, frame) = frame(
            view(view_props, 900.0, &Look::DARK),
            Size::new(900.0, 700.0),
        );
        let rect = |id: &str| frame.regions.iter().find(|r| r.id == id).unwrap().rect;
        let (one, two, old) = (rect("post:7"), rect("post:8"), rect("post:10"));
        assert!(
            (one.y - two.y).abs() < 0.5 && two.x > one.x,
            "{one:?} {two:?}"
        );
        assert!(old.y > one.y + one.height);
        assert!(texts(&frame).contains(&"Archivadas — 1".to_owned()));
    }

    #[test]
    fn a_card_shows_its_first_words_reaction_and_picture() {
        let image = limn::RasterImage::new(
            "1".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "foto",
        )
        .unwrap();
        let shown = PostCard {
            excerpt: "Busco algo luminoso para interiores".into(),
            image: Some(image),
            reaction: Some(("👍".into(), 4)),
            ..post(7, "¿Qué lente?")
        };
        for gallery in [false, true] {
            let (_, frame) = frame(
                view(forum(vec![shown.clone()], gallery), 900.0, &Look::DARK),
                Size::new(900.0, 600.0),
            );
            let painted = texts(&frame);
            for text in ["Busco algo luminoso para interiores", "👍", "4"] {
                assert!(painted.contains(&text.to_owned()), "{text}: {painted:?}");
            }
            let pictures: Vec<limn::Rect> = frame
                .commands
                .iter()
                .filter_map(|command| match command {
                    limn::DrawCommand::RasterImage { rect, image, .. } if image.alt() == "foto" => {
                        Some(*rect)
                    }
                    _ => None,
                })
                .collect();
            let [picture] = pictures.as_slice() else {
                panic!("{pictures:?}");
            };
            let card = frame
                .regions
                .iter()
                .find(|r| r.id == "post:7")
                .unwrap()
                .rect;
            // Beside the text in a list; over it, as wide as the card, in a gallery.
            if gallery {
                assert!(picture.width > 200.0 && picture.y < card.y + 16.0);
            } else {
                assert_eq!((picture.width, picture.height), (THUMBNAIL, THUMBNAIL));
                assert!(picture.x > card.x + card.width / 2.0);
            }
        }
    }

    #[test]
    fn a_forum_without_posts_says_so() {
        let (_, frame) = frame(
            view(forum(Vec::new(), false), 600.0, &Look::DARK),
            Size::new(600.0, 400.0),
        );
        let painted = texts(&frame);
        assert!(
            painted.contains(&"Sin publicaciones".to_owned()),
            "{painted:?}"
        );
        assert!(controls(&frame).contains(&"th:more:5".to_owned()));
    }
}
