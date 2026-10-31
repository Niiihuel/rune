//! The friends page of home, from plain props: its header with tabs, the
//! search, a person with their actions and the menu of the rest, the form
//! that sends a request by handle, and the question before ending something.

use limn::{
    AlertDialog, Button, ButtonVariant, Color, Component, ControlSize, DropdownMenu,
    DropdownMenuEntry, DropdownMenuItem, Element, Empty, FontFamily, Icon, IconButton,
    PopoverPlacement, SearchField, Tab, Tabs, TabsVariant, TextField,
};

use rune_i18n::{t, tr};

use crate::{avatar_with_image, gap, line, strong_line, text, with_status, Look};

/// The actions these pieces report. A person's own end with their ID and
/// the action: `friend:{user}:accept`.
pub mod ids {
    /// The row over the direct conversations that opens this page.
    pub const HOME: &str = "home:friends";
    /// Followed by the key of a [`super::FriendsTab`].
    pub const TAB: &str = "friends:tab:";
    pub const QUERY: &str = "friends:query";
    pub const USERNAME: &str = "friends:add:username";
    pub const SEND: &str = "friends:add:send";
    pub const MENU_DISMISS: &str = "friends:menu:dismiss";
    pub const CONFIRM: &str = "friends:confirm";
    pub const CONFIRM_CANCEL: &str = "friends:confirm:cancel";
    /// The dialog that gives a friend a private name.
    pub const NICKNAME: &str = "friends:nickname";
    pub const NICKNAME_SAVE: &str = "friends:nickname:save";
    pub const NICKNAME_CLEAR: &str = "friends:nickname:clear";
    pub const NICKNAME_CLOSE: &str = "friends:nickname:close";
    /// The question about ignoring offers blocking instead, as a link.
    pub const IGNORE_BLOCK: &str = "friends:ignore:block";

    /// What a person's control reports: `friend:{user}:{action}`.
    pub fn person(user: impl std::fmt::Display, action: &str) -> String {
        format!("friend:{user}:{action}")
    }
}

/// A person's row is this high.
pub const ROW: f32 = 62.0;
/// A section's heading is this high.
pub const SECTION: f32 = 40.0;
/// The question before removing or blocking.
pub const CONFIRM_WIDTH: f32 = 440.0;

/// Which people the page shows.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FriendsTab {
    #[default]
    Online,
    All,
    Pending,
    Blocked,
    Add,
}

impl FriendsTab {
    pub const ALL: [Self; 5] = [
        Self::Online,
        Self::All,
        Self::Pending,
        Self::Blocked,
        Self::Add,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::All => "all",
            Self::Pending => "pending",
            Self::Blocked => "blocked",
            Self::Add => "add",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Online => t("En línea"),
            Self::All => t("Todos"),
            Self::Pending => t("Pendientes"),
            Self::Blocked => t("Bloqueados"),
            Self::Add => t("Añadir amigo"),
        }
    }
}

/// Something that can be done about a person, and why not when it cannot.
#[derive(Clone, Debug, PartialEq)]
pub struct FriendAction {
    pub id: String,
    pub label: String,
    pub icon: Icon,
    pub denied: Option<String>,
    /// It ends something: drawn in the danger color.
    pub danger: bool,
}

/// One person of the page.
#[derive(Clone, Debug, PartialEq)]
pub struct FriendRow {
    pub name: String,
    pub handle: String,
    pub image: Option<limn::RasterImage>,
    pub accent: Color,
    /// Their status dot, when the provider reports presence.
    pub status: Option<Color>,
    /// Their status, or what kind of request it is.
    pub detail: String,
    /// A server's emoji before `detail`, when that is their custom status
    /// (2026-10-18).
    pub detail_emoji: Option<crate::identity::TextPicture>,
    /// The round buttons, in order.
    pub actions: Vec<FriendAction>,
    /// What the "more" button offers; empty, there is no such button.
    pub more: Vec<FriendAction>,
    /// The ID of the "more" button, and whether its menu is open.
    pub more_id: String,
    pub menu_open: bool,
}

/// The row over the direct conversations that opens this page, with the
/// count of requests waiting for the account.
pub fn home_entry(selected: bool, pending: usize, look: &Look) -> Element {
    let mut parts = vec![
        gap(14.0, 0.0),
        Element::icon(Icon::Users)
            .foreground(if selected { look.text } else { look.dim })
            .size(Some(20.0), Some(20.0))
            .centered_y(),
        gap(18.0, 0.0),
        strong_line(
            t("Amigos"),
            16.0,
            if selected { look.text } else { look.dim },
        )
        .flex(1.0)
        .centered_y(),
    ];
    if pending > 0 {
        parts.push(crate::sidebar::count(pending as u32, look).centered_y());
    }
    parts.push(gap(8.0, 0.0));
    let body = Element::clickable(ids::HOME, Element::row(parts))
        .size(None, Some(43.0))
        .radius(6.0)
        .background(if selected {
            look.selected
        } else {
            Color::rgba(0, 0, 0, 0)
        })
        .hover_background(if selected { look.selected } else { look.hover })
        .accessible(
            limn::Access::new(limn::Role::ListItem)
                .name(t("Amigos"))
                .selected(selected),
        );
    Element::row(vec![
        gap(crate::sidebar::INSET, 0.0),
        body.flex(1.0),
        gap(crate::sidebar::INSET, 0.0),
    ])
}

/// The header of the page: what it is, and its tabs. `pending` counts the
/// requests waiting for the account.
pub fn header(tab: FriendsTab, pending: usize, look: &Look) -> Element {
    let tabs = FriendsTab::ALL
        .into_iter()
        .map(|candidate| {
            let label = match candidate {
                FriendsTab::Pending if pending > 0 => format!("{} · {pending}", candidate.label()),
                _ => candidate.label().to_owned(),
            };
            let width = label.chars().count() as f32 * 7.5 + 24.0;
            Tab::new(format!("{}{}", ids::TAB, candidate.key()), label, width)
                .active(candidate == tab)
        })
        .collect();
    let tabs = Tabs::new(tabs, look.palette())
        .variant(TabsVariant::Underlined)
        .accent(look.accent)
        .id("friends:tabs")
        .height(28.0)
        .render();
    Element::column(vec![
        Element::row(vec![
            gap(16.0, 0.0),
            Element::icon(Icon::Users)
                .foreground(look.muted)
                .size(Some(20.0), Some(20.0))
                .centered_y(),
            strong_line(t("Amigos"), 16.0, look.text)
                .flex(1.0)
                .centered_y(),
        ])
        .gap(12.0)
        .size(None, Some(44.0)),
        // A narrow window scrolls the tabs sideways instead of cutting them.
        Element::row(vec![
            gap(12.0, 0.0),
            Element::horizontal_scroll("friends:tabs-scroll", tabs).flex(1.0),
            gap(12.0, 0.0),
        ])
        .size(None, Some(36.0)),
    ])
}

/// The search over the people of a tab.
pub fn search(query: &str, look: &Look) -> Element {
    SearchField::new(ids::QUERY, query, t("Buscar"), look.menu_palette())
        .accessibility_label(t("Buscar amigos"))
        .render()
}

/// A heading over a group of people: "EN LÍNEA — 3".
pub fn section(title: &str, count: usize, look: &Look) -> Element {
    Element::column(vec![
        Element::spacer().flex(1.0),
        line(
            format!("{} — {count}", title.to_uppercase()),
            12.0,
            look.muted,
        )
        .font_family(FontFamily::SansMedium),
        gap(0.0, 8.0),
    ])
    .size(None, Some(SECTION))
}

/// A person of the page, with their actions on the right.
pub fn row(row: &FriendRow, look: &Look) -> Element {
    let face = avatar_with_image(&row.name, row.accent, 32.0, row.image.clone());
    let face = match row.status {
        Some(status) => with_status(face, 32.0, status, look.canvas),
        None => face,
    };
    let mut parts = vec![
        face.centered_y(),
        gap(12.0, 0.0),
        Element::column(vec![
            Element::row(vec![
                strong_line(&row.name, 15.0, look.text).shrink_to_fit(),
                line(&row.handle, 13.0, look.muted).shrink_to_fit(),
            ])
            .gap(6.0),
            crate::identity::status_line(
                Some(&row.detail),
                row.detail_emoji.as_ref(),
                13.0,
                look.muted,
                look,
            )
            .unwrap_or_else(|| line("", 13.0, look.muted)),
        ])
        .gap(2.0)
        .flex(1.0)
        .centered_y(),
    ];
    for action in &row.actions {
        parts.push(gap(10.0, 0.0));
        parts.push(round(action, look).centered_y());
    }
    if !row.more.is_empty() {
        let anchor = round(
            &FriendAction {
                id: row.more_id.clone(),
                label: t("Más").into(),
                icon: Icon::More,
                denied: None,
                danger: false,
            },
            look,
        );
        let entries = row.more.iter().map(|action| entry(action, look)).collect();
        parts.push(gap(10.0, 0.0));
        parts.push(
            DropdownMenu::new(
                anchor,
                entries,
                row.menu_open,
                ids::MENU_DISMISS,
                look.menu_palette(),
            )
            .placement(PopoverPlacement::BottomEnd)
            .width(220.0)
            .render()
            .centered_y(),
        );
    }
    Element::column(vec![
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.border),
        Element::row(parts).flex(1.0),
    ])
    .size(None, Some(ROW))
    .accessible(limn::Access::new(limn::Role::ListItem).name(row.name.clone()))
}

fn round(action: &FriendAction, look: &Look) -> Element {
    let hint = match &action.denied {
        Some(reason) => format!("{}: {reason}", action.label),
        None => action.label.clone(),
    };
    IconButton::new(action.id.clone(), action.icon, look.menu_palette())
        .variant(ButtonVariant::Raised)
        .round()
        .tooltip(hint.clone())
        .accessibility_label(hint)
        .ink(if action.danger {
            look.danger
        } else {
            look.muted
        })
        .enabled(action.denied.is_none())
        .render()
}

fn entry(action: &FriendAction, look: &Look) -> DropdownMenuEntry {
    let item = DropdownMenuItem::new(action.id.clone(), action.label.clone()).icon(action.icon);
    let item = match &action.denied {
        Some(reason) => item.description(reason.clone()).disabled(),
        None if action.danger => item.destructive(look.danger),
        None => item,
    };
    DropdownMenuEntry::Item(item)
}

/// What the page says when a tab has nobody to show, or cannot show them.
pub fn empty(title: &str, detail: &str, look: &Look) -> Element {
    crate::centered(
        Empty::new(title, detail, look.palette())
            .icon(Icon::Users)
            .render(),
    )
}

/// The form that asks someone to be friends by their handle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AddFriend {
    pub username: String,
    /// The answer to the last request: what it says, and whether it failed.
    pub note: Option<(String, bool)>,
    /// Why requests cannot be sent, when they cannot.
    pub denied: Option<String>,
    /// A request left and has not been answered.
    pub sending: bool,
    /// The simulation answers: said so under the form.
    pub simulated: bool,
}

pub fn add_friend(form: &AddFriend, look: &Look) -> Element {
    let palette = look.menu_palette();
    let field = TextField::new(
        ids::USERNAME,
        form.username.clone(),
        t("Nombre de usuario"),
        palette,
    )
    .accessibility_label(t("Nombre de usuario"))
    .size(ControlSize::Spacious);
    let field = if form.denied.is_some() {
        field.disabled()
    } else {
        field
    }
    .render();
    let send = Button::new(ids::SEND, t("Enviar solicitud"), look.accent_palette())
        .size(ControlSize::Spacious)
        .loading(form.sending)
        .enabled(form.denied.is_none() && !form.username.trim().is_empty() && !form.sending)
        .render();
    let mut parts = vec![
        strong_line(t("Añadir amigo"), 16.0, look.text),
        text(
            t("Puedes añadir amigos con su nombre de usuario."),
            14.0,
            look.muted,
        ),
        gap(0.0, 4.0),
        field,
        Element::row(vec![Element::spacer().flex(1.0), send]),
    ];
    match (&form.denied, &form.note) {
        (Some(reason), _) => parts.push(text(reason.clone(), 13.0, look.danger)),
        (None, Some((note, failed))) => parts.push(text(
            note.clone(),
            13.0,
            if *failed { look.danger } else { look.online },
        )),
        (None, None) => {}
    }
    if form.simulated {
        parts.push(text(
            t("Simulación local: las solicitudes se confirman en este equipo, sin enviar nada."),
            12.0,
            look.faint,
        ));
    }
    Element::column(parts).gap(8.0)
}

/// The dialog that gives `name`, a friend, a private name, over `root`.
pub fn nickname_dialog(root: Element, name: &str, value: &str, look: &Look) -> Element {
    let palette = look.menu_palette();
    let body = Element::column(vec![
        TextField::new(ids::NICKNAME, value.to_owned(), t("Sin apodo"), palette)
            .accessibility_label(t("Apodo"))
            .size(ControlSize::Spacious)
            .render(),
        text(
            t("Hasta 32 caracteres. Se ve en tu lista de amigos."),
            12.0,
            look.muted,
        ),
    ])
    .gap(8.0);
    let footer = Element::row(vec![
        Button::new(ids::NICKNAME_CLEAR, t("Quitar apodo"), palette)
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Spacious)
            .render(),
        Element::spacer().flex(1.0),
        Button::new(ids::NICKNAME_CLOSE, t("Cancelar"), palette)
            .variant(ButtonVariant::Secondary)
            .size(ControlSize::Spacious)
            .render(),
        Button::new(ids::NICKNAME_SAVE, t("Guardar"), look.accent_palette())
            .size(ControlSize::Spacious)
            .render(),
    ])
    .gap(9.0);
    limn::Modal::new(
        root,
        true,
        t("Cambiar apodo"),
        tr!("Un nombre para {name} que solo ves tú.", name),
        ids::NICKNAME_CLOSE,
        palette,
    )
    .width(CONFIRM_WIDTH)
    .body(body)
    .footer(footer)
    .render()
}

/// The question before ending something with someone, over `root`.
pub fn confirm(root: Element, title: &str, body: &str, confirm: &str, look: &Look) -> Element {
    AlertDialog::new(
        root,
        true,
        title.to_owned(),
        body.to_owned(),
        ids::CONFIRM,
        ids::CONFIRM_CANCEL,
        look.menu_palette(),
    )
    .width(CONFIRM_WIDTH)
    .confirm_label(confirm.to_owned())
    .destructive(look.danger)
    .render()
}

/// Ignoring someone, as the official client asks: what it does and what it
/// does not, and blocking as the other way, offered in a link. Confirmed
/// with `ids::CONFIRM`, cancelled with `ids::CONFIRM_CANCEL`.
pub fn ignore(root: Element, name: &str, look: &Look) -> Element {
    let point = |words: &str| {
        Element::row(vec![
            text("•", 14.0, look.muted),
            gap(8.0, 0.0),
            text(words.to_owned(), 14.0, look.muted).flex(1.0),
        ])
    };
    let body = Element::column(vec![
        point(t("Ocultar su perfil y mensajes")),
        point(t("Puedes volver a mostrarlos en cualquier momento")),
        point(t("Silencia sus notificaciones y actividad")),
        point(t("Verás menos de esta persona")),
        point(t("Todavía pueden enviarte mensajes y ver tu actividad")),
        point(t("No sabrán que los ignoraste")),
        Element::column(vec![
            strong_line(t("¿No es suficiente?"), 14.0, look.text),
            Button::new(
                ids::IGNORE_BLOCK,
                t("Optar por bloquear"),
                look.menu_palette(),
            )
            .variant(ButtonVariant::Link)
            .size(ControlSize::Compact)
            .render(),
            text(
                t("Detén el contacto directo y limita lo que ven"),
                12.0,
                look.muted,
            ),
        ])
        .gap(4.0),
    ])
    .gap(8.0);
    AlertDialog::new(
        root,
        true,
        tr!("¿Deseas ignorar a {name}?", name),
        "",
        ids::CONFIRM,
        ids::CONFIRM_CANCEL,
        look.menu_palette(),
    )
    .width(CONFIRM_WIDTH)
    .confirm_label(t("Ignorar"))
    .body(body)
    .render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    #[test]
    fn the_ignore_question_names_the_person_and_offers_blocking_instead() {
        let (_, laid) = frame(
            ignore(Element::spacer(), "Ada", &Look::DARK),
            Size::new(600.0, 640.0),
        );
        let painted = texts(&laid);
        assert!(
            painted.contains(&"¿Deseas ignorar a Ada?".to_owned()),
            "{painted:?}"
        );
        assert!(painted.contains(&"Verás menos de esta persona".to_owned()));
        let shown = controls(&laid);
        assert!(shown.contains(&ids::IGNORE_BLOCK.to_owned()));
        assert!(shown.contains(&ids::CONFIRM.to_owned()));
    }

    fn person(actions: Vec<FriendAction>, more: Vec<FriendAction>, open: bool) -> FriendRow {
        FriendRow {
            name: "Ada".into(),
            handle: "ada".into(),
            image: None,
            accent: Color::rgb(90, 120, 200),
            status: Some(Look::DARK.online),
            detail: "En línea".into(),
            detail_emoji: None,
            actions,
            more,
            more_id: ids::person(10, "more"),
            menu_open: open,
        }
    }

    fn action(id: &str, denied: Option<&str>) -> FriendAction {
        FriendAction {
            id: id.into(),
            label: id.into(),
            icon: Icon::Chat,
            denied: denied.map(str::to_owned),
            danger: false,
        }
    }

    #[test]
    fn a_custom_status_shows_its_servers_emoji_before_its_words() {
        let emoji = crate::identity::TextPicture {
            token: "<:ok:1>".into(),
            name: "ok".into(),
            image: limn::RasterImage::new(
                "a".repeat(64),
                limn::RasterImageFormat::Png,
                3,
                std::sync::Arc::from("AAAA"),
                "ok",
            ),
            playing: false,
        };
        let shown = FriendRow {
            detail: "de vacaciones".into(),
            detail_emoji: Some(emoji),
            ..person(Vec::new(), Vec::new(), false)
        };
        let (_, frame) = frame(row(&shown, &Look::DARK), Size::new(700.0, 62.0));
        assert!(texts(&frame).contains(&"de vacaciones".to_owned()));
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            limn::DrawCommand::RasterImage { rect, .. } if rect.width == 14.0
        )));
    }

    #[test]
    fn the_header_offers_every_tab_and_counts_what_waits() {
        let (_, frame) = frame(
            header(FriendsTab::Pending, 2, &Look::DARK),
            Size::new(900.0, 48.0),
        );
        let found = controls(&frame);
        for tab in FriendsTab::ALL {
            assert!(
                found.contains(&format!("friends:tab:{}", tab.key())),
                "{found:?}"
            );
        }
        assert!(texts(&frame).contains(&"Pendientes · 2".to_owned()));
    }

    #[test]
    fn a_row_shows_its_actions_and_a_denied_one_cannot_be_used() {
        let row = person(
            vec![
                action("friend:10:message", None),
                action(
                    "friend:10:accept",
                    Some("Esta conexión no permite gestionar amistades"),
                ),
            ],
            vec![action("friend:10:block", None)],
            true,
        );
        let (_, frame) = frame(
            super::row(&row, &Look::DARK).size(Some(700.0), None),
            Size::new(800.0, 400.0),
        );
        let found = controls(&frame);
        assert!(found.contains(&"friend:10:message".to_owned()));
        assert!(!found.contains(&"friend:10:accept".to_owned()), "{found:?}");
        assert!(found.contains(&"friend:10:more".to_owned()));
        // The menu is open: its item is there.
        assert!(found.contains(&"friend:10:block".to_owned()), "{found:?}");
        let painted = texts(&frame);
        assert!(painted.contains(&"Ada".to_owned()) && painted.contains(&"En línea".to_owned()));
    }

    #[test]
    fn the_add_form_says_why_it_cannot_send_and_labels_the_simulation() {
        let denied = AddFriend {
            username: "zoe".into(),
            denied: Some("Esta conexión no permite enviar solicitudes de amistad".into()),
            ..AddFriend::default()
        };
        let (_, laid) = frame(add_friend(&denied, &Look::DARK), Size::new(700.0, 300.0));
        assert!(!controls(&laid).contains(&ids::SEND.to_owned()));
        assert!(texts(&laid)
            .iter()
            .any(|text| text.contains("no permite enviar solicitudes")));
        let simulated = AddFriend {
            username: "zoe".into(),
            simulated: true,
            note: Some(("Solicitud enviada a zoe".into(), false)),
            ..AddFriend::default()
        };
        let (_, frame) = frame(add_friend(&simulated, &Look::DARK), Size::new(700.0, 300.0));
        assert!(controls(&frame).contains(&ids::SEND.to_owned()));
        let painted = texts(&frame);
        assert!(painted
            .iter()
            .any(|text| text.starts_with("Simulación local")));
        assert!(painted.contains(&"Solicitud enviada a zoe".to_owned()));
    }

    #[test]
    fn ending_something_asks_first() {
        let (_, frame) = frame(
            confirm(
                Element::spacer(),
                "Bloquear a Ada",
                "No va a poder escribirte.",
                "Bloquear",
                &Look::DARK,
            ),
            Size::new(900.0, 600.0),
        );
        let found = controls(&frame);
        assert!(
            found.contains(&ids::CONFIRM.to_owned())
                && found.contains(&ids::CONFIRM_CANCEL.to_owned())
        );
        assert!(texts(&frame).contains(&"Bloquear".to_owned()));
    }
}
