//! The dialog that manages a group conversation, from plain props: its
//! picture, its name, its people (who owns it is marked), the friends that
//! can join, and leaving it. Removing someone and leaving are asked first,
//! apart. And the dialog that makes a new group from chosen friends.

use limn::{
    AlertDialog, Button, ButtonVariant, Checkbox, Color, Component, ControlSize, Element, Icon,
    Modal, SearchField, TextField,
};

use rune_i18n::{t, tr};

use crate::{avatar_with_image, gap, line, strong_line, text, Look};

/// The actions these pieces report; a group's own carry its ID.
pub mod ids {
    pub const CLOSE: &str = "group:close";
    pub const CONFIRM: &str = "group:confirm";
    pub const CONFIRM_CANCEL: &str = "group:confirm:cancel";

    /// The dialog that makes a new group: opening it, its search, choosing
    /// a friend (`group:new:pick:{user}`), making it and closing it.
    pub const NEW: &str = "group:new";
    pub const NEW_QUERY: &str = "group:new:query";
    pub const NEW_PICK: &str = "group:new:pick:";
    pub const NEW_CREATE: &str = "group:new:create";
    pub const NEW_CLOSE: &str = "group:new:close";

    /// `group:{channel}:{action}`: `manage`, `name`, `rename`, `leave`,
    /// `icon`, `icon:remove`, `close` (a direct conversation), or
    /// `add:{user}`, `remove:{user}` and `owner:{user}`.
    pub fn of(channel: impl std::fmt::Display, action: &str) -> String {
        format!("group:{channel}:{action}")
    }
}

pub const WIDTH: f32 = 480.0;

/// Someone in the dialog, and what can be done about them here.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupPerson {
    pub name: String,
    pub handle: String,
    pub image: Option<limn::RasterImage>,
    pub accent: Color,
    pub owner: bool,
    /// The buttons beside them: each one's ID and label, and why it cannot
    /// be used, when it cannot.
    pub actions: Vec<(String, String, Option<String>)>,
}

/// What the dialog shows.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupForm {
    /// The group's ID, for the IDs of its controls.
    pub channel: u64,
    pub title: String,
    /// What is written in the name field.
    pub name: String,
    /// Why it cannot be renamed, when it cannot.
    pub rename_denied: Option<String>,
    pub can_save: bool,
    pub members: Vec<GroupPerson>,
    pub candidates: Vec<GroupPerson>,
    /// Said instead of the candidates when there are none, or none can be known.
    pub candidates_note: Option<String>,
    pub leave_denied: Option<String>,
    /// Its picture, when it has one and it loaded.
    pub icon: Option<limn::RasterImage>,
    pub accent: Color,
    /// It has a picture to take away.
    pub has_icon: bool,
    /// Why its picture cannot be changed, when it cannot.
    pub icon_denied: Option<String>,
    /// Said under the picture: that a simulated one is not drawn.
    pub icon_note: Option<String>,
}

/// A friend the new group may have.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub user: u64,
    pub name: String,
    pub handle: String,
    pub image: Option<limn::RasterImage>,
    pub accent: Color,
    pub chosen: bool,
}

/// What the dialog that makes a group shows.
#[derive(Clone, Debug, PartialEq)]
pub struct NewGroupForm {
    pub query: String,
    /// The friends that match the search, by name.
    pub friends: Vec<Choice>,
    pub chosen: usize,
    /// The most friends a group takes.
    pub most: usize,
    /// Said instead of the friends when there are none to show.
    pub note: Option<String>,
    /// Why it cannot be made, when it cannot; waiting for the provider too.
    pub denied: Option<String>,
}

pub fn dialog(root: Element, form: &GroupForm, look: &Look) -> Element {
    let palette = look.menu_palette();
    let field = TextField::new(
        ids::of(form.channel, "name"),
        form.name.clone(),
        t("Sin nombre: se ven los nombres de sus integrantes"),
        palette,
    )
    .accessibility_label(t("Nombre del grupo"))
    .size(ControlSize::Spacious);
    let field = if form.rename_denied.is_some() {
        field.disabled()
    } else {
        field
    };
    let save = Button::new(
        ids::of(form.channel, "rename"),
        t("Guardar"),
        look.accent_palette(),
    )
    .size(ControlSize::Spacious)
    .enabled(form.can_save && form.rename_denied.is_none())
    .render();
    let face = match &form.icon {
        Some(image) => avatar_with_image(&form.title, form.accent, 56.0, Some(image.clone())),
        None => crate::sidebar::group_mark(form.accent, 56.0),
    };
    let change = Button::new(
        ids::of(form.channel, "icon"),
        t("Cambiar imagen"),
        look.menu_palette(),
    )
    .variant(ButtonVariant::Secondary)
    .size(ControlSize::Compact)
    .enabled(form.icon_denied.is_none())
    .render();
    let remove = Button::new(
        ids::of(form.channel, "icon:remove"),
        t("Quitar imagen"),
        look.menu_palette(),
    )
    .variant(ButtonVariant::Ghost)
    .size(ControlSize::Compact)
    .enabled(form.icon_denied.is_none() && form.has_icon)
    .render();
    let mut beside = vec![Element::row(vec![change, remove]).gap(8.0)];
    if let Some(reason) = form.icon_denied.as_ref().or(form.icon_note.as_ref()) {
        beside.push(text(reason.clone(), 12.0, look.muted));
    }
    let mut parts = vec![
        Element::row(vec![
            face.centered_y(),
            Element::column(beside).gap(6.0).flex(1.0).centered_y(),
        ])
        .gap(14.0),
        gap(0.0, 4.0),
        strong_line(t("Nombre del grupo"), 14.0, look.text),
        Element::row(vec![field.render().flex(1.0), save]).gap(9.0),
    ];
    if let Some(reason) = &form.rename_denied {
        parts.push(text(reason.clone(), 12.0, look.muted));
    }
    parts.push(gap(0.0, 4.0));
    parts.push(strong_line(
        tr!("Integrantes — {count}", count = form.members.len()),
        14.0,
        look.text,
    ));
    parts.extend(form.members.iter().map(|person| person_row(person, look)));
    parts.push(gap(0.0, 4.0));
    parts.push(strong_line(t("Añadir amigos"), 14.0, look.text));
    if let Some(note) = &form.candidates_note {
        parts.push(text(note.clone(), 13.0, look.muted));
    }
    parts.extend(
        form.candidates
            .iter()
            .map(|person| person_row(person, look)),
    );
    let leave = Button::new(
        ids::of(form.channel, "leave"),
        t("Abandonar grupo"),
        look.menu_palette(),
    )
    .variant(ButtonVariant::Destructive)
    .size(ControlSize::Spacious)
    .enabled(form.leave_denied.is_none())
    .render();
    let mut footer = vec![leave];
    if let Some(reason) = &form.leave_denied {
        footer.push(
            text(reason.clone(), 12.0, look.muted)
                .flex(1.0)
                .centered_y(),
        );
    } else {
        footer.push(Element::spacer().flex(1.0));
    }
    footer.push(
        Button::new(ids::CLOSE, t("Cerrar"), look.menu_palette())
            .variant(ButtonVariant::Secondary)
            .size(ControlSize::Spacious)
            .render(),
    );
    Modal::new(
        root,
        true,
        t("Gestionar grupo"),
        form.title.clone(),
        ids::CLOSE,
        palette,
    )
    .width(WIDTH)
    .body(Element::column(parts).gap(8.0))
    .footer(Element::row(footer).gap(9.0))
    .render()
}

fn person_row(person: &GroupPerson, look: &Look) -> Element {
    let mut names = vec![strong_line(&person.name, 14.0, look.text).shrink_to_fit()];
    if person.owner {
        names.push(
            Element::icon(Icon::Custom(crate::members::CROWN))
                .foreground(look.accent)
                .size(Some(13.0), Some(13.0))
                .accessible(limn::Access::default().description(t("Dueño del grupo")))
                .centered_y(),
        );
    }
    let mut parts = vec![
        avatar_with_image(&person.name, person.accent, 32.0, person.image.clone()).centered_y(),
        Element::column(vec![
            Element::row(names).gap(4.0),
            line(&person.handle, 12.0, look.muted),
        ])
        .flex(1.0)
        .centered_y(),
    ];
    for (id, label, denied) in &person.actions {
        let button = Button::new(id.clone(), label.clone(), look.menu_palette())
            .variant(ButtonVariant::Secondary)
            .size(ControlSize::Compact)
            .enabled(denied.is_none());
        let button = match denied {
            Some(reason) => limn::Tooltip::new(
                button.render(),
                format!("{id}:hint"),
                reason.clone(),
                look.menu_palette(),
            )
            .render(),
            None => button.render(),
        };
        parts.push(button.centered_y());
    }
    Element::row(parts).gap(10.0).size(None, Some(44.0))
}

/// The dialog that makes a group from chosen friends, over `root`.
pub fn create_dialog(root: Element, form: &NewGroupForm, look: &Look) -> Element {
    let palette = look.menu_palette();
    let mut parts = vec![
        text(
            tr!(
                "Elegí a quiénes sumar: hasta {most} amigos. Con uno solo se abre su conversación.",
                most = form.most
            ),
            13.0,
            look.muted,
        ),
        SearchField::new(
            ids::NEW_QUERY,
            form.query.clone(),
            t("Buscar amigos"),
            palette,
        )
        .render(),
    ];
    if let Some(note) = &form.note {
        parts.push(text(note.clone(), 13.0, look.muted));
    }
    for friend in &form.friends {
        let full = !friend.chosen && form.chosen >= form.most;
        parts.push(
            Element::row(vec![
                avatar_with_image(&friend.name, friend.accent, 32.0, friend.image.clone())
                    .centered_y(),
                Element::column(vec![
                    strong_line(&friend.name, 14.0, look.text).shrink_to_fit(),
                    line(&friend.handle, 12.0, look.muted),
                ])
                .flex(1.0)
                .centered_y(),
                Checkbox::new(
                    format!("{}{}", ids::NEW_PICK, friend.user),
                    friend.chosen,
                    palette,
                )
                .enabled(!full)
                .accessibility_label(tr!("Sumar a {name}", name = friend.name))
                .render()
                .centered_y(),
            ])
            .gap(10.0)
            .size(None, Some(44.0)),
        );
    }
    let mut footer = vec![text(
        tr!("{chosen} de {most}", chosen = form.chosen, most = form.most),
        13.0,
        look.muted,
    )
    .centered_y()];
    if let Some(reason) = &form.denied {
        footer.push(
            text(reason.clone(), 12.0, look.muted)
                .flex(1.0)
                .centered_y(),
        );
    } else {
        footer.push(Element::spacer().flex(1.0));
    }
    footer.push(
        Button::new(ids::NEW_CLOSE, t("Cancelar"), palette)
            .variant(ButtonVariant::Secondary)
            .size(ControlSize::Spacious)
            .render(),
    );
    footer.push(
        Button::new(
            ids::NEW_CREATE,
            if form.chosen == 1 {
                t("Abrir conversación")
            } else {
                t("Crear grupo")
            },
            look.accent_palette(),
        )
        .size(ControlSize::Spacious)
        .enabled(form.chosen > 0 && form.denied.is_none())
        .render(),
    );
    Modal::new(
        root,
        true,
        t("Crear grupo"),
        t("Una conversación con varios amigos a la vez."),
        ids::NEW_CLOSE,
        palette,
    )
    .width(WIDTH)
    .body(Element::column(parts).gap(8.0))
    .footer(Element::row(footer).gap(9.0))
    .render()
}

/// The question before removing someone or leaving, over `root`.
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
    .width(crate::friends::CONFIRM_WIDTH)
    .confirm_label(confirm.to_owned())
    .destructive(look.danger)
    .render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    fn person(name: &str, owner: bool, action: Option<(&str, Option<&str>)>) -> GroupPerson {
        GroupPerson {
            name: name.into(),
            handle: name.to_lowercase(),
            image: None,
            accent: Color::rgb(90, 120, 200),
            owner,
            actions: action
                .map(|(id, denied)| {
                    (
                        id.to_owned(),
                        "Quitar".to_owned(),
                        denied.map(str::to_owned),
                    )
                })
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn the_dialog_offers_only_what_the_account_may_do() {
        let form = GroupForm {
            channel: 21,
            title: "Proyecto".into(),
            name: "Proyecto".into(),
            rename_denied: None,
            can_save: true,
            members: vec![
                person("Ada", true, None),
                person(
                    "Bruno",
                    false,
                    Some(("group:21:remove:3", Some("Solo quien es dueño"))),
                ),
                GroupPerson {
                    actions: vec![
                        ("group:21:remove:4".into(), "Quitar".into(), None),
                        ("group:21:owner:4".into(), "Hacer dueño".into(), None),
                    ],
                    ..person("Carla", false, None)
                },
            ],
            candidates: vec![person("Zoe", false, Some(("group:21:add:35", None)))],
            candidates_note: None,
            leave_denied: None,
            icon: None,
            accent: Color::rgb(90, 120, 200),
            has_icon: false,
            icon_denied: None,
            icon_note: Some("Simulación local: la imagen no se dibuja.".into()),
        };
        let (_, laid) = frame(
            dialog(Element::spacer(), &form, &Look::DARK),
            Size::new(1000.0, 800.0),
        );
        let found = controls(&laid);
        for id in [
            "group:21:icon",
            "group:21:name",
            "group:21:rename",
            "group:21:add:35",
            "group:21:remove:4",
            "group:21:owner:4",
            "group:21:leave",
            ids::CLOSE,
        ] {
            assert!(found.contains(&id.to_owned()), "{id}: {found:?}");
        }
        assert!(!found.contains(&"group:21:remove:3".to_owned()));
        // Nothing to take away without a picture.
        assert!(!found.contains(&"group:21:icon:remove".to_owned()));
        let painted = texts(&laid);
        assert!(
            painted.contains(&"Integrantes — 3".to_owned()),
            "{painted:?}"
        );
        let (_, frame) = frame(
            confirm(
                Element::spacer(),
                "Abandonar «Proyecto»",
                "Dejás de verlo.",
                "Abandonar",
                &Look::DARK,
            ),
            Size::new(900.0, 600.0),
        );
        assert!(controls(&frame).contains(&ids::CONFIRM.to_owned()));
    }

    #[test]
    fn the_new_group_dialog_stops_choosing_at_its_limit() {
        let choice = |user: u64, chosen: bool| Choice {
            user,
            name: format!("Amigo {user}"),
            handle: format!("amigo{user}"),
            image: None,
            accent: Color::rgb(90, 120, 200),
            chosen,
        };
        let form = NewGroupForm {
            query: String::new(),
            friends: vec![choice(10, true), choice(11, true), choice(12, false)],
            chosen: 2,
            most: 2,
            note: None,
            denied: None,
        };
        let (_, laid) = frame(
            create_dialog(Element::spacer(), &form, &Look::DARK),
            Size::new(1000.0, 800.0),
        );
        let found = controls(&laid);
        for id in ["group:new:pick:10", "group:new:pick:11", ids::NEW_CREATE] {
            assert!(found.contains(&id.to_owned()), "{id}: {found:?}");
        }
        // Full: a third friend cannot be chosen.
        assert!(!found.contains(&"group:new:pick:12".to_owned()));
        assert!(texts(&laid).contains(&"2 de 2".to_owned()));
    }
}
