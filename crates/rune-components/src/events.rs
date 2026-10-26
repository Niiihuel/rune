//! A server's scheduled events (eventos-programados): the entry over its
//! channels, and the dialog with the list, an event's detail, the form
//! that plans or edits one and the question before deleting it. Pure
//! views: what is chosen and written lives in `rune-ui`.

use chrono::{Local, NaiveDate, NaiveTime, TimeZone};
use limn::{
    Button, ButtonVariant, Checkbox, Component, ControlSize, DateTimePicker, Element, FontFamily,
    Icon, Modal, RadioGroup, RadioOption, TextField, Tooltip,
};
use rune_i18n::{t, tr};
use rune_model::{EventStatus, ScheduledEvent, ScheduledEventId, Timestamp};

use crate::{gap, icon, line, strong_line, text, Look};

/// A calendar: the events' entry and their dates.
pub const CALENDAR: &str = crate::glyphs::glyph!("calendar");

/// The actions of the entry and the dialog, by the domain's ids.
pub mod ids {
    use rune_model::{ChannelId, ScheduledEventId};

    pub const OPEN: &str = "ev:open";
    pub const CLOSE: &str = "ev:close";
    pub const BACK: &str = "ev:back";
    pub const NEW: &str = "ev:new";
    pub const SAVE: &str = "ev:save";
    pub const RETRY: &str = "ev:retry";
    pub const DELETE: &str = "ev:delete-yes";
    pub const NAME: &str = "ev:name";
    pub const DESCRIPTION: &str = "ev:description";
    pub const LOCATION: &str = "ev:location";
    pub const START_DATE: &str = "ev:start-date";
    pub const START_TIME: &str = "ev:start-time";
    pub const END_DATE: &str = "ev:end-date";
    pub const END_TIME: &str = "ev:end-time";
    pub const HAS_END: &str = "ev:has-end";
    /// Where it happens: somewhere else than a channel.
    pub const ELSEWHERE: &str = "ev:where:elsewhere";

    /// An event's detail.
    pub fn show(event: ScheduledEventId) -> String {
        format!("ev:show:{}", event.0)
    }

    /// «Me interesa», on or off.
    pub fn interest(event: ScheduledEventId) -> String {
        format!("ev:interest:{}", event.0)
    }

    pub fn edit(event: ScheduledEventId) -> String {
        format!("ev:edit:{}", event.0)
    }

    pub fn start(event: ScheduledEventId) -> String {
        format!("ev:start:{}", event.0)
    }

    pub fn finish(event: ScheduledEventId) -> String {
        format!("ev:finish:{}", event.0)
    }

    pub fn delete(event: ScheduledEventId) -> String {
        format!("ev:delete:{}", event.0)
    }

    /// Where it happens: the stage or voice channel `channel`.
    pub fn place(channel: ChannelId) -> String {
        format!("ev:where:{}", channel.0)
    }
}

/// The local date and time of `at`, as the pickers write them.
pub fn picker_values(at: Timestamp) -> (String, String) {
    let local = Local
        .timestamp_millis_opt(at.0)
        .single()
        .unwrap_or_else(Local::now);
    (
        local.format("%Y-%m-%d").to_string(),
        local.format("%H:%M").to_string(),
    )
}

/// The instant of a local `date` (`YYYY-MM-DD`) at `time` (`HH:MM`); `None`
/// for one not written whole, or that the clock skips.
pub fn picker_instant(date: &str, time: &str) -> Option<Timestamp> {
    if date.len() != 10 || time.len() != 5 {
        return None;
    }
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let time = NaiveTime::parse_from_str(time, "%H:%M").ok()?;
    Local
        .from_local_datetime(&date.and_time(time))
        .earliest()
        .map(|local| Timestamp(local.timestamp_millis()))
}

/// The entry over a server's channels: «Eventos», and how many are to
/// come or happening when known.
pub fn entry(count: usize, look: &Look) -> Element {
    let mut parts = vec![
        gap(8.0, 0.0),
        Element::icon(Icon::Custom(CALENDAR))
            .foreground(look.dim)
            .size(Some(20.0), Some(20.0))
            .centered_y(),
        gap(8.0, 0.0),
        strong_line(t("Eventos"), 15.0, look.dim)
            .flex(1.0)
            .centered_y(),
    ];
    let name = if count == 0 {
        t("Eventos").to_owned()
    } else {
        parts.push(line(count.to_string(), 13.0, look.muted).centered_y());
        tr!("Eventos ({count})", count)
    };
    parts.push(gap(8.0, 0.0));
    let body = Element::clickable(ids::OPEN, Element::row(parts))
        .size(None, Some(crate::sidebar::ROW))
        .radius(6.0)
        .hover_background(look.hover)
        .accessible(limn::Access::new(limn::Role::Button).name(name));
    Element::row(vec![
        gap(crate::sidebar::INSET, 0.0),
        body.flex(1.0),
        gap(crate::sidebar::INSET, 0.0),
    ])
}

/// An event as the list and the detail show it.
pub struct EventCard<'a> {
    pub event: &'a ScheduledEvent,
    /// Where it is: a channel's name, or the place written.
    pub place: String,
    /// Its cover, once it arrived.
    pub image: Option<limn::RasterImage>,
    /// Why the account cannot say it interests it, when it cannot.
    pub interest_denied: Option<String>,
}

/// When an event is, as a card says it: happening, over, or its start.
pub fn when(event: &ScheduledEvent) -> String {
    match event.status {
        EventStatus::Active => t("En curso").into(),
        EventStatus::Completed => t("Terminó").into(),
        EventStatus::Canceled => t("Cancelado").into(),
        EventStatus::Scheduled => crate::time::full_date(event.start),
    }
}

/// How many an event interests, as its card says it.
pub fn interested_label(count: u64) -> String {
    if count == 1 {
        t("Le interesa a una persona").into()
    } else {
        tr!("Les interesa a {count} personas", count)
    }
}

fn place_icon(event: &ScheduledEvent) -> Icon {
    match event.place {
        rune_model::EventPlace::Stage(_) => Icon::Microphone,
        rune_model::EventPlace::Voice(_) => Icon::Volume,
        rune_model::EventPlace::External(_) => Icon::Globe,
    }
}

/// «Me interesa», marked when it does.
fn interest_button(card: &EventCard, busy: bool, look: &Look) -> Element {
    let event = card.event;
    let id = ids::interest(event.id);
    let button = Button::new(
        &id,
        if event.account_interested {
            t("Te interesa")
        } else {
            t("Me interesa")
        },
        if event.account_interested {
            look.accent_palette()
        } else {
            look.menu_palette()
        },
    )
    .variant(if event.account_interested {
        ButtonVariant::Default
    } else {
        ButtonVariant::Secondary
    })
    .enabled(card.interest_denied.is_none() && !busy && !event.status.ended());
    match &card.interest_denied {
        Some(reason) => Tooltip::new(
            button.render(),
            format!("{id}:hint"),
            reason.clone(),
            look.menu_palette(),
        )
        .render(),
        None => button.render(),
    }
}

/// What a card says of an event: when, its name and where.
fn summary(event: &ScheduledEvent, place: &str, width: f32, look: &Look) -> Vec<Element> {
    let live = event.status == EventStatus::Active;
    let ink = if live {
        look.online
    } else if event.status.ended() {
        look.muted
    } else {
        look.accent
    };
    let mut head = vec![Element::icon(if live {
        Icon::Circle
    } else {
        Icon::Custom(CALENDAR)
    })
    .foreground(ink)
    .size(Some(14.0), Some(14.0))
    .centered_y()];
    head.push(line(when(event), 13.0, ink).flex(1.0).centered_y());
    vec![
        Element::row(head).gap(6.0),
        text(event.name.clone(), 16.0, look.text)
            .font_family(FontFamily::SansMedium)
            .size(Some(width), None),
        Element::row(vec![
            icon(place_icon(event), look.muted).centered_y(),
            line(place.to_owned(), 13.0, look.muted)
                .flex(1.0)
                .centered_y(),
        ])
        .gap(6.0),
    ]
}

/// One event of the list: its summary opens its detail; «Me interesa»
/// beside how many it interests.
fn list_card(card: &EventCard, busy: bool, width: f32, look: &Look) -> Element {
    let event = card.event;
    let inner = width - 24.0;
    let about = Element::clickable(
        ids::show(event.id),
        Element::column(summary(event, &card.place, inner, look)).gap(4.0),
    )
    .accessible(limn::Access::new(limn::Role::Button).name(event.name.clone()));
    let mut bottom = Vec::new();
    if let Some(count) = event.interested {
        bottom.push(
            line(interested_label(count), 12.0, look.muted)
                .flex(1.0)
                .centered_y(),
        );
    } else {
        bottom.push(Element::spacer().flex(1.0));
    }
    bottom.push(interest_button(card, busy, look).centered_y());
    Element::panel(
        Element::column(vec![about, Element::row(bottom).gap(8.0)])
            .gap(10.0)
            .padding(12.0),
    )
    .background(look.canvas)
    .border(look.border, 1.0)
    .radius(8.0)
    .size(Some(width), None)
}

/// What the form holds while an event is planned or edited.
pub struct FormView<'a> {
    /// `None` while planning a new one.
    pub editing: Option<ScheduledEventId>,
    /// The stage and voice channels it may happen in: id and name, and
    /// whether it is a stage.
    pub channels: &'a [(rune_model::ChannelId, String, bool)],
    /// The channel chosen, or `None` for somewhere else.
    pub channel: Option<rune_model::ChannelId>,
    pub location: &'a str,
    pub name: &'a str,
    pub description: &'a str,
    pub start_date: &'a str,
    pub start_time: &'a str,
    /// The month each calendar shows.
    pub start_month: (i32, u32),
    pub has_end: bool,
    pub end_date: &'a str,
    pub end_time: &'a str,
    pub end_month: (i32, u32),
    /// Whether its start may still change: not once it started.
    pub starts: bool,
}

/// What the dialog shows.
pub enum EventsPage<'a> {
    List {
        cards: Vec<EventCard<'a>>,
        /// Why the account cannot plan one, when it cannot.
        create_denied: Option<String>,
    },
    Detail {
        card: EventCard<'a>,
        /// Who planned it.
        creator: Option<String>,
        /// Who it interests, once read.
        interested: Option<Vec<String>>,
        /// Why the account cannot change it, when it cannot.
        change_denied: Option<String>,
    },
    Form(FormView<'a>),
    /// The question before deleting the event of this name.
    Delete(&'a str),
}

/// The dialog as it is now.
pub struct EventsView<'a> {
    /// The server's name.
    pub server: &'a str,
    pub page: EventsPage<'a>,
    /// Still asking the provider for what to show.
    pub loading: bool,
    /// A change is on its way.
    pub busy: bool,
    /// Why the last change or read did not happen.
    pub problem: Option<&'a str>,
    /// Why the form cannot be saved yet, once it was tried.
    pub invalid: Option<&'a str>,
    /// A line under the title: the simulation says it is one.
    pub note: Option<&'a str>,
}

fn spinner(label: &str, look: &Look) -> Element {
    Element::row(vec![
        Element::spinner("ev:loading", look.accent, limn::SpinnerVariant::Circle)
            .size(Some(16.0), Some(16.0))
            .centered_y(),
        line(label.to_owned(), 14.0, look.muted).centered_y(),
    ])
    .gap(8.0)
}

fn heading(label: &str, look: &Look) -> Element {
    strong_line(label.to_owned(), 13.0, look.muted)
}

/// A button of the footer.
fn action(id: &str, label: &str, palette: limn::ComponentPalette, enabled: bool) -> Element {
    Button::new(id, label, palette)
        .size(ControlSize::Spacious)
        .enabled(enabled)
        .render()
        .flex(1.0)
}

fn secondary(id: &str, label: &str, look: &Look, enabled: bool) -> Element {
    Button::new(id, label, look.menu_palette())
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Spacious)
        .enabled(enabled)
        .render()
        .flex(1.0)
}

/// With a hint saying why it is off, when it is.
fn explained(button: Element, id: &str, reason: Option<&String>, look: &Look) -> Element {
    match reason {
        Some(reason) => Tooltip::new(
            button,
            format!("{id}:hint"),
            reason.clone(),
            look.menu_palette(),
        )
        .render()
        .flex(1.0),
        None => button,
    }
}

/// The dialog over `root`, `width` wide.
pub fn events_dialog(root: Element, view: &EventsView<'_>, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let inner = Modal::body_width(width) - 2.0;
    let mut body = Vec::new();
    if let Some(note) = view.note {
        body.push(line(note, 12.0, look.muted));
    }
    let (title, footer, focus) = match &view.page {
        EventsPage::List {
            cards,
            create_denied,
        } => {
            if view.loading {
                body.push(spinner(t("Cargando eventos…"), look));
            } else if cards.is_empty() && view.problem.is_none() {
                body.push(
                    Element::column(vec![
                        strong_line(t("No hay eventos programados"), 15.0, look.text),
                        text(
                            t("Cuando alguien planee uno, va a aparecer aquí."),
                            13.0,
                            look.muted,
                        )
                        .size(Some(inner), None),
                    ])
                    .gap(4.0),
                );
            }
            for card in cards {
                body.push(list_card(card, view.busy, inner, look));
            }
            let mut buttons = vec![secondary(ids::CLOSE, t("Cerrar"), look, true)];
            if view.problem.is_some() && !view.loading {
                buttons.push(action(
                    ids::RETRY,
                    t("Reintentar"),
                    look.accent_palette(),
                    true,
                ));
            } else {
                buttons.push(explained(
                    action(
                        ids::NEW,
                        t("Crear evento"),
                        look.accent_palette(),
                        create_denied.is_none(),
                    ),
                    ids::NEW,
                    create_denied.as_ref(),
                    look,
                ));
            }
            (
                tr!("Eventos de {server}", server = view.server),
                buttons,
                ids::CLOSE.to_owned(),
            )
        }
        EventsPage::Detail {
            card,
            creator,
            interested,
            change_denied,
        } => {
            let event = card.event;
            if let Some(image) = &card.image {
                body.push(
                    Element::panel(
                        Element::raster_image(image.clone().cover())
                            .size(Some(inner), Some(inner * 0.4)),
                    )
                    .radius(8.0)
                    .clip_children(),
                );
            }
            body.push(Element::column(summary(event, &card.place, inner, look)).gap(4.0));
            if let Some(description) = &event.description {
                body.push(text(description.clone(), 14.0, look.text).size(Some(inner), None));
            }
            if let Some(creator) = creator {
                body.push(line(
                    tr!("Lo creó {person}", person = creator),
                    13.0,
                    look.muted,
                ));
            }
            body.push(
                Element::row(vec![
                    line(
                        event.interested.map(interested_label).unwrap_or_default(),
                        13.0,
                        look.muted,
                    )
                    .flex(1.0)
                    .centered_y(),
                    interest_button(card, view.busy, look).centered_y(),
                ])
                .gap(8.0),
            );
            body.push(heading(t("Interesados"), look));
            match interested {
                None => body.push(spinner(t("Cargando…"), look)),
                Some(people) if people.is_empty() => {
                    body.push(line(t("Todavía no le interesa a nadie."), 13.0, look.muted))
                }
                Some(people) => {
                    for person in people {
                        body.push(line(person.clone(), 14.0, look.text));
                    }
                }
            }
            let mut buttons = vec![secondary(ids::BACK, t("Volver"), look, !view.busy)];
            let may = change_denied.is_none() && !view.busy;
            if !event.status.ended() {
                buttons.push(explained(
                    secondary(&ids::edit(event.id), t("Editar"), look, may),
                    &ids::edit(event.id),
                    change_denied.as_ref(),
                    look,
                ));
                let (id, label) = match event.status {
                    EventStatus::Active => (ids::finish(event.id), t("Terminar")),
                    _ => (ids::start(event.id), t("Iniciar")),
                };
                buttons.push(secondary(&id, label, look, may));
            }
            buttons.push(
                Button::new(ids::delete(event.id), t("Borrar"), palette)
                    .variant(ButtonVariant::Destructive)
                    .size(ControlSize::Spacious)
                    .enabled(may)
                    .render()
                    .flex(1.0),
            );
            (event.name.clone(), buttons, ids::BACK.to_owned())
        }
        EventsPage::Form(form) => {
            body.extend(form_body(form, view.busy, inner, look));
            let label = match (form.editing, view.busy) {
                (_, true) => t("Guardando…"),
                (None, false) => t("Crear evento"),
                (Some(_), false) => t("Guardar"),
            };
            let save = Button::new(ids::SAVE, label, look.accent_palette())
                .size(ControlSize::Spacious)
                .loading(view.busy)
                .render()
                .flex(1.0);
            (
                if form.editing.is_some() {
                    t("Editar evento").to_owned()
                } else {
                    t("Crear evento").to_owned()
                },
                vec![secondary(ids::BACK, t("Cancelar"), look, !view.busy), save],
                ids::NAME.to_owned(),
            )
        }
        EventsPage::Delete(name) => {
            body.push(
                text(
                    t("Nadie va a poder verlo, y no se puede deshacer."),
                    14.0,
                    look.text,
                )
                .size(Some(inner), None),
            );
            let delete = Button::new(ids::DELETE, t("Borrar evento"), palette)
                .variant(ButtonVariant::Destructive)
                .size(ControlSize::Spacious)
                .loading(view.busy)
                .render()
                .flex(1.0);
            (
                tr!("¿Borrar «{name}»?", name),
                vec![
                    secondary(ids::BACK, t("Cancelar"), look, !view.busy),
                    delete,
                ],
                ids::BACK.to_owned(),
            )
        }
    };
    // Over the buttons, where it is seen however far the body scrolled.
    let mut bottom = Vec::new();
    if let Some(said) = view.invalid.or(view.problem) {
        bottom.push(
            Element::row(vec![
                icon(Icon::WarningTriangle, look.danger).centered_y(),
                text(said.to_owned(), 12.0, look.danger).flex(1.0),
            ])
            .gap(6.0),
        );
    }
    bottom.push(Element::row(footer).gap(9.0));
    Modal::new(root, true, title, "", ids::CLOSE, palette)
        .width(width)
        .body(Element::column(body).gap(12.0))
        .footer(Element::column(bottom).gap(8.0))
        .initial_focus(focus)
        .render()
}

/// The fields of the form: where, what and when.
fn form_body(form: &FormView, busy: bool, inner: f32, look: &Look) -> Vec<Element> {
    let palette = look.menu_palette();
    let mut body = vec![heading(t("¿Dónde es?"), look)];
    let mut options: Vec<RadioOption> = form
        .channels
        .iter()
        .map(|(id, name, stage)| {
            RadioOption::new(ids::place(*id), name.clone()).description(if *stage {
                t("Escenario")
            } else {
                t("Canal de voz")
            })
        })
        .collect();
    options.push(
        RadioOption::new(ids::ELSEWHERE, t("Otro lugar"))
            .description(t("Una dirección o un enlace")),
    );
    let chosen = match form.channel {
        Some(channel) => ids::place(channel),
        None => ids::ELSEWHERE.to_owned(),
    };
    body.push(
        RadioGroup::new("ev:where", options, Some(chosen.as_str()), palette)
            .accessibility_label(t("¿Dónde es?"))
            .enabled(!busy)
            .render(),
    );
    if form.channel.is_none() {
        body.push(
            TextField::new(
                ids::LOCATION,
                form.location,
                t("Dirección o enlace"),
                palette,
            )
            .width(inner)
            .enabled(!busy)
            .render(),
        );
    }
    body.push(heading(t("Nombre del evento"), look));
    body.push(
        TextField::new(ids::NAME, form.name, t("¿Qué van a hacer?"), palette)
            .width(inner)
            .enabled(!busy)
            .render(),
    );
    body.push(heading(t("Descripción"), look));
    body.push(
        Element::panel(
            Element::text_area(
                ids::DESCRIPTION,
                form.description,
                t("Cuenta de qué se trata (opcional)"),
            )
            .text_area_auto_grow(66.0, 180.0)
            .foreground(look.text)
            .font_family(FontFamily::Sans)
            .font_size(14.0)
            .padding(10.0),
        )
        .background(look.canvas)
        .border(look.border, 1.0)
        .radius(4.0)
        .size(Some(inner), None),
    );
    body.push(heading(t("Empieza"), look));
    body.push(
        DateTimePicker::new(
            ids::START_DATE,
            ids::START_TIME,
            form.start_date,
            form.start_time,
            palette,
        )
        .display_month(form.start_month.0, form.start_month.1)
        .enabled(!busy && form.starts)
        .render(),
    );
    // One held elsewhere always says when it ends.
    if form.channel.is_some() {
        body.push(
            Checkbox::new(ids::HAS_END, form.has_end, palette)
                .label(t("Con hora de fin"))
                .enabled(!busy)
                .render(),
        );
    }
    if form.has_end || form.channel.is_none() {
        body.push(heading(t("Termina"), look));
        body.push(
            DateTimePicker::new(
                ids::END_DATE,
                ids::END_TIME,
                form.end_date,
                form.end_time,
                palette,
            )
            .display_month(form.end_month.0, form.end_month.1)
            .enabled(!busy)
            .render(),
        );
    }
    body
}

/// An event an invitation names, as its card shows it over the server.
#[derive(Clone, Debug, PartialEq)]
pub struct InvitedEvent {
    pub name: String,
    /// When it is, as [`when`] says it.
    pub when: String,
    /// Where it is.
    pub place: String,
}

/// The event's part of an invitation's card: when, its name and where;
/// «Ver evento» reports `detail` while the account can see it.
pub fn invited_event(
    event: &InvitedEvent,
    detail: Option<String>,
    width: f32,
    scale: f32,
    look: &Look,
) -> Element {
    let mut rows = vec![
        Element::row(vec![
            Element::icon(Icon::Custom(CALENDAR))
                .foreground(look.accent)
                .size(Some(14.0), Some(14.0))
                .centered_y(),
            line(event.when.clone(), 13.0 * scale, look.accent)
                .flex(1.0)
                .centered_y(),
        ])
        .gap(6.0),
        text(event.name.clone(), 16.0 * scale, look.text)
            .font_family(FontFamily::SansMedium)
            .size(Some(width.max(40.0)), None),
        line(event.place.clone(), 13.0 * scale, look.muted),
    ];
    if let Some(id) = detail {
        rows.push(
            Button::new(id, t("Ver evento"), look.menu_palette())
                .variant(ButtonVariant::Secondary)
                .render(),
        );
    }
    Element::panel(Element::column(rows).gap(4.0).padding(12.0))
        .background(look.canvas)
        .radius(6.0)
        .size(Some(width + 24.0), None)
        .accessible(limn::Access::new(limn::Role::Group).name(event.name.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;
    use rune_model::{ChannelId, EventPlace, ServerId};

    const LOOK: Look = Look::DARK;

    fn event(status: EventStatus, mine: bool) -> ScheduledEvent {
        ScheduledEvent {
            id: ScheduledEventId(7),
            server: ServerId(1),
            name: "Noche de juegos".into(),
            description: Some("Trae tu joystick".into()),
            start: Timestamp(1_893_607_200_000),
            end: None,
            place: EventPlace::Voice(ChannelId(4)),
            status,
            image: None,
            creator: None,
            interested: Some(3),
            account_interested: mine,
        }
    }

    fn dialog(page: EventsPage) -> limn::Frame {
        let view = EventsView {
            server: "Taller",
            page,
            loading: false,
            busy: false,
            problem: None,
            invalid: None,
            note: None,
        };
        frame(
            events_dialog(Element::spacer(), &view, 520.0, &LOOK),
            Size::new(1000.0, 900.0),
        )
        .1
    }

    #[test]
    fn the_list_offers_each_event_and_planning_only_when_allowed() {
        let one = event(EventStatus::Scheduled, false);
        let card = || EventCard {
            event: &one,
            place: "Sala".into(),
            image: None,
            interest_denied: None,
        };
        let laid = dialog(EventsPage::List {
            cards: vec![card()],
            create_denied: None,
        });
        let found = controls(&laid);
        for id in [ids::show(one.id), ids::interest(one.id), ids::NEW.into()] {
            assert!(found.contains(&id), "{id} in {found:?}");
        }
        let painted = texts(&laid).join(" ");
        assert!(painted.contains("Noche de juegos") && painted.contains("Me interesa"));
        assert!(painted.contains("Les interesa a 3 personas"));
        let denied = dialog(EventsPage::List {
            cards: Vec::new(),
            create_denied: Some("No".into()),
        });
        assert!(!controls(&denied).contains(&ids::NEW.to_owned()));
        assert!(texts(&denied)
            .join(" ")
            .contains("No hay eventos programados"));
    }

    #[test]
    fn the_detail_offers_changes_only_to_who_may_and_none_once_over() {
        let live = event(EventStatus::Active, true);
        let detail = |event, denied: Option<String>| {
            dialog(EventsPage::Detail {
                card: EventCard {
                    event,
                    place: "Sala".into(),
                    image: None,
                    interest_denied: None,
                },
                creator: Some("Ada".into()),
                interested: Some(vec!["Ada".into(), "Bruno".into()]),
                change_denied: denied,
            })
        };
        let laid = detail(&live, None);
        let found = controls(&laid);
        assert!(found.contains(&ids::finish(live.id)) && !found.contains(&ids::start(live.id)));
        assert!(found.contains(&ids::delete(live.id)));
        let painted = texts(&laid).join(" ");
        assert!(painted.contains("En curso") && painted.contains("Te interesa"));
        assert!(painted.contains("Bruno") && painted.contains("Lo creó Ada"));
        let denied = detail(&live, Some("No".into()));
        assert!(!controls(&denied).contains(&ids::delete(live.id)));
        let over = event(EventStatus::Completed, false);
        let laid = detail(&over, None);
        assert!(!controls(&laid).contains(&ids::edit(over.id)));
        assert!(!controls(&laid).contains(&ids::interest(over.id)));
    }

    #[test]
    fn the_form_asks_for_a_place_when_elsewhere_and_always_an_end_there() {
        let channels = [(ChannelId(4), "Sala".to_owned(), false)];
        let form = |channel| FormView {
            editing: None,
            channels: &channels,
            channel,
            location: "",
            name: "",
            description: "",
            start_date: "2030-01-02",
            start_time: "18:00",
            start_month: (2030, 1),
            has_end: false,
            end_date: "",
            end_time: "",
            end_month: (2030, 1),
            starts: true,
        };
        // The fields alone, all in view: the dialog's body scrolls.
        let fields = |form: FormView| {
            let laid = frame(
                Element::column(form_body(&form, false, 480.0, &LOOK)),
                Size::new(520.0, 2000.0),
            )
            .1;
            controls(&laid)
        };
        let found = fields(form(Some(ChannelId(4))));
        assert!(found.contains(&ids::START_DATE.to_owned()));
        assert!(!found.contains(&ids::LOCATION.to_owned()));
        assert!(!found.contains(&ids::END_DATE.to_owned()));
        let laid = dialog(EventsPage::Form(form(None)));
        assert!(controls(&laid).contains(&ids::NAME.to_owned()));
        let found = fields(form(None));
        assert!(found.contains(&ids::LOCATION.to_owned()));
        assert!(found.contains(&ids::END_DATE.to_owned()));
    }

    #[test]
    fn picker_values_go_and_come_back_whole() {
        let at = picker_instant("2030-01-02", "18:30").unwrap();
        assert_eq!(
            picker_values(at),
            ("2030-01-02".to_owned(), "18:30".to_owned())
        );
        assert_eq!(picker_instant("2030-1-2", "18:30"), None);
        assert_eq!(picker_instant("2030-01-02", "25:00"), None);
    }

    #[test]
    fn the_entry_says_how_many_are_coming() {
        let laid = frame(entry(2, &LOOK), Size::new(294.0, 40.0)).1;
        assert!(controls(&laid).contains(&ids::OPEN.to_owned()));
        assert_eq!(texts(&laid), ["Eventos", "2"]);
        let shown = frame(
            invited_event(
                &InvitedEvent {
                    name: "Salida".into(),
                    when: "Mañana".into(),
                    place: "Costanera".into(),
                },
                Some("msg:1:invitation:0:event".into()),
                300.0,
                1.0,
                &LOOK,
            ),
            Size::new(400.0, 200.0),
        )
        .1;
        assert!(controls(&shown).contains(&"msg:1:invitation:0:event".to_owned()));
        assert!(texts(&shown).join(" ").contains("Ver evento"));
    }
}
