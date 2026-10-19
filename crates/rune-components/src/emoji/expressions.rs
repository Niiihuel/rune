//! Composer expression tabs use plain props and expose actions. Picking an
//! item never sends a message; the UI owns insertion and explicit confirmation.

use limn::{Button, ButtonVariant, Component, Element, Icon, RasterImage, ScrollArea, TextField};

use rune_i18n::t;

use super::PickerView;
use crate::{line, Look};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PickerTab {
    #[default]
    Emoji,
    Stickers,
    Gif,
}

pub struct CatalogItem {
    pub id: u64,
    pub name: String,
    pub image: Option<RasterImage>,
    pub unavailable: Option<String>,
    /// Where it comes from, a heading for it and those after it: another
    /// server, a pack (P5). `None` keeps the catalog's own title.
    pub section: Option<String>,
}

pub struct ExpressionView<'a> {
    pub tab: PickerTab,
    pub custom: &'a [CatalogItem],
    pub stickers: &'a [CatalogItem],
    pub gif_url: &'a str,
    pub gif_recent: &'a [String],
    pub gif_search: super::GifSearchView,
    /// The favorites' addresses, the first added first, for the tab
    /// without a search; with one, see [`super::GifSearchView::favorites`].
    pub gif_favorites: &'a [String],
    pub gif_favorites_scope: &'a str,
    pub feedback: Option<&'a str>,
}

pub(super) fn tabs(base: &str, selected: PickerTab, look: &Look) -> Element {
    let mut items = Vec::new();
    for (tab, id, label) in [
        (PickerTab::Emoji, "emoji", t("Emojis")),
        (PickerTab::Stickers, "stickers", "Stickers"),
        (PickerTab::Gif, "gif", "GIF"),
    ] {
        let palette = limn::ComponentPalette {
            raised: look.selected,
            ..look.menu_palette()
        };
        items.push(
            Button::new(format!("{base}:tab={id}"), label, palette)
                .variant(if tab == selected {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Ghost
                })
                .render(),
        );
    }
    Element::row(items).gap(8.0).size(None, Some(32.0))
}

pub(super) fn catalog(
    base: &str,
    kind: &str,
    title: &str,
    items: &[CatalogItem],
    query: &str,
    look: &Look,
) -> Element {
    let query = query.trim().trim_matches(':').to_lowercase();
    let matching = items
        .iter()
        .filter(|item| item.name.to_lowercase().contains(&query));
    let total = matching.clone().count();
    let found: Vec<_> = matching.take(72).collect();
    let side = if kind == "sticker" { 112.0 } else { 51.0 };
    let columns = if kind == "sticker" { 4 } else { 9 };
    let mut rows = Vec::new();
    // Each section under its heading, in the order given (P5).
    let mut sections: Vec<(&str, Vec<&CatalogItem>)> = Vec::new();
    for item in &found {
        let heading = item.section.as_deref().unwrap_or(title);
        match sections.last_mut() {
            Some((last, items)) if *last == heading => items.push(item),
            _ => sections.push((heading, vec![item])),
        }
    }
    if sections.is_empty() {
        rows.push(line(title, 14.0, look.text).padding(8.0));
    }
    for (heading, group) in sections.iter().flat_map(|(heading, items)| {
        items
            .chunks(columns)
            .enumerate()
            .map(move |(index, group)| ((index == 0).then_some(*heading), group))
    }) {
        if let Some(heading) = heading {
            rows.push(line(heading, 14.0, look.text).padding(8.0));
        }
        let cells = group
            .iter()
            .map(|item| {
                let id = format!("{base}:{kind}={}", item.id);
                let image = item.image.clone().map_or_else(
                    || {
                        crate::centered(
                            Element::icon(if kind == "sticker" {
                                Icon::Image
                            } else {
                                Icon::Emoji
                            })
                            .icon_size(24.0)
                            .foreground(look.faint),
                        )
                    },
                    |image| Element::raster_image(image).size(Some(side - 14.0), Some(side - 14.0)),
                );
                // A sticker without a picture RUNE draws shows its name.
                let image = if kind == "sticker" && item.image.is_none() {
                    Element::column(vec![
                        image.flex(1.0),
                        line(item.name.clone(), 11.0, look.muted),
                    ])
                    .padding(4.0)
                } else {
                    image
                };
                let content = Element::panel(crate::centered(image))
                    .size(Some(side), Some(side))
                    .radius(6.0);
                let tile = if let Some(reason) = &item.unavailable {
                    // Locked: a padlock over its corner says so at a glance (P5).
                    let lock = Element::panel(
                        Element::icon(Icon::Lock)
                            .foreground(look.muted)
                            .size(Some(12.0), Some(12.0)),
                    )
                    .padding(2.0)
                    .radius(4.0)
                    .background(look.menu);
                    Element::stack(vec![
                        content,
                        Element::column(vec![
                            Element::spacer().flex(1.0),
                            Element::row(vec![Element::spacer().flex(1.0), lock]),
                        ]),
                    ])
                    .size(Some(side), Some(side))
                    .accessible(
                        limn::Access::new(limn::Role::Button)
                            .name(format!("{}: {reason}", item.name))
                            .disabled(true),
                    )
                } else {
                    Element::clickable(id.clone(), content)
                        .hover_background(look.selected)
                        .accessible(limn::Access::new(limn::Role::Button).name(item.name.clone()))
                };
                let hint = item.unavailable.as_ref().map_or_else(
                    || item.name.clone(),
                    |reason| format!("{} — {reason}", item.name),
                );
                limn::Tooltip::new(tile, id, hint, look.menu_palette()).render()
            })
            .collect();
        rows.push(Element::row(cells));
    }
    if found.is_empty() {
        rows.push(line(t("No hay resultados."), 14.0, look.muted).padding(12.0));
    } else if total > found.len() {
        rows.push(
            line(
                t("Se muestran 72 resultados. Refiná la búsqueda para ver otros."),
                13.0,
                look.muted,
            )
            .padding(12.0),
        );
    }
    Element::column(rows)
}

pub(super) fn media_picker(view: &PickerView<'_>, look: &Look) -> Element {
    let expressions = view.expressions.as_ref().expect("media tab props exist");
    let base = view.base;
    let mut children = vec![Element::panel(tabs(base, expressions.tab, look)).padding(12.0)];
    if expressions.tab == PickerTab::Stickers {
        children.push(
            Element::panel(
                limn::SearchField::new(
                    super::ids::search(base),
                    view.query,
                    t("Buscar stickers"),
                    look.menu_palette(),
                )
                .render(),
            )
            .padding(12.0),
        );
        children.push(
            ScrollArea::new(
                format!("{base}:stickers"),
                catalog(
                    base,
                    "sticker",
                    t("Stickers del servidor"),
                    expressions.stickers,
                    view.query,
                    look,
                ),
            )
            .render()
            .flex(1.0),
        );
        children.push(
            line(
                t("Elegí un sticker y confirmá su envío en el composer."),
                13.0,
                look.muted,
            )
            .padding(12.0),
        );
    } else if expressions.gif_search.available {
        children.push(super::gifs::search(base, &expressions.gif_search, look));
    } else {
        children.push(line(t("Añadir un GIF por enlace"), 16.0, look.text).padding(12.0));
        children.push(
            Element::panel(
                TextField::new(
                    format!("{base}:gif-url"),
                    expressions.gif_url,
                    "https://…",
                    look.menu_palette(),
                )
                .render(),
            )
            .padding(12.0),
        );
        children.push(
            Element::panel(
                Button::new(
                    format!("{base}:gif-insert"),
                    t("Añadir al borrador"),
                    look.menu_palette(),
                )
                .enabled(!expressions.gif_url.trim().is_empty())
                .render(),
            )
            .padding(12.0),
        );
        children.push(
            line(
                t("Pegá un enlace HTTPS. Búsqueda sin configurar."),
                13.0,
                look.muted,
            )
            .padding(12.0),
        );
        if let Some(feedback) = expressions.feedback {
            children.push(line(feedback, 13.0, look.danger).padding(12.0));
        }
        let mut recent = vec![
            super::gifs::favorites(
                base,
                expressions.gif_favorites,
                expressions.gif_favorites_scope,
                look,
            ),
            line(t("Enlaces usados en esta sesión"), 14.0, look.text).padding(12.0),
        ];
        for (index, url) in expressions.gif_recent.iter().take(12).enumerate() {
            let favorite = expressions.gif_favorites.contains(url);
            recent.push(
                Element::row(vec![
                    Button::new(
                        format!("{base}:gif-recent={index}"),
                        super::gifs::short(url),
                        look.menu_palette(),
                    )
                    .variant(ButtonVariant::Ghost)
                    .render()
                    .flex(1.0),
                    Button::new(
                        format!("{base}:gif-fav-recent={index}"),
                        "",
                        look.menu_palette(),
                    )
                    .icon(if favorite {
                        Icon::HeartFill
                    } else {
                        Icon::Heart
                    })
                    .variant(ButtonVariant::Ghost)
                    .accessibility_label(if favorite {
                        t("Quitar de favoritos")
                    } else {
                        t("Agregar a favoritos")
                    })
                    .render(),
                ])
                .gap(4.0),
            );
        }
        children.push(
            ScrollArea::new(format!("{base}:gif-recents"), Element::column(recent))
                .render()
                .flex(1.0),
        );
    }
    let (width, height) = super::picker_size(true);
    Element::column(children)
        .size(Some(width - 12.0), Some(height - 12.0))
        .radius(8.0)
        .background(look.menu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame};

    #[test]
    fn media_tab_padding_does_not_collapse_button_hit_areas() {
        for tab in [PickerTab::Stickers, PickerTab::Gif] {
            let view = PickerView {
                base: "picker",
                query: "",
                recent: &[],
                placeholder: "Buscar",
                tabs: true,
                expressions: Some(ExpressionView {
                    tab,
                    custom: &[],
                    stickers: &[],
                    gif_url: "https://example.com/a.gif",
                    gif_recent: &[],
                    gif_search: Default::default(),
                    gif_favorites: &[],
                    gif_favorites_scope: "",
                    feedback: None,
                }),
            };
            let (_, laid) = frame(
                media_picker(&view, &Look::DARK),
                limn::Size::new(530.0, 530.0),
            );
            for id in ["picker:tab=emoji", "picker:tab=stickers", "picker:tab=gif"] {
                let region = laid.regions.iter().find(|region| region.id == id).unwrap();
                assert!(region.rect.height >= 30.0, "{id}: {:?}", region.rect);
            }
            if tab == PickerTab::Gif {
                let region = laid
                    .regions
                    .iter()
                    .find(|region| region.id == "picker:gif-insert")
                    .unwrap();
                assert!(region.rect.height >= 30.0, "{:?}", region.rect);
            }
        }
    }

    #[test]
    fn real_tabs_expose_actions_and_unavailable_catalog_items_cannot_be_clicked() {
        let entries = [
            CatalogItem {
                section: Default::default(),
                id: 7,
                name: "wave".into(),
                image: None,
                unavailable: None,
            },
            CatalogItem {
                section: Default::default(),
                id: 8,
                name: "blocked".into(),
                image: None,
                unavailable: Some("No disponible".into()),
            },
        ];
        let (_, laid) = frame(
            Element::column(vec![
                tabs("picker", PickerTab::Emoji, &Look::DARK),
                catalog("picker", "custom", "Servidor", &entries, "", &Look::DARK),
            ]),
            limn::Size::new(600.0, 400.0),
        );
        let ids = controls(&laid);
        for expected in [
            "picker:tab=emoji",
            "picker:tab=stickers",
            "picker:tab=gif",
            "picker:custom=7",
        ] {
            assert!(ids.contains(&expected.to_owned()), "{expected}");
        }
        assert!(!ids.contains(&"picker:custom=8".to_owned()));
    }
}
