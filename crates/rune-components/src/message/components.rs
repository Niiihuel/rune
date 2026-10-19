//! What an app laid out with a message: texts, pictures, boxes, rows of
//! buttons and menus. A button that opens a page opens it; with
//! [`AppParts`], a button or a menu its app named tells the app it was used
//! (P3), shows that it waits for the app and why it failed; without them,
//! they show why they do nothing.

use std::cell::Cell;

use limn::{
    Access, Button, ButtonVariant, Color, Component as _, DropdownOption, Element, FontFamily,
    Icon, PopoverPlacement, Role, SearchField, Select, SelectVariant,
};
use rune_i18n::{es, t, tr};
use rune_model::components::{Button as Part, Choice, Media, MenuKind, Picked, Select as Menu};
use rune_model::{ButtonStyle, Component, TextReference};

use super::embeds::{markdown, opens, picture, wrapped, Draw, PARTS_TEXT, PICTURE, THUMBNAIL};
use crate::{gap, icon, line};

/// Why buttons and menus of an app do nothing here: a copy, a sample, or
/// a message without an app that answers them.
pub const NO_INTERACTIONS: &str = es("Estos controles no se pueden usar desde aquí");

/// What the account may do with the parts of one message's app (P3).
#[derive(Clone, Copy, Debug, Default)]
pub struct AppParts<'a> {
    /// Why its buttons and menus cannot be used, when they cannot.
    pub denied: Option<&'a str>,
    /// An interaction is in flight, here or elsewhere: nothing else goes.
    pub busy: bool,
    /// The part of this message waiting for its app, by its `custom_id`,
    /// and whether the app got it.
    pub waiting: Option<(&'a str, bool)>,
    /// Why the last use of one of this message's parts failed: the part's
    /// `custom_id` and the reason.
    pub problem: Option<(&'a str, &'a str)>,
    /// The menu of this message open.
    pub menu: Option<&'a OpenMenu>,
}

/// A menu of an app's message, open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenMenu {
    pub custom_id: String,
    /// What is picked so far, as its app is told.
    pub picked: Vec<String>,
    /// What a menu of people, roles or channels searches.
    pub query: String,
    /// What a menu of people, roles or channels offers now: value, label.
    pub options: Vec<(String, String)>,
}

/// Action IDs of an app's parts, from their message and `custom_id`.
pub mod app_ids {
    use rune_model::MessageId;

    /// A button pressed, or a menu opened or closed.
    pub fn part(message: MessageId, custom_id: &str) -> String {
        format!("msg:{message}:app:{custom_id}")
    }

    /// The `index`th option of an open menu, picked.
    pub fn pick(message: MessageId, index: usize, custom_id: &str) -> String {
        format!("msg:{message}:app-pick:{index}:{custom_id}")
    }

    /// An open menu closed.
    pub fn close(message: MessageId, custom_id: &str) -> String {
        format!("msg:{message}:app-close:{custom_id}")
    }

    /// What an open menu of people, roles or channels searches.
    pub fn query(message: MessageId, custom_id: &str) -> String {
        format!("msg:{message}:app-query:{custom_id}")
    }

    /// A reply only the account sees, taken away.
    pub fn dismiss(message: MessageId) -> String {
        format!("msg:{message}:dismiss")
    }
}

/// The label of what `menu` has picked, `picked` by value: a text menu's
/// options, or people, roles and channels by name; else its placeholder.
pub fn menu_label(
    menu: &Menu,
    picked: &[String],
    open: Option<&OpenMenu>,
    names: Option<&crate::text::Names>,
) -> String {
    let name = |value: &String| -> String {
        if let Some(option) = menu.options.iter().find(|option| option.value == *value) {
            return option.label.clone();
        }
        if let Some((_, label)) = open
            .into_iter()
            .flat_map(|open| &open.options)
            .find(|(found, _)| found == value)
        {
            return label.clone();
        }
        let picked = menu
            .defaults
            .iter()
            .find(|picked| picked.value() == *value)
            .copied();
        picked_name(picked, value, names)
    };
    if picked.is_empty() {
        return menu
            .placeholder
            .clone()
            .unwrap_or_else(|| t("Elegí una opción").into());
    }
    picked.iter().map(name).collect::<Vec<_>>().join(", ")
}

/// How someone, a role or a channel picked reads, by what `names` knows.
fn picked_name(picked: Option<Picked>, value: &str, names: Option<&crate::text::Names>) -> String {
    let id = value.parse::<u64>().unwrap_or(0);
    match (picked, names) {
        (Some(Picked::Role(role)), Some(names)) => names
            .role_ids
            .get(&role)
            .map_or_else(|| t("@rol").into(), |(name, _)| format!("@{name}")),
        (Some(Picked::Channel(_)), Some(names)) => names
            .channels
            .iter()
            .find(|(_, channel)| *channel == id)
            .map_or_else(|| t("#canal").into(), |(name, _)| format!("#{name}")),
        (_, Some(names)) => {
            let user = rune_model::UserId(id);
            names
                .display_names
                .get(&user)
                .or_else(|| names.people.get(&user))
                .map_or_else(|| format!("@{value}"), |name| format!("@{name}"))
        }
        (_, None) => value.to_owned(),
    }
}
/// Where the numbers of an app's parts' spoilers start among a message's
/// revealed spoilers, after those of its text.
pub const PARTS_SPOILER: usize = 1_000_000;
/// The widest a box of parts gets, as a card.
const BOX_WIDTH: f32 = 520.0;

/// Draws a message's file part by its name and width, when the message has
/// that file.
pub(super) type FileCard<'a> = &'a dyn Fn(&str, f32) -> Option<Element>;

/// Where the spoiler numbers of one list of a message's parts start, so each
/// of its spoilers has its own: the message's own parts, or its `card`th
/// card's, of the message or of the copy it forwards. A spoiler's control is
/// `msg:{id}:component:{n}:reveal` and, revealed, the message's revealed
/// spoilers hold [`PARTS_SPOILER`] + `n`.
pub fn first_spoiler(forwarded: bool, card: Option<usize>) -> usize {
    // At most ten cards, and far fewer than a thousand spoilers a list.
    (usize::from(forwarded) * 11 + card.map_or(0, |card| card + 1)) * 1_000
}

/// The parts, one under another, `width` wide; their spoilers numbered from
/// `first`.
pub(super) fn parts(
    parts: &[Component],
    references: &[TextReference],
    first: usize,
    width: f32,
    draw: &Draw<'_>,
    file: FileCard<'_>,
) -> Element {
    let at = Parts {
        references,
        draw,
        file,
        next: Cell::new(0),
        spoiler: Cell::new(first),
        text: Cell::new(PARTS_SPOILER + first + PARTS_TEXT),
    };
    let mut column: Vec<Element> = parts.iter().map(|part| at.one(part, width)).collect();
    let small = 12.0 * draw.scale;
    match draw.app {
        _ if draw.passive => {}
        None if needs_reason(parts) => {
            column.push(line(t(NO_INTERACTIONS), small, draw.look.faint))
        }
        None => {}
        Some(app) => {
            if let Some((_, received)) = app.waiting {
                let said = if received {
                    t("La aplicación está pensando…")
                } else {
                    t("Enviando a la aplicación…")
                };
                column.push(
                    Element::row(vec![
                        Element::spinner(
                            format!("{}:app:waiting", draw.base),
                            draw.look.muted,
                            limn::SpinnerVariant::Circle,
                        )
                        .size(Some(14.0), Some(14.0))
                        .centered_y(),
                        line(said, small, draw.look.muted).centered_y(),
                    ])
                    .gap(6.0),
                );
            } else if let Some((_, reason)) = app.problem {
                column.push(
                    Element::row(vec![
                        icon(Icon::WarningTriangle, draw.look.danger).centered_y(),
                        wrapped(
                            reason,
                            small,
                            FontFamily::Sans,
                            draw.look.danger,
                            width - 24.0,
                        )
                        .flex(1.0),
                    ])
                    .gap(6.0),
                );
            } else if let Some(reason) = app.denied.filter(|_| needs_reason(parts)) {
                column.push(line(reason, small, draw.look.faint));
            }
        }
    }
    Element::column(column).gap(8.0).size(Some(width), None)
}

/// Whether a button or a menu among `parts` cannot be used from here.
fn needs_reason(parts: &[Component]) -> bool {
    parts.iter().any(|part| match part {
        Component::Button(button) => button.url.is_none(),
        Component::Select(_) => true,
        Component::Row(parts) | Component::Container { parts, .. } => needs_reason(parts),
        Component::Section { parts, accessory } => {
            needs_reason(parts) || needs_reason(std::slice::from_ref(accessory))
        }
        _ => false,
    })
}

/// What every part of one list is drawn with.
struct Parts<'a> {
    references: &'a [TextReference],
    draw: &'a Draw<'a>,
    file: FileCard<'a>,
    /// The number of the next control, for its ID.
    next: Cell<usize>,
    /// The number of the next spoiler, as `components::pictures` counts them.
    spoiler: Cell<usize>,
    /// The revealed number of its texts' next spoiler.
    text: Cell<usize>,
}

impl Parts<'_> {
    fn one(&self, part: &Component, width: f32) -> Element {
        let draw = self.draw;
        let look = draw.look;
        let scale = draw.scale;
        match part {
            Component::Row(parts) => {
                Element::row(parts.iter().map(|part| self.one(part, width)).collect())
                    .gap(8.0)
                    .wrap()
                    .size(Some(width), None)
            }
            Component::Button(button) => self.button(button),
            Component::Select(menu) if menu.custom_id.is_some() && draw.app.is_some() => {
                self.menu(menu, width)
            }
            Component::Select(Menu {
                placeholder,
                chosen,
                ..
            }) => {
                let said = match chosen.as_slice() {
                    [] => placeholder
                        .clone()
                        .unwrap_or_else(|| t("Elegí una opción").into()),
                    chosen => chosen
                        .iter()
                        .map(|choice| choice.label.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                };
                // What its app left picked, as the reference client shows it;
                // several, each in a chip.
                let shown = match chosen.as_slice() {
                    [] => line(&said, 14.0 * scale, look.faint),
                    [only] => self.choice(only),
                    chosen => Element::row(
                        chosen
                            .iter()
                            .map(|choice| {
                                self.choice(choice)
                                    .padding(4.0)
                                    .background(look.raised)
                                    .radius(4.0)
                            })
                            .collect(),
                    )
                    .gap(4.0)
                    .wrap(),
                };
                Element::row(vec![
                    shown.flex(1.0).centered_y(),
                    icon(Icon::ChevronDown, look.faint).centered_y(),
                ])
                .padding(10.0)
                .background(look.canvas)
                .border(look.border, 1.0)
                .radius(4.0)
                .size(Some(width.min(400.0)), None)
                .accessible(
                    Access::new(Role::ComboBox)
                        .id(self.id())
                        .name(said)
                        .description(t(NO_INTERACTIONS))
                        .disabled(true),
                )
            }
            Component::Text(source) => {
                markdown(source, self.references, width, scale, &self.text, draw)
            }
            Component::Section { parts, accessory } => {
                let side = match &**accessory {
                    Component::Thumbnail(_) => THUMBNAIL,
                    _ => 0.0,
                };
                let text_width = (width - side - 16.0).max(40.0);
                Element::row(vec![
                    Element::column(
                        parts
                            .iter()
                            .map(|part| self.one(part, text_width))
                            .collect(),
                    )
                    .gap(8.0)
                    .flex(1.0),
                    self.one(accessory, THUMBNAIL),
                ])
                .gap(16.0)
                .size(Some(width), None)
            }
            Component::Thumbnail(item) => self.media(item, THUMBNAIL, (THUMBNAIL, THUMBNAIL)),
            Component::Gallery(items) => self.gallery(items, width),
            Component::File { name, spoiler } => {
                if let Some(n) = self.hidden(*spoiler) {
                    return self.veil(n, width.min(400.0), 56.0);
                }
                (self.file)(name, width).unwrap_or_else(|| {
                    Element::row(vec![
                        icon(Icon::File, look.muted).centered_y(),
                        line(name, 14.0 * scale, look.text).flex(1.0).centered_y(),
                    ])
                    .gap(8.0)
                    .padding(10.0)
                    .background(look.card)
                    .border(look.border, 1.0)
                    .radius(8.0)
                    .size(Some(width.min(400.0)), None)
                })
            }
            Component::Separator { divider, large } => {
                let room = if *large { 16.0 } else { 6.0 };
                let rule = if *divider {
                    Element::panel(Element::spacer())
                        .size(Some(width), Some(1.0))
                        .background(look.border)
                } else {
                    Element::spacer()
                };
                Element::column(vec![gap(0.0, room), rule, gap(0.0, room)])
            }
            Component::Container {
                accent,
                spoiler,
                parts,
            } => {
                let outer = width.min(BOX_WIDTH);
                if let Some(n) = self.hidden(*spoiler) {
                    // What it hides keeps its numbers for when it shows.
                    self.spoiler
                        .set(self.spoiler.get() + rune_model::components::spoilers(parts));
                    return self.veil(n, outer, 96.0);
                }
                let inner = (outer - 4.0 - 32.0).max(40.0);
                let stripe = accent.map_or(look.border, |rgb| {
                    Color::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
                });
                Element::row(vec![
                    Element::panel(Element::spacer())
                        .size(Some(4.0), None)
                        .background(stripe),
                    Element::panel(
                        Element::column(parts.iter().map(|part| self.one(part, inner)).collect())
                            .gap(8.0),
                    )
                    .padding(16.0)
                    .flex(1.0),
                ])
                .background(look.card)
                .border(look.border, 1.0)
                .radius(8.0)
                .clip_children()
                .size(Some(outer), None)
                .accessible(Access::new(Role::Group).name(t("Contenido de una aplicación")))
            }
            Component::Unknown(kind) => wrapped(
                &tr!(
                    "Una parte de la aplicación que RUNE todavía no muestra (tipo {kind})",
                    kind
                ),
                12.0 * scale,
                FontFamily::Sans,
                look.faint,
                width,
            ),
        }
    }

    /// A picked option of a menu: its emoji, then its label.
    fn choice(&self, choice: &Choice) -> Element {
        let draw = self.draw;
        let mut row = Vec::new();
        let picture = choice
            .picture
            .as_ref()
            .and_then(|picture| draw.images?.get(picture));
        match (picture, &choice.emoji) {
            (Some(picture), _) => row.push(
                match self.playing(picture) {
                    Some(surface) => Element::surface(surface, picture.alt())
                        .surface_fit(limn::SurfaceFit::Contain),
                    None => Element::raster_image(picture.clone()),
                }
                .size(Some(18.0 * draw.scale), Some(18.0 * draw.scale))
                .centered_y(),
            ),
            (None, Some(emoji)) => {
                row.push(line(emoji, 14.0 * draw.scale, draw.look.text).centered_y())
            }
            (None, None) => {}
        }
        row.push(line(&choice.label, 14.0 * draw.scale, draw.look.text).centered_y());
        Element::row(row).gap(6.0)
    }

    /// The surface `picture` plays on, while the host plays it: an emoji
    /// that moves, unless motion is reduced (limn-teclado-y-superficies).
    fn playing(&self, picture: &limn::RasterImage) -> Option<String> {
        self.draw
            .animated
            .is_some_and(|on| on.contains(picture.sha256()))
            .then(|| super::animation_surface(picture.sha256()))
    }

    /// `button` with its emoji's picture before the label, moving if the
    /// host plays it.
    fn pictured(&self, button: Button, picture: &limn::RasterImage) -> Button {
        match self.playing(picture) {
            Some(surface) => button.surface(surface),
            None => button.image(picture.clone()),
        }
    }

    /// The number of the next spoiler, when `spoiler` and it is not revealed.
    fn hidden(&self, spoiler: bool) -> Option<usize> {
        if !spoiler {
            return None;
        }
        let n = self.spoiler.get();
        self.spoiler.set(n + 1);
        (!self.draw.revealed.contains(&(PARTS_SPOILER + n))).then_some(n)
    }

    /// The ID of the next control: `msg:{id}:component:{n}`.
    fn id(&self) -> String {
        let n = self.next.get();
        self.next.set(n + 1);
        format!("{}:component:{n}", self.draw.base)
    }

    /// Whether a part its app named as `custom_id` may be used now, and
    /// why not; `None` when it only shows why parts do nothing here.
    fn usable(&self, custom_id: Option<&str>, disabled: bool) -> Option<Result<(), String>> {
        let app = self.draw.app?;
        custom_id?;
        Some(if disabled {
            Err(t("La aplicación desactivó este control").into())
        } else if let Some(reason) = app.denied {
            Err(reason.to_owned())
        } else if app.busy || self.draw.passive {
            Err(t("Esperá la respuesta de la aplicación").into())
        } else {
            Ok(())
        })
    }

    /// The `n` of `msg:{id}`, for an app's IDs.
    fn message(&self) -> rune_model::MessageId {
        rune_model::MessageId(
            self.draw
                .base
                .strip_prefix("msg:")
                .and_then(|id| id.parse().ok())
                .unwrap_or(0),
        )
    }

    /// A menu its app named: what is picked, and opened, what it offers.
    /// One choice goes as it is picked; several, as the menu closes.
    fn menu(&self, menu: &Menu, width: f32) -> Element {
        let draw = self.draw;
        let look = draw.look;
        let custom_id = menu.custom_id.as_deref().unwrap_or_default();
        let message = self.message();
        let _ = self.id();
        let app = draw.app.unwrap_or_default();
        let usable = self.usable(Some(custom_id), menu.disabled);
        let open = app
            .menu
            .filter(|open| open.custom_id == custom_id && matches!(usable, Some(Ok(()))));
        let picked = open.map_or_else(|| menu.starting(), |open| open.picked.clone());
        let label = menu_label(menu, &picked, open, draw.names);
        let waiting = app.waiting.is_some_and(|(waiting, _)| waiting == custom_id);
        let options: Vec<DropdownOption> = match menu.kind {
            MenuKind::Text => menu
                .options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    // A server emoji's picture stands for its `:name:` once
                    // loaded, moving if it does (limn-teclado-y-superficies).
                    let picture = option
                        .picture
                        .as_ref()
                        .and_then(|picture| draw.images?.get(picture));
                    let shown = match (&option.emoji, picture) {
                        (Some(emoji), None) => format!("{emoji} {}", option.label),
                        _ => option.label.clone(),
                    };
                    let shown =
                        DropdownOption::new(app_ids::pick(message, index, custom_id), shown)
                            .selected(picked.contains(&option.value));
                    match picture {
                        Some(picture) => match self.playing(picture) {
                            Some(surface) => shown.surface(surface),
                            None => shown.image(picture.clone()),
                        },
                        None => shown,
                    }
                })
                .collect(),
            _ => open
                .map(|open| {
                    open.options
                        .iter()
                        .enumerate()
                        .map(|(index, (value, shown))| {
                            DropdownOption::new(
                                app_ids::pick(message, index, custom_id),
                                shown.clone(),
                            )
                            .icon(match menu.kind {
                                MenuKind::Role => Icon::Users,
                                MenuKind::Channel => Icon::Hash,
                                _ => Icon::At,
                            })
                            .selected(picked.contains(value))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        let mut select = Select::new(
            app_ids::part(message, custom_id),
            label.clone(),
            options,
            open.is_some(),
            app_ids::close(message, custom_id),
            look.palette(),
        )
        .variant(SelectVariant::Field)
        .menu_palette(look.menu_palette())
        .placement(PopoverPlacement::BottomStart)
        .width(width.min(400.0))
        .menu_width(width.min(400.0))
        .enabled(matches!(usable, Some(Ok(()))) && !waiting)
        .accessibility_label(
            menu.placeholder
                .clone()
                .unwrap_or_else(|| t("Menú de la aplicación").into()),
        );
        if let Some(open) = open.filter(|_| menu.kind != MenuKind::Text) {
            let mut header = vec![SearchField::new(
                app_ids::query(message, custom_id),
                open.query.clone(),
                t("Buscar"),
                look.menu_palette(),
            )
            .width(width.min(400.0) - 16.0)
            .accessibility_label(t("Buscar en el menú"))
            .render()];
            if open.options.is_empty() {
                header.push(line(t("Nada que coincida"), 12.0, look.faint));
            }
            if menu.max > 1 {
                header.push(line(
                    tr!("Elegí hasta {max}; se envía al cerrar", max = menu.max),
                    12.0,
                    look.faint,
                ));
            }
            select = select.header(Element::column(header).gap(4.0).padding(8.0));
        } else if open.is_some() && menu.max > 1 {
            select = select.header(
                line(
                    tr!("Elegí hasta {max}; se envía al cerrar", max = menu.max),
                    12.0,
                    look.faint,
                )
                .padding(8.0),
            );
        }
        let element = select.render();
        match usable {
            Some(Err(reason)) => element.accessible(
                Access::new(Role::ComboBox)
                    .id(app_ids::part(message, custom_id))
                    .name(label)
                    .description(reason)
                    .disabled(true),
            ),
            _ => element,
        }
    }

    /// A button as its app styled it. One that opens a page goes through the
    /// timeline's link handler; one its app named tells the app, while the
    /// account may use apps; any other is shown disabled.
    fn button(&self, part: &Part) -> Element {
        let draw = self.draw;
        let look = draw.look;
        let label = match (&part.emoji, &part.label) {
            (Some(emoji), Some(label)) => format!("{emoji} {label}"),
            (Some(emoji), None) => emoji.clone(),
            (None, label) => label.clone().unwrap_or_default(),
        };
        let (palette, variant) = match part.style {
            ButtonStyle::Primary => (look.accent_palette(), ButtonVariant::Default),
            ButtonStyle::Success => (look.palette(), ButtonVariant::Success),
            ButtonStyle::Danger => (look.palette(), ButtonVariant::Destructive),
            _ => (look.palette(), ButtonVariant::Outline),
        };
        let own = self.id();
        let interactive = self
            .usable(part.custom_id.as_deref(), part.disabled)
            .filter(|_| part.url.is_none());
        if let Some(usable) = interactive {
            let custom_id = part.custom_id.as_deref().unwrap_or_default();
            let id = app_ids::part(self.message(), custom_id);
            let waiting = draw
                .app
                .and_then(|app| app.waiting)
                .is_some_and(|(waiting, _)| waiting == custom_id);
            let picture = part
                .picture
                .as_ref()
                .and_then(|picture| draw.images?.get(picture));
            let shown = match picture {
                Some(_) => part.label.clone().unwrap_or_default(),
                None => label.clone(),
            };
            let mut button = Button::new(id.clone(), shown, palette)
                .variant(variant)
                .enabled(usable.is_ok())
                .loading(waiting);
            if let Some(picture) = picture {
                button = self.pictured(button, picture);
            }
            if let Err(reason) = &usable {
                button = button.tooltip(reason.clone());
            }
            let mut access = Access::new(Role::Button)
                .id(id)
                .name(label)
                .disabled(usable.is_err());
            if let Err(reason) = usable {
                access = access.description(reason);
            }
            return button.render().accessible(access);
        }
        let usable = part.url.is_some() && !part.disabled && !draw.passive;
        let mut access = Access::new(if part.url.is_some() {
            Role::Link
        } else {
            Role::Button
        })
        .name(label.clone())
        .disabled(!usable);
        if part.url.is_none() {
            access = access.description(t(NO_INTERACTIONS));
        }
        // A server emoji's picture, once loaded, stands in for its `:name:`.
        let picture = part
            .picture
            .as_ref()
            .and_then(|picture| draw.images?.get(picture));
        let shown = match picture {
            Some(_) => part.label.clone().unwrap_or_default(),
            None => label.clone(),
        };
        let mut button = Button::new(part.url.as_deref().map_or(own, opens), shown, palette)
            .variant(variant)
            .enabled(usable);
        if let Some(picture) = picture {
            button = self.pictured(button, picture);
        }
        if part.url.is_some() {
            button = button.trailing_icon(Icon::ExternalLink);
        }
        button.render().accessible(access)
    }

    /// A picture of a part in the box `bounds`, or its description when it
    /// is not a picture RUNE loads; a spoiler waits until revealed, and so
    /// does an explicit one the account's filter hides, blurred unless it
    /// is a spoiler too (contenido-de-mensajes).
    fn media(&self, item: &Media, width: f32, bounds: (f32, f32)) -> Element {
        let draw = self.draw;
        if let Some(n) = self.hidden(item.covered()) {
            let (w, h) = (width.min(bounds.0), bounds.1.min(width));
            if item.explicit && draw.obscure_explicit {
                let action = format!("{}:component:{n}:reveal", draw.base);
                let behind = item.image.as_ref().filter(|_| !item.spoiler);
                return super::embeds::explicit((w.max(40.0), h.max(32.0)), &action, behind, draw);
            }
            if item.spoiler {
                return self.veil(n, w, h);
            }
        }
        match (&item.image, draw.pictures) {
            (Some(image), true) => picture(image, width, bounds, None, None, draw),
            _ => {
                let said = item
                    .description
                    .as_deref()
                    .unwrap_or(t("Contenido multimedia que RUNE no muestra"));
                Element::panel(wrapped(
                    said,
                    12.0 * draw.scale,
                    FontFamily::Sans,
                    draw.look.muted,
                    (width.min(bounds.0) - 16.0).max(24.0),
                ))
                .padding(8.0)
                .background(draw.look.card)
                .radius(4.0)
                .size(Some(width.min(bounds.0)), None)
            }
        }
    }

    /// One picture large; more, as tiles of two or three a row.
    fn gallery(&self, items: &[Media], width: f32) -> Element {
        if let [only] = items {
            return self.media(only, width, PICTURE);
        }
        let width = width.min(PICTURE.0 + 120.0);
        let per_row = if matches!(items.len(), 2 | 4) { 2 } else { 3 };
        let tile = ((width - 4.0 * (per_row - 1) as f32) / per_row as f32).floor();
        Element::column(
            items
                .chunks(per_row)
                .map(|row| {
                    Element::row(
                        row.iter()
                            .map(|item| self.media(item, tile, (tile, tile)))
                            .collect(),
                    )
                    .gap(4.0)
                })
                .collect(),
        )
        .gap(4.0)
    }

    /// What the spoiler numbered `n` covers until it is revealed.
    fn veil(&self, n: usize, width: f32, height: f32) -> Element {
        let draw = self.draw;
        let look = draw.look;
        let (width, height) = (width.max(40.0), height.max(32.0));
        let face = Element::panel(crate::centered(line(
            "SPOILER",
            12.0 * draw.scale,
            look.text,
        )))
        .size(Some(width), Some(height))
        .background(look.canvas)
        .radius(8.0);
        if draw.passive {
            return face;
        }
        Element::clickable(format!("{}:component:{n}:reveal", draw.base), face)
            .size(Some(width), Some(height))
            .accessible(Access::new(Role::Button).name(t("Mostrar spoiler")))
    }
}
