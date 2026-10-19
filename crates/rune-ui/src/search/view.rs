use limn::{
    Button, ButtonVariant, Checkbox, Component, ControlSize, DatePicker, DropdownOption, Element,
    Icon, ListRow, Modal, Popover, PopoverPlacement, ScrollArea, SearchField, Select,
};
use rune_components::{line, Look};
use rune_i18n::{t, tr};

use super::*;
use crate::ViewContext;

fn date(search: &MessageSearch, from: bool, look: &Look) -> Element {
    let (base, open, value, title) = if from {
        ("message-search:from", Menu::From, &search.from, t("Desde"))
    } else {
        (
            "message-search:until",
            Menu::Until,
            &search.until,
            t("Hasta"),
        )
    };
    let label = if value.is_empty() {
        title.to_owned()
    } else {
        format!("{title}: {value}")
    };
    let trigger = Button::new(format!("{base}-open"), label, look.menu_palette())
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Compact)
        .render();
    let mut picker = DatePicker::new(base, value, look.menu_palette())
        .accessibility_label(tr!("{title}, fecha local", title));
    let (year, month) = search.month.unwrap_or_else(|| time::picker_month(value));
    picker = picker.display_month(year, month);
    Popover::new(
        trigger,
        Element::column(vec![
            picker.render(),
            Button::new(
                format!("{base}:clear"),
                t("Quitar filtro de fecha"),
                look.menu_palette(),
            )
            .variant(ButtonVariant::Ghost)
            .render(),
        ])
        .gap(6.0),
        search.menu == Some(open),
        "message-search:menu-close",
        look.menu_palette(),
    )
    .width(280.0)
    .render()
    .flex(1.0)
}

pub(crate) fn view(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let Some(search) = &ui.search else {
        return root;
    };
    if !readable(search, ui, store) {
        return root;
    }
    let name = store
        .channel(search.channel)
        .map(|channel| channel.name.as_str())
        .unwrap_or("");
    let provider = store
        .provider
        .as_ref()
        .map_or(t("el proveedor"), |provider| t(&provider.name));
    let server = store
        .channel(search.channel)
        .and_then(|channel| channel.server)
        .and_then(|server| store.server(server));
    let description = match search.scope {
        Scope::Loaded => tr!("Sólo los {count} mensajes cargados de {name}. Flechas para elegir, Enter para ir. Ctrl+K cambia de conversación.", count = search.source.len(), name),
        Scope::Device => tr!("Mensajes guardados de {name} en este equipo. El historial local puede estar incompleto.", name),
        Scope::Remote => tr!(
            "Todo el historial de {place} en {provider}, con filtros como from:, mentions:, has:, in:, before: o pinned:. Enter busca; lo cargado se muestra mientras escribís.",
            place = server.map_or(name, |server| server.shown_name()),
            provider
        ),
    };
    let status = if let Some(error) = search.form_error {
        error.into()
    } else if search.scope == Scope::Remote {
        remote::status(search, store)
    } else if let Some(cached) = &search.cached {
        match cached.phase {
            Phase::Loading => t("Buscando en el historial guardado…").into(),
            Phase::Dirty => t("Los mensajes cambiaron. Actualizá la búsqueda.").into(),
            Phase::Failed(error) => error.label().into(),
            Phase::Ready
                if search.hits.is_empty()
                    && cached
                        .pages
                        .back()
                        .is_some_and(|page| page.continuation.is_some()) =>
            {
                t("Sin coincidencias en este tramo. Podés seguir buscando.").into()
            }
            Phase::Ready => tr!(
                "{count} resultados en esta página · página {page} · más recientes primero",
                count = search.hits.len(),
                page = cached.skipped + cached.page + 1
            ),
        }
    } else if search.query.trim().is_empty()
        && search.author.is_none()
        && !search.files
        && !search.pinned
        && search.from.is_empty()
        && search.until.is_empty()
    {
        t("Escribí texto o elegí filtros para buscar en esta conversación.").into()
    } else if search.hits.is_empty() {
        t("Sin resultados en los mensajes cargados.").into()
    } else {
        let mut line = tr!(
            "{count} resultados · más recientes primero",
            count = search.hits.len()
        );
        if search.files {
            line.push_str(t(" · archivos adjuntos, sin stickers"));
        }
        line
    };
    let data = search.hits.clone();
    let selected = search.selected;
    let wall = context.wall;
    let palette_look = *look;
    // Where each result is, when not here, and where it came from.
    let places: Arc<Vec<String>> = Arc::new(
        data.iter()
            .map(|hit| {
                let mut place = String::new();
                if hit.data.channel != search.channel {
                    let channel = store
                        .channel(hit.data.channel)
                        .map_or(t("canal no disponible"), |channel| channel.name.as_str());
                    place.push_str(&format!(" · #{channel}"));
                }
                match hit.source {
                    Source::Remote => place.push_str(&format!(" · {provider}")),
                    Source::Both => place.push_str(&tr!(" · {provider} y cargado", provider)),
                    Source::Loaded if search.scope == Scope::Remote => {
                        place.push_str(t(" · cargado"))
                    }
                    _ => {}
                }
                place
            })
            .collect(),
    );
    let results = ScrollArea::uniform_list(RESULTS, data.len(), 52.0, move |index| {
        let hit = &data[index];
        ListRow::new(
            format!("message-search:go:{}:{}", hit.data.channel, hit.data.id),
            &hit.author,
            palette_look.menu_palette(),
        )
        .meta(format!(
            "{}{}",
            time::stamp(hit.data.timestamp, wall),
            places[index]
        ))
        // limn-gap: a `ListRow`'s subtitle is only text, and formatted text
        // has no «…» (`max_lines`, limn PR #82), so a server's emoji reads
        // `:name:` here (`SearchHit::from_message`), not its picture.
        .subtitle(&hit.data.snippet)
        .filled()
        .title_ink(if index == selected {
            palette_look.accent
        } else {
            palette_look.text
        })
        .trailing_icon(if index == selected {
            Icon::ArrowRight
        } else {
            Icon::ChevronRight
        })
        .render()
        .size(None, Some(52.0))
    })
    .appearance(look.scrollbar(true))
    .render()
    .size(
        None,
        Some((context.window.height - 420.0).clamp(44.0, 280.0)),
    );
    let scope = Select::new(
        "message-search:scope",
        match search.scope {
            Scope::Loaded => t("Cargados"),
            Scope::Device => t("En este equipo"),
            Scope::Remote => t("Todo el historial"),
        },
        vec![
            DropdownOption::new("message-search:scope:loaded", t("Cargados"))
                .selected(search.scope == Scope::Loaded),
            DropdownOption::new("message-search:scope:device", t("En este equipo"))
                .enabled(ui.local_history_available)
                .selected(search.scope == Scope::Device),
            DropdownOption::new("message-search:scope:remote", t("Todo el historial"))
                .enabled(store.remote_search(search.channel).allowed())
                .selected(search.scope == Scope::Remote),
        ],
        search.menu == Some(Menu::Scope),
        "message-search:menu-close",
        look.menu_palette(),
    )
    .accessibility_label(t("Dónde buscar mensajes"))
    .width(164.0)
    .render();
    let author_trigger = Button::new(
        "message-search:author",
        search
            .author
            .and_then(|author| store.user(author))
            .map(|user| user.name.as_str())
            .unwrap_or(t("Todos los autores")),
        look.menu_palette(),
    )
    .variant(ButtonVariant::Secondary)
    .size(ControlSize::Compact)
    .render();
    let author_body = if search.menu == Some(Menu::Author) {
        let entries: Vec<_> = search
            .author_options
            .iter()
            .filter_map(|author| store.user(*author).map(|user| (*author, user.name.clone())))
            .collect();
        let palette = look.menu_palette();
        Element::column(vec![
            SearchField::new(
                "message-search:author-query",
                &search.author_query,
                t("Buscar autor conocido"),
                palette,
            )
            .size(ControlSize::Compact)
            .accessibility_label(t("Filtrar autores conocidos por nombre"))
            .render(),
            Button::new("message-search:author:all", t("Todos los autores"), palette)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Compact)
                .render(),
            ScrollArea::uniform_list(
                "message-search:authors",
                entries.len(),
                32.0,
                move |index| {
                    ListRow::new(
                        format!("message-search:author:{}", entries[index].0),
                        &entries[index].1,
                        palette,
                    )
                    .render()
                    .size(None, Some(32.0))
                },
            )
            .render()
            .size(None, Some(192.0)),
            line(
                t("Hasta 64 autores conocidos. Escribí para filtrar."),
                12.0,
                look.muted,
            ),
        ])
        .gap(6.0)
    } else {
        Element::spacer()
    };
    let author = Popover::new(
        author_trigger,
        author_body,
        search.menu == Some(Menu::Author),
        "message-search:menu-close",
        look.menu_palette(),
    )
    .width(300.0)
    .render()
    .flex(1.0);
    let mut body = vec![
        Element::row(vec![
            SearchField::new(
                QUERY,
                &search.query,
                t("Texto del mensaje"),
                look.menu_palette(),
            )
            .accessibility_label(t("Buscar mensajes de esta conversación"))
            .shortcut("Ctrl+F")
            .render()
            .flex(1.0),
            scope,
        ])
        .gap(8.0),
        offered(search, store, look),
        Element::row(
            [
                (
                    "mine",
                    t("Solo míos"),
                    search.author.is_some() && search.author == store.me(),
                ),
                ("files", t("Con archivos"), search.files),
                ("pinned", t("Fijados"), search.pinned),
            ]
            .into_iter()
            .map(|(id, label, checked)| {
                Checkbox::new(format!("message-search:{id}"), checked, look.menu_palette())
                    .label(label)
                    .size(ControlSize::Compact)
                    .render()
                    .flex(1.0)
            })
            .collect(),
        )
        .gap(8.0),
        Element::row(vec![
            author,
            date(search, true, look),
            date(search, false, look),
        ])
        .gap(8.0),
        // The whole history's line says more: it wraps.
        if search.scope == Scope::Remote {
            rune_components::text(status, 13.0, look.muted)
        } else {
            line(status, 13.0, look.muted)
        },
        results,
    ];
    if search.scope == Scope::Remote {
        let (previous, next) = remote::pages(search);
        let relevance = search.order == rune_model::RemoteOrder::Relevance;
        let order = Select::new(
            "message-search:order",
            if relevance {
                t("Más relevantes")
            } else {
                t("Más recientes")
            },
            vec![
                DropdownOption::new("message-search:order:newest", t("Más recientes"))
                    .selected(!relevance),
                DropdownOption::new("message-search:order:relevance", t("Más relevantes"))
                    .selected(relevance),
            ],
            search.menu == Some(Menu::Order),
            "message-search:menu-close",
            look.menu_palette(),
        )
        .accessibility_label(t("Orden de los resultados"))
        .width(164.0)
        // The dialog's last row: its menu opens up, inside the dialog.
        .placement(PopoverPlacement::TopStart)
        .render();
        body.push(
            Element::row(vec![
                order,
                Element::spacer().flex(1.0),
                Button::new(
                    "message-search:previous",
                    t("Anterior"),
                    look.menu_palette(),
                )
                .size(ControlSize::Compact)
                .enabled(previous)
                .render(),
                Button::new(
                    "message-search:refresh",
                    t("Actualizar"),
                    look.menu_palette(),
                )
                .size(ControlSize::Compact)
                .enabled(search.remote.is_some())
                .render(),
                Button::new("message-search:next", t("Siguiente"), look.menu_palette())
                    .size(ControlSize::Compact)
                    .enabled(next)
                    .render(),
            ])
            .gap(8.0),
        );
    }
    if search.scope == Scope::Device {
        let cached = search.cached.as_ref();
        let ready = cached.is_some_and(|cached| matches!(cached.phase, Phase::Ready));
        let previous = ready && cached.is_some_and(|cached| cached.page > 0);
        let next = ready
            && cached.is_some_and(|cached| {
                cached.page + 1 < cached.pages.len()
                    || cached
                        .pages
                        .back()
                        .is_some_and(|page| page.continuation.is_some())
            });
        body.push(
            Element::row(vec![
                Button::new(
                    "message-search:previous",
                    t("Anterior"),
                    look.menu_palette(),
                )
                .size(ControlSize::Compact)
                .enabled(previous)
                .render(),
                Button::new(
                    "message-search:refresh",
                    t("Actualizar"),
                    look.menu_palette(),
                )
                .size(ControlSize::Compact)
                .enabled(ui.local_history_available)
                .render(),
                Button::new("message-search:next", t("Siguiente"), look.menu_palette())
                    .size(ControlSize::Compact)
                    .enabled(next)
                    .render(),
            ])
            .gap(8.0),
        );
    }
    if let Some(status) = crate::context::status(ui, look) {
        body.push(status);
    }
    Modal::new(
        root,
        true,
        t("Buscar mensajes"),
        description,
        "message-search:close",
        look.menu_palette(),
    )
    .width(Modal::fit_width(context.window.width, 620.0))
    .body(Element::column(body).gap(8.0))
    .initial_focus(QUERY)
    .render()
}

/// The filters the field suggests for what is being written, under it.
fn offered(search: &MessageSearch, store: &Store, look: &Look) -> Element {
    let found = suggestions(search, store);
    if found.is_empty() {
        return Element::spacer().size(None, Some(0.0));
    }
    let chosen = search.suggestion % found.len();
    let palette = look.menu_palette();
    let rows: Vec<Element> = found
        .into_iter()
        .take(9)
        .enumerate()
        .map(|(index, item)| {
            ListRow::new(
                format!("message-search:suggest:{index}"),
                &item.label,
                palette,
            )
            .icon(item.icon)
            .sizing(limn::ListRowSizing::COMPACT)
            .meta(&item.detail)
            .title_ink(if index == chosen {
                look.accent
            } else {
                look.text
            })
            .render()
        })
        .collect();
    Element::column(
        std::iter::once(line(
            t("Filtros · ↑↓ para elegir, Enter o Tab para usar, Esc para cerrar"),
            12.0,
            look.muted,
        ))
        .chain(rows)
        .collect(),
    )
    .gap(2.0)
}
