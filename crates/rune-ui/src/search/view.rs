//! The search dialog: the field and where it looks, the filters as pills,
//! and the results as cards grouped by channel or by day, with a state of
//! their own while there is nothing, something is asked, or nothing matched.

use std::collections::HashMap;

use limn::{
    Button, ButtonVariant, Checkbox, Component, ControlSize, DatePicker, DropdownOption, Element,
    Empty, Icon, ListRow, Modal, Popover, ScrollArea, SearchField, Select, Spinner,
};
use rune_components::search::{self as cards, Heading, ResultCard};
use rune_components::{line, Look};
use rune_i18n::{t, tr};

use super::*;
use crate::ViewContext;

/// How a person shows on a result and in the author menu.
#[derive(Clone)]
struct Face {
    name: String,
    handle: String,
    ink: Option<limn::Color>,
    accent: limn::Color,
    image: Option<limn::RasterImage>,
}

fn face(ui: &Ui, store: &Store, user: UserId, server: Option<ServerId>, look: &Look) -> Face {
    let found = store.user(user);
    Face {
        name: found
            .map(|_| store.display_name(user, server))
            .unwrap_or_else(|| t("Usuario desconocido").into()),
        handle: found.map(|user| user.name.clone()).unwrap_or_default(),
        ink: server
            .and_then(|server| store.top_role(user, server))
            .and_then(|role| role.color)
            .filter(|_| ui.prefs.role_colors)
            .map(|color| look.ink(color)),
        accent: found.map_or(look.accent, |user| look.ink(user.accent)),
        image: ui.picture(store.avatar_in(server, user)),
    }
}

/// "Enlace" for "enlace".
fn capital(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

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
    let trigger = cards::pill(
        &format!("{base}-open"),
        Icon::Clock,
        &label,
        !value.is_empty(),
        true,
        look,
    );
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
}

/// Who wrote it: the pill shows the person chosen, with their face.
fn author(ui: &Ui, store: &Store, search: &MessageSearch, look: &Look) -> Element {
    let server = store.channel(search.channel).and_then(|found| found.server);
    let chosen = search
        .author
        .map(|author| face(ui, store, author, server, look));
    let trigger = match chosen {
        Some(Face {
            name,
            image: Some(image),
            ..
        }) => cards::face_pill(
            "message-search:author",
            image,
            &tr!("De: {name}", name),
            look,
        ),
        Some(chosen) => cards::pill(
            "message-search:author",
            Icon::Users,
            &tr!("De: {name}", name = chosen.name),
            true,
            true,
            look,
        ),
        None => cards::pill(
            "message-search:author",
            Icon::Users,
            t("Autor"),
            false,
            true,
            look,
        ),
    };
    let body = if search.menu == Some(Menu::Author) {
        let entries: Vec<(UserId, Face)> = search
            .author_options
            .iter()
            .filter(|author| store.user(**author).is_some())
            .map(|author| (*author, face(ui, store, *author, server, look)))
            .collect();
        let palette = look.menu_palette();
        let chosen = search.author;
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
            ListRow::new("message-search:author:all", t("Todos los autores"), palette)
                .icon(Icon::Users)
                .sizing(limn::ListRowSizing::COMPACT)
                .render(),
            ScrollArea::uniform_list(
                "message-search:authors",
                entries.len(),
                36.0,
                move |index| {
                    let (id, face) = &entries[index];
                    let mut row =
                        ListRow::new(format!("message-search:author:{id}"), &face.name, palette)
                            .leading(rune_components::avatar_with_image(
                                &face.name,
                                face.accent,
                                24.0,
                                face.image.clone(),
                            ))
                            .meta(format!("@{}", face.handle));
                    if let Some(ink) = face.ink {
                        row = row.title_ink(ink);
                    }
                    if chosen == Some(*id) {
                        row = row.trailing_icon(Icon::Check);
                    }
                    row.render().size(None, Some(36.0))
                },
            )
            .render()
            .size(None, Some(216.0)),
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
    Popover::new(
        trigger,
        body,
        search.menu == Some(Menu::Author),
        "message-search:menu-close",
        look.menu_palette(),
    )
    .width(300.0)
    .render()
}

/// What the messages carry: files everywhere, the rest where the messages
/// themselves are at hand.
fn carries(search: &MessageSearch, look: &Look) -> Element {
    let chosen: Vec<&str> = search
        .files
        .then_some(Has::File)
        .into_iter()
        .chain(search.has.iter().copied())
        .map(Has::label)
        .collect();
    let label = match chosen.as_slice() {
        [] => t("Tiene").to_owned(),
        [one] => tr!("Tiene: {what}", what = *one),
        many => tr!("Tiene: {count}", count = many.len()),
    };
    let trigger = cards::pill(
        "message-search:has",
        Icon::Paperclip,
        &label,
        !chosen.is_empty(),
        true,
        look,
    );
    let body = if search.menu == Some(Menu::Has) {
        let palette = look.menu_palette();
        let mut rows: Vec<Element> = Has::ALL
            .into_iter()
            .map(|has| {
                let (id, on) = if has == Has::File {
                    ("message-search:files".to_owned(), search.files)
                } else {
                    (
                        format!("message-search:has:{}", has.key()),
                        search.has.contains(&has),
                    )
                };
                Checkbox::new(id, on, palette)
                    .label(capital(has.label()))
                    .size(ControlSize::Compact)
                    .enabled(has == Has::File || search.scope != Scope::Device)
                    .render()
            })
            .collect();
        if search.scope == Scope::Device {
            rows.push(line(
                t("Lo guardado en este equipo sólo sabe de archivos."),
                12.0,
                look.muted,
            ));
        }
        Element::column(rows).gap(4.0)
    } else {
        Element::spacer()
    };
    Popover::new(
        trigger,
        body,
        search.menu == Some(Menu::Has),
        "message-search:menu-close",
        look.menu_palette(),
    )
    .width(220.0)
    .render()
}

/// What the results area shows.
enum Shown {
    /// Nothing written or chosen yet.
    Idle,
    /// Something is asked: what is waited for.
    Busy(String),
    /// It cannot be searched like this: why.
    Problem(String),
    /// Nothing matched: a title and what to try.
    Nothing(String, String),
    /// Results, and the line over them.
    Results(String),
}

fn shown(search: &MessageSearch, store: &Store) -> Shown {
    let empty = search.hits.is_empty();
    if let Some(error) = search.form_error {
        return Shown::Problem(error.into());
    }
    match search.scope {
        Scope::Remote => {
            let status = remote::status(search, store);
            let asked = search
                .remote
                .as_ref()
                .filter(|asked| asked.asked == remote::signature(search));
            match asked.map(|asked| &asked.phase) {
                Some(remote::Phase::Loading | remote::Phase::Indexing { .. }) if empty => {
                    Shown::Busy(status)
                }
                Some(remote::Phase::Failed(_)) if empty => Shown::Problem(status),
                Some(remote::Phase::Ready(_)) if empty => {
                    Shown::Nothing(t("Sin resultados").into(), status)
                }
                _ if empty && search.idle() => Shown::Idle,
                _ if empty => Shown::Nothing(t("Sin resultados entre los cargados").into(), status),
                _ => Shown::Results(status),
            }
        }
        Scope::Device => match search.cached.as_ref().map(|cached| (cached, &cached.phase)) {
            None => Shown::Idle,
            Some((_, Phase::Loading)) => {
                Shown::Busy(t("Buscando en el historial guardado…").into())
            }
            Some((_, Phase::Failed(error))) => Shown::Problem(error.label().into()),
            Some((_, Phase::Dirty)) => Shown::Nothing(
                t("Los mensajes cambiaron").into(),
                t("Los mensajes cambiaron. Actualizá la búsqueda.").into(),
            ),
            Some((cached, Phase::Ready)) if empty => Shown::Nothing(
                t("Sin resultados").into(),
                if cached
                    .pages
                    .back()
                    .is_some_and(|page| page.continuation.is_some())
                {
                    t("Sin coincidencias en este tramo. Podés seguir buscando.")
                } else {
                    t("Probá con otras palabras o quitá filtros.")
                }
                .into(),
            ),
            Some((cached, Phase::Ready)) => Shown::Results(tr!(
                "{count} resultados en esta página · página {page} · más recientes primero",
                count = search.hits.len(),
                page = cached.skipped + cached.page + 1
            )),
        },
        Scope::Loaded if search.idle() => Shown::Idle,
        Scope::Loaded if empty => Shown::Nothing(
            t("Sin resultados").into(),
            if !search.query.trim().is_empty() && store.remote_search(search.channel).allowed() {
                t("Nada de lo cargado coincide. Enter busca en todo el historial.")
            } else {
                t("Nada de lo cargado coincide. Probá con otras palabras o quitá filtros.")
            }
            .into(),
        ),
        Scope::Loaded => {
            let mut line = tr!(
                "{count} resultados · más recientes primero",
                count = search.hits.len()
            );
            if search.files {
                line.push_str(t(" · archivos adjuntos, sin stickers"));
            }
            Shown::Results(line)
        }
    }
}

/// The words a result's text marks: the phrase searched among loaded or
/// kept messages, the words beside the filters in the whole history.
fn terms(search: &MessageSearch) -> Vec<String> {
    if search.scope == Scope::Remote {
        search
            .query
            .split_whitespace()
            .filter(|word| !word.contains(':'))
            .map(str::to_owned)
            .collect()
    } else {
        vec![search.query.trim().to_owned()]
    }
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
    let channel = store.channel(search.channel);
    let server_id = channel.and_then(|channel| channel.server);
    let place = match channel {
        Some(channel) if channel.server.is_some() => format!("#{}", channel.name),
        _ => store.channel_title(search.channel),
    };
    let provider = store
        .provider
        .as_ref()
        .map_or(t("el proveedor"), |provider| t(&provider.name));
    let server = server_id.and_then(|server| store.server(server));
    let description = match search.scope {
        Scope::Loaded => tr!(
            "En {place} · {count} mensajes cargados",
            place = place.as_str(),
            count = search.source.len()
        ),
        Scope::Device => tr!(
            "En {place} · guardados en este equipo, pueden faltar mensajes",
            place = place.as_str()
        ),
        Scope::Remote => tr!(
            "En todo {place} · {provider}",
            place = server.map_or(place.as_str(), |server| server.shown_name()),
            provider
        ),
    };
    let palette = look.menu_palette();
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
        palette,
    )
    .accessibility_label(t("Dónde buscar mensajes"))
    .width(164.0)
    .render();
    let mine = search.author.is_some() && search.author == store.me();
    let mut pills = vec![
        author(ui, store, search, look),
        cards::pill(
            "message-search:mine",
            Icon::At,
            t("Míos"),
            mine,
            false,
            look,
        ),
        carries(search, look),
        cards::pill(
            "message-search:pinned",
            Icon::Pin,
            t("Fijados"),
            search.pinned,
            false,
            look,
        ),
        date(search, true, look),
        date(search, false, look),
    ];
    let filtered = search.author.is_some()
        || search.files
        || search.pinned
        || !search.has.is_empty()
        || !search.from.is_empty()
        || !search.until.is_empty();
    if filtered {
        pills.push(
            Button::new("message-search:clear-filters", t("Quitar filtros"), palette)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Compact)
                .icon(Icon::Close)
                .render(),
        );
    }
    // The dialog as tall as the window lets it, up to a limit: the results
    // take what the rest leaves them.
    let card = (context.window.height - 48.0).clamp(160.0, 900.0);
    let mut body = vec![
        Element::row(vec![
            SearchField::new(QUERY, &search.query, t("Texto del mensaje"), palette)
                .accessibility_label(t("Buscar mensajes de esta conversación"))
                .shortcut("Ctrl+F")
                .render()
                .flex(1.0),
            scope,
        ])
        .gap(8.0),
        offered(search, store, look),
        Element::row(pills).gap(6.0).wrap(),
    ];
    let state = |element: Element| element.flex(1.0);
    let shown = shown(search, store);
    // The whole history's order, once something was asked, over the results.
    if search.scope == Scope::Remote
        && (search.remote.is_some() || matches!(shown, Shown::Results(_)))
    {
        let status = match &shown {
            Shown::Results(status) => status.clone(),
            _ => String::new(),
        };
        body.push(
            Element::row(vec![
                rune_components::text(status, 12.0, look.muted)
                    .flex(1.0)
                    .centered_y(),
                order(search, look),
            ])
            .gap(8.0),
        );
    } else if let Shown::Results(status) = &shown {
        body.push(rune_components::text(status.clone(), 12.0, look.muted));
    }
    match shown {
        Shown::Idle => body.push(state(
            Empty::new(
                tr!("Buscá en {place}", place = place.as_str()),
                if search.scope == Scope::Remote {
                    t("Escribí y apretá Enter. Filtros: from:, has:, in:, before:, after:, pinned:.")
                } else {
                    t("Escribí una palabra o elegí un filtro.")
                },
                palette,
            )
            .icon(Icon::Search)
            .compact()
            .render(),
        )),
        Shown::Busy(status) => body.push(state(
            Empty::new(t("Buscando mensajes…"), status, palette)
                .action_element(
                    Spinner::new("message-search:busy", look.muted)
                        .size(20.0)
                        .render(),
                )
                .compact()
                .render(),
        )),
        Shown::Problem(reason) => body.push(state(
            Empty::new(t("No se pudo buscar"), reason, palette)
                .icon(Icon::WarningTriangle)
                .compact()
                .render(),
        )),
        Shown::Nothing(title, hint) => body.push(state(
            Empty::new(title, hint, palette)
                .icon(Icon::Search)
                .compact()
                .render(),
        )),
        Shown::Results(_) => {
            body.push(results(ui, store, search, look, context).flex(1.0));
        }
    }
    if let Some((previous, next, refresh)) = pager(ui, search) {
        body.push(
            Element::row(vec![
                Button::new("message-search:previous", t("Anterior"), palette)
                    .size(ControlSize::Compact)
                    .icon(Icon::ArrowLeft)
                    .enabled(previous)
                    .render(),
                Button::new("message-search:next", t("Siguiente"), palette)
                    .size(ControlSize::Compact)
                    .trailing_icon(Icon::ArrowRight)
                    .enabled(next)
                    .render(),
                Element::spacer().flex(1.0),
                Button::new("message-search:refresh", t("Actualizar"), palette)
                    .size(ControlSize::Compact)
                    .variant(ButtonVariant::Ghost)
                    .icon(Icon::Refresh)
                    .enabled(refresh)
                    .render(),
            ])
            .gap(8.0),
        );
    }
    body.push(line(
        t("↑↓ elegir · Enter ir · Esc cerrar · Ctrl+K cambiar de conversación"),
        12.0,
        look.faint,
    ));
    if let Some(status) = crate::context::status(ui, look) {
        body.push(status);
    }
    Modal::new(
        root,
        true,
        t("Buscar mensajes"),
        description,
        "message-search:close",
        palette,
    )
    .width(Modal::fit_width(context.window.width, 640.0))
    .height(card)
    // All of it below the header, none of it scrolling but the results:
    // a modal's body scrolls, so the list could not fill it.
    .footer(Element::column(body).gap(10.0).flex(1.0))
    .initial_focus(QUERY)
    .render()
}

/// The previous and next pages, and asking again, where results come by page.
fn pager(ui: &Ui, search: &MessageSearch) -> Option<(bool, bool, bool)> {
    match search.scope {
        Scope::Loaded => None,
        Scope::Remote => {
            let (previous, next) = remote::pages(search);
            Some((previous, next, search.remote.is_some()))
        }
        Scope::Device => {
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
            Some((previous, next, ui.local_history_available))
        }
    }
}

/// How the provider orders the whole history.
fn order(search: &MessageSearch, look: &Look) -> Element {
    let relevance = search.order == rune_model::RemoteOrder::Relevance;
    Select::new(
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
    .icon(Icon::Sort)
    .size(ControlSize::Compact)
    .width(170.0)
    .render()
}

/// The results, a card each, built as they come into view.
fn results(
    ui: &Ui,
    store: &Store,
    search: &MessageSearch,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let data = search.hits.clone();
    let server = store.channel(search.channel).and_then(|found| found.server);
    // The few people and channels of many results, looked up once.
    let mut faces: HashMap<UserId, Face> = HashMap::new();
    let mut channels: HashMap<ChannelId, String> = HashMap::new();
    let mut pictures: HashMap<RemoteImage, limn::RasterImage> = HashMap::new();
    for hit in data.iter() {
        faces
            .entry(hit.data.author)
            .or_insert_with(|| face(ui, store, hit.data.author, server, look));
        channels.entry(hit.data.channel).or_insert_with(|| {
            store
                .channel(hit.data.channel)
                .map_or(t("canal no disponible").into(), |found| found.name.clone())
        });
        for picture in &hit.media.pictures {
            if let Some(image) = ui.picture(Some(picture)) {
                pictures.insert(picture.clone(), image);
            }
        }
    }
    // Several channels group by channel; one, by day, unless ranked.
    let by_channel = channels.len() > 1;
    let ranked =
        search.scope == Scope::Remote && search.order == rune_model::RemoteOrder::Relevance;
    let by_day = !(by_channel || ranked);
    let provider = store
        .provider
        .as_ref()
        .map_or(t("el proveedor"), |provider| t(&provider.name))
        .to_owned();
    let remote = search.scope == Scope::Remote;
    let selected = search.selected;
    let wall = context.wall;
    let terms = terms(search);
    let look = *look;
    let keys = data.clone();
    ScrollArea::measured_list(
        RESULTS,
        data.len(),
        96.0,
        move |index| keys.get(index).map_or(0, |hit| hit.data.id.0),
        move |index| {
            let hit = &data[index];
            let before = index.checked_sub(1).map(|index| &data[index]);
            let heading = if by_channel {
                (before.map(|before| before.data.channel) != Some(hit.data.channel))
                    .then(|| Heading::Channel(channels[&hit.data.channel].clone()))
            } else if by_day {
                let day = time::day(hit.data.timestamp);
                (before.map(|before| time::day(before.data.timestamp)) != Some(day))
                    .then(|| Heading::Day(time::day_label(hit.data.timestamp, wall)))
            } else {
                None
            };
            let face = &faces[&hit.data.author];
            let mut files = hit.media.files.clone();
            if files.is_empty() && hit.media.pictures.is_empty() && hit.data.files {
                files.push(t("Archivo adjunto").into());
            }
            cards::card(
                &ResultCard {
                    id: format!("message-search:go:{}:{}", hit.data.channel, hit.data.id),
                    name: face.name.clone(),
                    name_ink: face.ink,
                    accent: face.accent,
                    face: face.image.clone(),
                    when: time::stamp(hit.data.timestamp, wall),
                    source: match hit.source {
                        Source::Remote => Some(provider.clone()),
                        Source::Both => {
                            Some(tr!("{provider} y cargado", provider = provider.as_str()))
                        }
                        Source::Loaded if remote => Some(t("cargado").into()),
                        _ => None,
                    },
                    // limn-gap: a server's emoji reads `:name:` here
                    // (`SearchHit::from_message`), not its picture.
                    text: hit.data.snippet.clone(),
                    pinned: hit.data.pinned,
                    pictures: hit
                        .media
                        .pictures
                        .iter()
                        .map(|picture| pictures.get(picture).cloned())
                        .collect(),
                    files,
                    selected: index == selected,
                    heading,
                },
                &terms,
                &look,
            )
        },
    )
    .appearance(look.scrollbar(true))
    .render()
}

/// The pictures the results in view and the author menu show.
pub(crate) fn wanted(
    ui: &Ui,
    store: &Store,
    tree: &limn::UiTree,
    add: &mut dyn FnMut(Option<&RemoteImage>),
) {
    let Some(search) = &ui.search else {
        return;
    };
    let server = store.channel(search.channel).and_then(|found| found.server);
    if let Some(author) = search.author {
        add(store.avatar_in(server, author));
    }
    if search.menu == Some(Menu::Author) {
        for author in &search.author_options {
            add(store.avatar_in(server, *author));
        }
    }
    let Some(rows) = tree.visible_rows(RESULTS) else {
        return;
    };
    for hit in search.hits.get(rows).into_iter().flatten() {
        add(store.avatar_in(server, hit.data.author));
        for picture in &hit.media.pictures {
            add(Some(picture));
        }
    }
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
