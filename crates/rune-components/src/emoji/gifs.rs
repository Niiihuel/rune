//! A small, provider-neutral GIF page. Every item stays in service order, and
//! thumbnails are optional; a failed image never removes a search result.

use crate::{line, Look};
use limn::{
    Button, ButtonVariant, Color, Component, Element, Icon, RasterImage, ScrollArea, SearchField,
};
use rune_i18n::{t, tr};

pub struct GifSearchItem {
    pub id: String,
    pub title: String,
    pub creator: Option<String>,
    pub image: Option<RasterImage>,
    /// It is among the favorites: its heart is filled.
    pub favorite: bool,
}

#[derive(Default)]
pub struct GifSearchView {
    pub available: bool,
    pub query: String,
    pub loading: bool,
    pub error: Option<String>,
    pub items: Vec<GifSearchItem>,
    pub has_page: bool,
    pub has_next: bool,
    pub has_previous: bool,
    pub attribution: Option<String>,
    pub attribution_image: Option<RasterImage>,
    /// The favorites' addresses, the first added first.
    pub favorites: Vec<String>,
    /// Where the favorites are kept, as the tab says it.
    pub favorites_scope: String,
    // Presence, identity and expressions (P5).
    /// What is trending to search for, with a picture: each searches its
    /// name (`{base}:gif:category={i}`).
    pub categories: Vec<(String, Option<RasterImage>)>,
    /// What to search for, as the service suggests it for what is written
    /// (`{base}:gif:suggest={i}`).
    pub suggestions: Vec<String>,
    /// The searches to choose from when there are two: each one's key, its
    /// name, and whether it is the one in use (`{base}:gif:source={key}`).
    pub sources: Vec<(String, String, bool)>,
}

/// The favorites of the GIF tab, with where they are kept: each one adds
/// its address to the draft (`{base}:gif-favorite={i}`) or leaves the list
/// (`{base}:gif-unfavorite={i}`).
pub(super) fn favorites(base: &str, favorites: &[String], scope: &str, look: &Look) -> Element {
    let mut rows = vec![Element::row(vec![
        line(t("Favoritos"), 14.0, look.text).centered_y(),
        line(format!("· {scope}"), 12.0, look.muted)
            .flex(1.0)
            .centered_y(),
    ])
    .gap(6.0)];
    if favorites.is_empty() {
        rows.push(line(
            t("Tocá el corazón de un GIF para guardarlo acá."),
            13.0,
            look.muted,
        ));
    }
    for (index, url) in favorites.iter().enumerate() {
        rows.push(
            Element::row(vec![
                Button::new(
                    format!("{base}:gif-favorite={index}"),
                    short(url),
                    look.menu_palette(),
                )
                .variant(ButtonVariant::Ghost)
                .render()
                .flex(1.0),
                Button::new(
                    format!("{base}:gif-unfavorite={index}"),
                    "",
                    look.menu_palette(),
                )
                .icon(Icon::HeartFill)
                .variant(ButtonVariant::Ghost)
                .accessibility_label(t("Quitar de favoritos"))
                .tooltip(t("Quitar de favoritos"))
                .render(),
            ])
            .gap(4.0),
        );
    }
    Element::column(rows).gap(4.0).padding(8.0)
}

/// An address cut to fit a row.
pub(super) fn short(url: &str) -> String {
    if url.chars().count() > 62 {
        format!("{}…", url.chars().take(61).collect::<String>())
    } else {
        url.to_owned()
    }
}

pub(super) fn search(base: &str, view: &GifSearchView, look: &Look) -> Element {
    let prefix = format!("{base}:gif");
    let search = Element::row(vec![
        SearchField::new(
            format!("{prefix}:query"),
            &view.query,
            t("Buscar GIF"),
            look.menu_palette(),
        )
        .render()
        .flex(1.0),
        Button::new(format!("{prefix}:search"), t("Buscar"), look.menu_palette()).render(),
    ])
    .gap(8.0)
    .size(None, Some(36.0));
    let mut children = vec![search];
    let mut chips = vec![Button::new(
        format!("{prefix}:trending"),
        t("Tendencias"),
        look.menu_palette(),
    )
    .variant(ButtonVariant::Ghost)
    .render()];
    if view.sources.len() > 1 {
        chips.push(Element::spacer().flex(1.0));
        for (key, label, chosen) in &view.sources {
            chips.push(
                Button::new(format!("{prefix}:source={key}"), label, look.menu_palette())
                    .variant(if *chosen {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Ghost
                    })
                    .accessibility_label(tr!("Buscar en {label}", label))
                    .render(),
            );
        }
    }
    children.push(Element::row(chips).gap(4.0).size(None, Some(32.0)));
    if !view.suggestions.is_empty() {
        children.push(
            Element::row(
                view.suggestions
                    .iter()
                    .enumerate()
                    .map(|(index, term)| {
                        Button::new(
                            format!("{prefix}:suggest={index}"),
                            term,
                            look.menu_palette(),
                        )
                        .variant(ButtonVariant::Ghost)
                        .icon(Icon::Search)
                        .render()
                    })
                    .collect(),
            )
            .gap(4.0)
            .wrap(),
        );
    }
    let mut rows = vec![favorites(
        base,
        &view.favorites,
        &view.favorites_scope,
        look,
    )];
    if let Some(error) = &view.error {
        rows.push(line(error, 13.0, look.danger).padding(8.0));
    }
    // What is trending to search for, as tiles of their pictures (P5).
    for group in view.categories.chunks(3).enumerate() {
        let (row, group) = group;
        rows.push(
            Element::row(
                group
                    .iter()
                    .enumerate()
                    .map(|(column, (name, image))| {
                        let index = row * 3 + column;
                        let ground = match image {
                            Some(image) => Element::raster_image(image.clone().cover())
                                .size(Some(150.0), Some(64.0)),
                            None => Element::spacer().size(Some(150.0), Some(64.0)),
                        };
                        let tile = Element::stack(vec![
                            Element::panel(ground)
                                .radius(8.0)
                                .background(look.raised)
                                .clip_children(),
                            Element::panel(crate::centered(
                                Element::text(name.clone())
                                    .font_family(limn::FontFamily::SansBold)
                                    .font_size(14.0)
                                    .foreground(Color::WHITE),
                            ))
                            .radius(8.0)
                            .background(crate::Look::alpha(Color::rgb(0, 0, 0), 110)),
                        ])
                        .size(Some(150.0), Some(64.0));
                        Element::clickable(format!("{prefix}:category={index}"), tile)
                            .radius(8.0)
                            .accessible(
                                limn::Access::new(limn::Role::Button)
                                    .name(tr!("Buscar {name}", name)),
                            )
                    })
                    .collect(),
            )
            .gap(6.0),
        );
    }
    if view.loading {
        rows.push(line(t("Buscando GIF…"), 14.0, look.muted).padding(12.0));
    } else if view.items.is_empty() {
        rows.push(
            line(
                if view.has_page {
                    t("No hay resultados.")
                } else {
                    t("Buscá un GIF o explorá tendencias.")
                },
                14.0,
                look.muted,
            )
            .padding(12.0),
        );
    }
    for group in view.items.chunks(4) {
        let items = group
            .iter()
            .map(|item| {
                let image = item
                    .image
                    .clone()
                    .map_or_else(
                        || {
                            crate::centered(
                                Element::icon(Icon::Image)
                                    .icon_size(28.0)
                                    .foreground(look.muted),
                            )
                        },
                        Element::raster_image,
                    )
                    .size(Some(108.0), Some(108.0));
                let title = if item.title.is_empty() {
                    "GIF"
                } else {
                    &item.title
                };
                let mut content = vec![image];
                if let Some(creator) = &item.creator {
                    content.push(
                        Element::truncated_text(format!("@{creator}"))
                            .font_size(11.0)
                            .foreground(look.muted)
                            .size(Some(108.0), Some(16.0)),
                    );
                }
                let id = format!("{prefix}:pick={}", item.id);
                let tile = Element::clickable(id.clone(), Element::column(content).gap(2.0))
                    .size(Some(112.0), Some(130.0))
                    .radius(6.0)
                    .hover_background(look.selected)
                    .accessible(
                        limn::Access::new(limn::Role::Button)
                            .name(tr!("Añadir {title} al borrador", title)),
                    );
                let heart =
                    Button::new(format!("{prefix}:fav={}", item.id), "", look.menu_palette())
                        .icon(if item.favorite {
                            Icon::HeartFill
                        } else {
                            Icon::Heart
                        })
                        .variant(ButtonVariant::Ghost)
                        .accessibility_label(if item.favorite {
                            tr!("Quitar {title} de favoritos", title)
                        } else {
                            tr!("Agregar {title} a favoritos", title)
                        })
                        .render();
                Element::column(vec![
                    limn::Tooltip::new(tile, id, title, look.menu_palette()).render(),
                    heart,
                ])
            })
            .collect();
        rows.push(Element::row(items).gap(4.0));
    }
    children.push(
        ScrollArea::new(format!("{prefix}:results"), Element::column(rows).gap(4.0))
            .render()
            .flex(1.0),
    );
    let mut footer = vec![
        Button::new(format!("{prefix}:first"), t("Inicio"), look.menu_palette())
            .variant(ButtonVariant::Ghost)
            .enabled(view.has_previous && !view.loading)
            .render(),
        Button::new(format!("{prefix}:next"), t("Más GIF"), look.menu_palette())
            .variant(ButtonVariant::Ghost)
            .enabled(view.has_next && !view.loading)
            .render(),
        Element::spacer().flex(1.0),
    ];
    if let Some(attribution) = &view.attribution {
        let mark = if let Some(image) = &view.attribution_image {
            Element::clickable(
                format!("{prefix}:attribution"),
                Element::raster_image(image.clone()).size(Some(200.0), Some(42.0)),
            )
            .accessible(limn::Access::new(limn::Role::Link).name(attribution.clone()))
        } else {
            Button::new(
                format!("{prefix}:attribution"),
                attribution,
                look.menu_palette(),
            )
            .variant(ButtonVariant::Ghost)
            .render()
        };
        footer.push(mark);
    }
    children.push(Element::row(footer).gap(4.0).size(None, Some(42.0)));
    Element::column(children).gap(8.0).padding(12.0).flex(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};

    #[test]
    fn bounded_grid_keeps_fallback_items_and_visible_attribution_with_native_scroll() {
        let view = GifSearchView {
            available: true,
            has_page: true,
            has_next: true,
            items: (0..20)
                .map(|index| GifSearchItem {
                    id: format!("7:{index}"),
                    title: format!("GIF {index}"),
                    creator: Some("artist".into()),
                    image: None,
                    favorite: index == 1,
                })
                .collect(),
            attribution: Some("Powered By GIPHY".into()),
            favorites: vec!["https://example.com/fav.gif".into()],
            favorites_scope: "Guardados sólo en este equipo".into(),
            ..Default::default()
        };
        let (_, frame) = frame(
            Element::column(vec![search("picker", &view, &Look::ONYX)])
                .size(Some(506.0), Some(440.0)),
            limn::Size::new(530.0, 530.0),
        );
        let ids = controls(&frame);
        for id in [
            "picker:gif:query",
            "picker:gif:search",
            "picker:gif:trending",
            "picker:gif:pick=7:0",
            "picker:gif:fav=7:0",
            "picker:gif-favorite=0",
            "picker:gif-unfavorite=0",
            "picker:gif:next",
            "picker:gif:attribution",
        ] {
            assert!(ids.contains(&id.to_owned()), "{id}");
        }
        assert!(texts(&frame).iter().any(|text| text == "Powered By GIPHY"));
        let search = frame
            .regions
            .iter()
            .find(|region| region.id == "picker:gif:search")
            .unwrap();
        let footer = frame
            .regions
            .iter()
            .find(|region| region.id == "picker:gif:attribution")
            .unwrap();
        assert!(search.rect.height >= 30.0 && footer.rect.height >= 30.0);
        assert!(footer.rect.y > search.rect.y + search.rect.height);
    }
}
