//! What the channel list and the conversation's header open, from plain
//! props: the context menu of a channel, a category or the list itself (with
//! its submenus), the bell's notification menu, the pinned messages popout,
//! the topic dialog, the dialogs that duplicate and delete a channel, and the
//! line over the composer that says the slow mode is on.

use limn::{
    AlertDialog, Button, ButtonVariant, Color, Component, ControlSize, DropdownMenu,
    DropdownMenuEntry, DropdownMenuItem, Element, Empty, Icon, IconButton, Markdown, Modal, Point,
    Popover, PopoverPlacement, ScrollArea, ScrollbarAppearance, Tooltip, TooltipPlacement,
    TooltipSpec,
};

use rune_i18n::{t, tr};

use crate::{avatar, gap, icon, line, strong_line, text, Look};

/// The actions these pieces report. Those that name a channel or a message
/// are built by whoever answers them.
pub mod ids {
    /// Reported when the context menu is dismissed; its submenus report it
    /// followed by `:` and their own ID.
    pub const MENU_DISMISS: &str = "cl:menu:dismiss";
    /// Items that only open a submenu start with this: choosing them does
    /// nothing.
    pub const SUBMENU: &str = "cl:sub:";

    /// The bell of the header and its menu.
    pub const BELL: &str = "cl:bell";
    pub const BELL_DISMISS: &str = "cl:bell:dismiss";

    /// The topic in the header opens it whole.
    pub const TOPIC: &str = "cl:topic";
    pub const TOPIC_CLOSE: &str = "cl:topic:close";

    pub const DUPLICATE_NAME: &str = "cl:duplicate:name";
    pub const DUPLICATE_SUBMIT: &str = "cl:duplicate:submit";
    pub const DUPLICATE_CLOSE: &str = "cl:duplicate:close";

    pub const DELETE_CONFIRM: &str = "cl:delete:confirm";
    pub const DELETE_CANCEL: &str = "cl:delete:cancel";

    /// A channel moved into another category: take its permissions, keep
    /// its own, or do not move it.
    pub const SYNC_CONFIRM: &str = "cl:sync:confirm";
    pub const SYNC_KEEP: &str = "cl:sync:keep";
    pub const SYNC_CANCEL: &str = "cl:sync:cancel";

    /// The pin of the header and its popout.
    pub const PINS: &str = "pins:toggle";
    pub const PINS_DISMISS: &str = "pins:dismiss";
    pub const PINS_RETRY: &str = "pins:retry";
    /// Followed by a message's ID: go to it in the conversation.
    pub const PIN_JUMP: &str = "pins:jump:";
    /// Followed by a message's ID: take it out of the pinned ones.
    pub const PIN_REMOVE: &str = "pins:unpin:";

    // Threads, forums and stages (2026-10-18).
    /// The «más» of a narrow header and its menu.
    pub const MORE: &str = "cl:more";
    pub const MORE_DISMISS: &str = "cl:more:dismiss";
}

/// A context menu's width: a channel's, a category's or the list's.
pub const MENU_WIDTH: f32 = 248.0;
/// The bell's menu, a little wider for the levels' explanations.
pub const BELL_WIDTH: f32 = 280.0;
/// The pinned messages popout.
pub const PINS_WIDTH: f32 = 448.0;
/// A confirmation's width.
pub const CONFIRM_WIDTH: f32 = 425.0;

/// An entry of a channel menu.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Item(Item),
    Separator,
    /// A muted title over the entries that follow.
    Heading(String),
}

/// An item of a channel menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    /// The action it reports.
    pub id: String,
    pub label: String,
    /// A line under the label, or beside it on an item with a submenu: the
    /// level a setting has, or until when something lasts.
    pub detail: Option<String>,
    /// Why it cannot be chosen, when it cannot: shown under its label.
    pub denied: Option<String>,
    /// It destroys something: drawn in the danger color.
    pub danger: bool,
    /// A toggle (a check box), or one choice of a set (a radio item) when
    /// `radio`, and whether it is on or the chosen one.
    pub checked: Option<bool>,
    /// The menu that opens beside it; empty for an action.
    pub submenu: Vec<Entry>,
    pub icon: Option<Icon>,
    /// `checked` is one choice of a set: drawn as a radio item.
    pub radio: bool,
}

impl Item {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn denied(mut self, reason: Option<String>) -> Self {
        self.denied = reason;
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// One choice of a set, `chosen` or not.
    pub fn radio(mut self, chosen: bool) -> Self {
        self.checked = Some(chosen);
        self.radio = true;
        self
    }

    pub fn submenu(mut self, entries: Vec<Entry>) -> Self {
        self.submenu = entries;
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl From<Item> for Entry {
    fn from(item: Item) -> Self {
        Self::Item(item)
    }
}

/// `groups` of entries with a line between each two, empty ones left out.
pub fn grouped(groups: Vec<Vec<Entry>>) -> Vec<Entry> {
    let mut entries = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !entries.is_empty() {
            entries.push(Entry::Separator);
        }
        entries.extend(group);
    }
    entries
}

fn dropdown_entries(entries: Vec<Entry>, look: &Look) -> Vec<DropdownMenuEntry> {
    entries
        .into_iter()
        .map(|entry| match entry {
            Entry::Separator => DropdownMenuEntry::Separator,
            Entry::Heading(title) => DropdownMenuEntry::Heading(title),
            Entry::Item(item) => DropdownMenuEntry::Item(dropdown_item(item, look)),
        })
        .collect()
}

fn dropdown_item(item: Item, look: &Look) -> DropdownMenuItem {
    let mut row = DropdownMenuItem::new(item.id, item.label);
    if let Some(glyph) = item.icon {
        row = row.icon(glyph);
    }
    if let Some(checked) = item.checked {
        row = if item.radio {
            row.radio(checked)
        } else {
            row.checkbox(checked)
        };
    }
    if let Some(reason) = item.denied {
        // The line under the label may be cut; the hint says it whole.
        return row
            .enabled(false)
            .description(reason.clone())
            .tooltip_with(TooltipSpec::new(reason));
    }
    if !item.submenu.is_empty() {
        row = row.submenu(dropdown_entries(item.submenu, look));
    }
    if let Some(detail) = item.detail {
        row = row.description(detail);
    }
    if item.danger {
        row = row.destructive(look.danger);
    }
    row
}

/// The context menu with `entries`, its corner at `at` in the window. It is
/// laid over everything: put it last in a stack the size of the window.
pub fn context_menu(at: (f32, f32), entries: Vec<Entry>, look: &Look) -> Element {
    DropdownMenu::new(
        gap(1.0, 1.0),
        dropdown_entries(entries, look),
        true,
        ids::MENU_DISMISS,
        look.menu_palette(),
    )
    .context_menu_style()
    .width(MENU_WIDTH)
    .at(Point::new(at.0.max(0.0), at.1.max(0.0)))
    .render()
}

/// The bell of the header, crossed out while the channel is `muted`, and its
/// menu of `entries` while `open`.
pub fn bell(open: bool, muted: bool, entries: Vec<Entry>, look: &Look) -> Element {
    let button = IconButton::new(
        ids::BELL,
        if muted { Icon::BellSlash } else { Icon::Bell },
        look.palette(),
    )
    .variant(ButtonVariant::Ghost)
    .tooltip(t("Ajustes de notificaciones"))
    .tooltip_placement(TooltipPlacement::Below)
    .ink(if open { look.text } else { look.dim })
    .hover_ink(look.text)
    .active(open)
    .render();
    DropdownMenu::new(
        button,
        dropdown_entries(entries, look),
        open,
        ids::BELL_DISMISS,
        look.menu_palette(),
    )
    .context_menu_style()
    .width(BELL_WIDTH)
    .placement(PopoverPlacement::BottomEnd)
    .render()
}

/// The «más» of a header too narrow for every tool: the tools that do not
/// fit, as the `entries` of its menu while `open` (2026-10-18).
pub fn more(open: bool, entries: Vec<Entry>, look: &Look) -> Element {
    let button = IconButton::new(ids::MORE, Icon::More, look.palette())
        .variant(ButtonVariant::Ghost)
        .tooltip(t("Más"))
        .tooltip_placement(TooltipPlacement::Below)
        .accessibility_label(t("Más herramientas"))
        .ink(if open { look.text } else { look.dim })
        .hover_ink(look.text)
        .active(open)
        .render();
    DropdownMenu::new(
        button,
        dropdown_entries(entries, look),
        open,
        ids::MORE_DISMISS,
        look.menu_palette(),
    )
    .context_menu_style()
    .width(MENU_WIDTH)
    .placement(PopoverPlacement::BottomEnd)
    .render()
}

/// A pinned message as the popout shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct PinCard {
    pub message: u64,
    pub author: String,
    /// The author's name's ink.
    pub ink: Color,
    /// Behind the author's initial.
    pub accent: Color,
    /// When it was written, as a person reads it.
    pub when: String,
    /// Markdown source.
    pub content: String,
    /// The server emoji it names, drawn in place of their tokens
    /// (2026-10-18: the tokens showed as written).
    pub pictures: Vec<crate::identity::TextPicture>,
}

/// Content of the currently opened pins request.
pub enum PinsContent<'a> {
    Loading,
    Failed(&'a str),
    Ready(&'a [PinCard]),
}

/// The header action and its request's content. A denied action explains why
/// and never exposes the previous content of the popout.
pub fn pins(
    open: bool,
    content: PinsContent<'_>,
    can_unpin: bool,
    denied: Option<&str>,
    look: &Look,
) -> Element {
    let mut control = IconButton::new(ids::PINS, Icon::Pin, look.palette())
        .variant(ButtonVariant::Ghost)
        .tooltip(t("Mensajes fijados"))
        .tooltip_placement(TooltipPlacement::Below)
        .ink(if open { look.text } else { look.dim })
        .hover_ink(look.text)
        .active(open && denied.is_none())
        .enabled(denied.is_none());
    if let Some(reason) = denied {
        control = control.tooltip_with(TooltipSpec::new(reason));
    }
    let mut button = control
        .render()
        .accessible(limn::Access::new(limn::Role::Button).name(t("Mensajes fijados")));
    if let Some(reason) = denied {
        button = button.accessible(
            limn::Access::new(limn::Role::Button)
                .name(t("Mensajes fijados"))
                .description(reason),
        );
    }
    if !open || denied.is_some() {
        return button;
    }
    let palette = look.menu_palette();
    let body = match content {
        // As the official client: a spinner a little down, no skeleton.
        PinsContent::Loading => Element::column(vec![
            Element::spacer().size(None, Some(32.0)),
            crate::centered(limn::Spinner::new("pins:loading", look.muted).render())
                .size(None, Some(32.0)),
            Element::spacer().size(None, Some(32.0)),
        ])
        .accessible(
            limn::Access::new(limn::Role::ProgressIndicator).name(t("Cargando mensajes fijados")),
        ),
        PinsContent::Failed(reason) => Element::column(vec![
            Empty::new(
                t("No se pudieron cargar los mensajes fijados"),
                reason,
                palette,
            )
            .icon(Icon::ErrorCircle)
            .render()
            .accessible(
                limn::Access::new(limn::Role::Alert)
                    .id("pins:error")
                    .name(t("No se pudieron cargar los mensajes fijados"))
                    .description(reason),
            ),
            Button::new(ids::PINS_RETRY, t("Volver a intentar"), palette)
                .variant(ButtonVariant::Secondary)
                .render()
                .centered_x(),
        ])
        .gap(8.0)
        .padding(8.0),
        PinsContent::Ready([]) => Empty::new(
            t("Todavía no hay mensajes fijados"),
            t("Los mensajes que se fijen en esta conversación van a aparecer acá."),
            palette,
        )
        .icon(Icon::Pin)
        .render(),
        PinsContent::Ready(cards) => ScrollArea::new(
            "pins:list",
            Element::column(
                cards
                    .iter()
                    .map(|card| pin_card(card, can_unpin, look))
                    .collect(),
            )
            .gap(8.0)
            .padding(6.0),
        )
        .appearance(ScrollbarAppearance::overlay().buttons(false))
        .render()
        .max_height(420.0),
    };
    let header = Element::row(vec![
        icon(Icon::Pin, look.muted).centered_y(),
        strong_line(t("Mensajes fijados"), 17.0, look.text).centered_y(),
    ])
    .gap(10.0)
    .padding(8.0)
    .size(None, Some(48.0));
    let content = Element::column(vec![
        header,
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.border),
        body,
    ])
    .gap(6.0);
    Popover::new(button, content, true, ids::PINS_DISMISS, palette)
        .placement(PopoverPlacement::BottomEnd)
        .width(PINS_WIDTH)
        .render()
}

fn pin_card(card: &PinCard, can_unpin: bool, look: &Look) -> Element {
    let palette = look.menu_palette();
    let text_width = PINS_WIDTH - 12.0 * 2.0 - 40.0 - 12.0 - 36.0;
    let mut said = Markdown::new(&card.content, look.palette())
        .width(text_width)
        .render()
        .element;
    crate::text::with_pictures(&mut said, &card.pictures);
    let body = Element::row(vec![
        avatar(&card.author, card.accent, 40.0),
        Element::column(vec![
            Element::row(vec![
                strong_line(&card.author, 15.0, card.ink).shrink_to_fit(),
                line(&card.when, 11.0, look.faint).centered_y(),
            ])
            .gap(8.0)
            .size(None, Some(20.0)),
            said,
        ])
        .gap(2.0)
        .flex(1.0),
    ])
    .gap(12.0);
    let mut tools = vec![
        Element::spacer().flex(1.0),
        Button::new(
            format!("{}{}", ids::PIN_JUMP, card.message),
            t("Ir"),
            palette,
        )
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Compact)
        .render(),
    ];
    if can_unpin {
        tools.push(
            IconButton::new(
                format!("{}{}", ids::PIN_REMOVE, card.message),
                Icon::Close,
                palette,
            )
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Compact)
            .tooltip(t("Desfijar mensaje"))
            .render(),
        );
    }
    let tools = Element::row(tools)
        .gap(4.0)
        .reveal_on_hover(format!("pins:card:{}", card.message));
    Element::stack(vec![
        Element::panel(body)
            .padding(12.0)
            .radius(8.0)
            .background(look.raised)
            .border(look.border, 1.0),
        Element::column(vec![tools.padding(6.0), Element::spacer().flex(1.0)]),
    ])
    .hover_region()
}

/// The whole topic of `channel`, over `root`, with the pictures it names.
pub fn topic_dialog(
    root: Element,
    channel: &str,
    topic: &str,
    pictures: &[crate::identity::TextPicture],
    width: f32,
    look: &Look,
) -> Element {
    let palette = look.menu_palette();
    Modal::new(
        root,
        true,
        t("Tema del canal"),
        format!("#{channel}"),
        ids::TOPIC_CLOSE,
        palette,
    )
    .width(width)
    .body({
        let mut topic = Markdown::new(topic, look.palette())
            .width((width - 52.0).max(120.0))
            .render()
            .element;
        crate::text::with_pictures(&mut topic, pictures);
        topic
    })
    .render()
}

/// A copy of a channel being made, as its dialog shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct DuplicateForm {
    /// The name of the channel copied.
    pub original: String,
    pub voice: bool,
    /// The category both are in.
    pub category: Option<String>,
    /// What is written in the name field.
    pub name: String,
}

impl DuplicateForm {
    /// The name the copy will be made with: a text channel's is lowercase
    /// and dashed.
    pub fn final_name(&self) -> String {
        if self.voice {
            self.name.trim().to_owned()
        } else {
            crate::server_menu::channel_name(&self.name)
        }
    }
}

/// The dialog that makes a copy of a channel, over `root`.
pub fn duplicate_dialog(root: Element, form: &DuplicateForm, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let name = form.final_name();
    let original = if form.voice {
        form.original.clone()
    } else {
        format!("#{}", form.original)
    };
    let input = Element::text_input(ids::DUPLICATE_NAME, &form.name, t("canal-nuevo"))
        .leading_icon(if form.voice { Icon::Volume } else { Icon::Hash })
        .leading_icon_tint(look.muted)
        .background(look.raised)
        .border(look.border, 1.0)
        .focus_border(look.accent, 2.0)
        .foreground(look.text)
        .font_family(limn::FontFamily::Sans)
        .font_size(15.0)
        .padding(10.0)
        .radius(8.0)
        .size(None, Some(40.0))
        .accessibility_label(t("Nombre del canal"));
    let mut body = vec![strong_line(t("Nombre del canal"), 14.0, look.text), input];
    if !form.voice && !name.is_empty() && name != form.name {
        body.push(line(tr!("Se va a llamar #{name}", name), 12.0, look.muted));
    }
    body.push(text(
        tr!(
            "Se va a crear un canal nuevo con los mismos permisos, tema y modo lento que \
             {original}.",
            original
        ),
        12.0,
        look.muted,
    ));
    Modal::new(
        root,
        true,
        t("Duplicar canal"),
        form.category
            .as_ref()
            .map(|category| tr!("en {category}", category))
            .unwrap_or_default(),
        ids::DUPLICATE_CLOSE,
        palette,
    )
    .width(width)
    .body(Element::column(body).gap(8.0))
    .footer(
        Element::row(vec![
            Button::new(ids::DUPLICATE_CLOSE, t("Cancelar"), palette)
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Spacious)
                .render()
                .flex(1.0),
            Button::new(
                ids::DUPLICATE_SUBMIT,
                t("Crear un canal"),
                look.accent_palette(),
            )
            .size(ControlSize::Spacious)
            .enabled(!name.is_empty())
            .render()
            .flex(1.0),
        ])
        .gap(9.0),
    )
    .initial_focus(ids::DUPLICATE_NAME)
    .render()
}

/// What is about to be deleted, for its confirmation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Doomed {
    Text,
    Voice,
    Category,
}

/// The question before deleting `name`, over `root`. Its focus starts on
/// «Cancelar»: Enter never deletes.
pub fn delete_dialog(root: Element, name: &str, doomed: Doomed, look: &Look) -> Element {
    let (title, question) = match doomed {
        Doomed::Text => (
            t("Eliminar canal"),
            tr!(
                "¿Seguro que querés eliminar #{name}? No se puede deshacer.",
                name
            ),
        ),
        Doomed::Voice => (
            t("Eliminar canal"),
            tr!(
                "¿Seguro que querés eliminar {name}? No se puede deshacer.",
                name
            ),
        ),
        Doomed::Category => (
            t("Eliminar categoría"),
            tr!(
                "¿Seguro que querés eliminar la categoría {name}? Sus canales quedan sin \
                 categoría. No se puede deshacer.",
                name
            ),
        ),
    };
    AlertDialog::new(
        root,
        true,
        title,
        question,
        ids::DELETE_CONFIRM,
        ids::DELETE_CANCEL,
        look.menu_palette(),
    )
    .width(CONFIRM_WIDTH)
    .confirm_label(title)
    .destructive(look.danger)
    .render()
}

/// The question before moving the channel `name` into the category
/// `category`, over `root`: whether it takes the category's permissions in
/// place of its own, moves keeping them, or does not move. Its focus starts
/// on «Cancelar».
pub fn sync_dialog(root: Element, name: &str, category: &str, look: &Look) -> Element {
    AlertDialog::new(
        root,
        true,
        t("¿Sincronizar permisos?"),
        tr!(
            "{name} pasa a {category}. ¿Querés que tome los permisos de la categoría? Sus permisos propios se reemplazan.",
            name,
            category
        ),
        ids::SYNC_CONFIRM,
        ids::SYNC_CANCEL,
        look.menu_palette(),
    )
    .width(CONFIRM_WIDTH)
    .confirm_label(t("Sincronizar permisos"))
    .alternative(ids::SYNC_KEEP, t("Mover sin sincronizar"))
    .render()
}

/// How long the slow mode makes people wait, as a person reads it.
pub fn interval(secs: u32) -> String {
    match secs {
        1 => t("1 segundo").to_owned(),
        0 | 2..60 => tr!("{seconds} segundos", seconds = secs),
        60 => t("1 minuto").to_owned(),
        3_600 => t("1 hora").to_owned(),
        _ if secs.is_multiple_of(3_600) => tr!("{hours} horas", hours = secs / 3_600),
        _ if secs.is_multiple_of(60) => tr!("{minutes} minutos", minutes = secs / 60),
        _ => tr!(
            "{minutes} min {seconds} s",
            minutes = secs / 60,
            seconds = secs % 60
        ),
    }
}

/// The line over the composer while the slow mode is on: who is `immune`
/// is told so; the rest, how long they wait between two messages.
pub fn slowmode(secs: u32, immune: bool, look: &Look) -> Element {
    let wait = interval(secs);
    let said = if immune {
        t("Inmune al modo lento").to_owned()
    } else {
        tr!("Modo lento: un mensaje cada {wait}", wait)
    };
    let mark = Element::row(vec![
        icon(Icon::Clock, look.muted).centered_y(),
        line(said, 12.0, look.muted).centered_y(),
    ])
    .gap(6.0);
    Element::row(vec![
        Element::spacer().flex(1.0),
        Tooltip::new(
            mark,
            "cl:slowmode",
            tr!(
                "El modo lento está activado. Los miembros pueden enviar un mensaje cada {wait}.",
                wait
            ),
            look.palette(),
        )
        .render(),
        gap(16.0, 0.0),
    ])
    .size(None, Some(20.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    const WINDOW: Size = Size::new(900.0, 700.0);

    #[test]
    fn the_slow_mode_reads_in_the_unit_that_fits() {
        assert_eq!(interval(1), "1 segundo");
        assert_eq!(interval(10), "10 segundos");
        assert_eq!(interval(120), "2 minutos");
        assert_eq!(interval(3_600), "1 hora");
        assert_eq!(interval(90), "1 min 30 s");
        let (_, immune) = frame(slowmode(5, true, &Look::DARK), WINDOW);
        assert!(texts(&immune).contains(&"Inmune al modo lento".to_owned()));
        let (_, waits) = frame(slowmode(300, false, &Look::DARK), WINDOW);
        assert!(texts(&waits).contains(&"Modo lento: un mensaje cada 5 minutos".to_owned()));
    }

    fn menu(entries: Vec<Entry>) -> limn::Frame {
        let (_, frame) = frame(
            Element::stack(vec![
                Element::spacer(),
                context_menu((40.0, 60.0), entries, &Look::DARK),
            ]),
            WINDOW,
        );
        frame
    }

    #[test]
    fn a_context_menu_opens_where_asked_and_says_why_an_item_is_off() {
        let frame = menu(grouped(vec![
            vec![Item::new("a", "Marcar como leído").into()],
            Vec::new(),
            vec![
                Item::new("b", "Editar canal")
                    .denied(Some("No tenés permiso".into()))
                    .into(),
                Item::new("c", "Eliminar canal").danger().into(),
            ],
        ]));
        let ids = controls(&frame);
        assert!(ids.contains(&"a".to_owned()) && ids.contains(&"c".to_owned()));
        assert!(!ids.contains(&"b".to_owned()), "{ids:?}");
        let off = frame.access.iter().find(|node| node.id == "b").unwrap();
        assert!(off.disabled);
        assert!(texts(&frame).contains(&"No tenés permiso".to_owned()));
        let first = frame
            .regions
            .iter()
            .find(|region| region.id == "a")
            .unwrap();
        assert!(
            first.rect.x >= 40.0 && first.rect.y >= 60.0 && first.rect.x < 80.0,
            "{:?}",
            first.rect
        );
    }

    #[test]
    fn the_pins_popout_says_when_there_are_none_and_offers_to_go_to_each() {
        let (_, empty) = frame(
            Element::row(vec![
                Element::spacer().flex(1.0),
                pins(true, PinsContent::Ready(&[]), true, None, &Look::DARK),
            ]),
            WINDOW,
        );
        assert!(texts(&empty).contains(&"Todavía no hay mensajes fijados".to_owned()));
        let card = PinCard {
            message: 7,
            author: "Ada".into(),
            ink: Color::rgb(200, 200, 200),
            accent: Color::rgb(1, 2, 3),
            when: "Hoy a las 10:00".into(),
            content: "**hola** <:wave:1>".into(),
            // Its server emoji, as a picture (2026-10-18).
            pictures: vec![crate::identity::TextPicture {
                token: "<:wave:1>".into(),
                name: "wave".into(),
                image: limn::RasterImage::new(
                    "a".repeat(64),
                    limn::RasterImageFormat::Png,
                    3,
                    std::sync::Arc::from("AAAA"),
                    "wave",
                ),
                playing: false,
            }],
        };
        let (_, listed) = frame(
            Element::row(vec![
                Element::spacer().flex(1.0),
                pins(
                    true,
                    PinsContent::Ready(std::slice::from_ref(&card)),
                    false,
                    None,
                    &Look::DARK,
                ),
            ]),
            WINDOW,
        );
        let ids = controls(&listed);
        assert!(ids.contains(&"pins:jump:7".to_owned()), "{ids:?}");
        assert!(
            !ids.contains(&"pins:unpin:7".to_owned()),
            "not allowed here"
        );
        assert!(texts(&listed).contains(&"Mensajes fijados".to_owned()));
        assert!(!texts(&listed).iter().any(|text| text.contains("<:wave:1>")));
        assert!(listed.commands.iter().any(|command| matches!(
            command,
            limn::DrawCommand::RasterImage { image, .. } if image.alt() == "wave"
        )));
    }

    #[test]
    fn moving_into_a_category_asks_whether_it_takes_its_permissions() {
        let (_, shown) = frame(
            sync_dialog(Element::spacer(), "#rendimiento", "Voz", &Look::DARK),
            WINDOW,
        );
        let ids = controls(&shown);
        for id in [ids::SYNC_CONFIRM, ids::SYNC_KEEP, ids::SYNC_CANCEL] {
            assert!(ids.contains(&id.to_owned()), "{id} in {ids:?}");
        }
        assert!(texts(&shown)
            .iter()
            .any(|text| text.contains("#rendimiento pasa a Voz")));
    }

    #[test]
    fn a_copy_takes_a_text_channels_name_lowercase_and_needs_one() {
        let mut form = DuplicateForm {
            original: "general".into(),
            voice: false,
            category: Some("Desarrollo".into()),
            name: "Copia Nueva".into(),
        };
        assert_eq!(form.final_name(), "copia-nueva");
        let (_, shown) = frame(
            duplicate_dialog(Element::spacer(), &form, 510.0, &Look::DARK),
            WINDOW,
        );
        assert!(texts(&shown).contains(&"en Desarrollo".to_owned()));
        assert!(controls(&shown).contains(&ids::DUPLICATE_SUBMIT.to_owned()));
        form.name = "  ".into();
        let (_, empty) = frame(
            duplicate_dialog(Element::spacer(), &form, 510.0, &Look::DARK),
            WINDOW,
        );
        assert!(!controls(&empty).contains(&ids::DUPLICATE_SUBMIT.to_owned()));
    }
}
