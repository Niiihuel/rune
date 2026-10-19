//! The folders of servers on the rail: the dialog that names and colors
//! one. The rail draws them ([`crate::rail::RailFolder`]); their menus are
//! the channel menus' ([`crate::channel_menu::context_menu`]).

use limn::{Access, Component, ControlSize, Element, Modal, Role, TextField};
use rune_i18n::{es, t, tr};
use rune_model::{account::FOLDER_NAME_MAX, Rgb};

use crate::server_menu::{buttons, label, SERVER_COLORS};
use crate::{text, Look};

/// The actions these pieces report.
pub mod ids {
    pub const NAME: &str = "fd:name";
    pub const SAVE: &str = "fd:save";
    pub const CLOSE: &str = "fd:close";
    /// Followed by the color as six hex digits.
    pub const COLOR: &str = "fd:color=";
}

/// The colors a folder can have, the service's own first; their names are
/// Spanish, shown through `t`.
pub const FOLDER_COLORS: [(Rgb, &str); 9] = [
    (Rgb(0x58_65_F2), es("Azul")),
    SERVER_COLORS[0],
    SERVER_COLORS[1],
    SERVER_COLORS[2],
    SERVER_COLORS[3],
    SERVER_COLORS[4],
    SERVER_COLORS[5],
    SERVER_COLORS[6],
    SERVER_COLORS[7],
];

/// A folder being named and colored.
#[derive(Clone, Debug, PartialEq)]
pub struct FolderForm {
    /// What is written in the field.
    pub name: String,
    /// What it is called while the field is empty: its servers' names.
    pub placeholder: String,
    pub color: Rgb,
    /// Why it cannot be changed now, when it cannot.
    pub denied: Option<String>,
}

/// The dialog that names and colors a folder, over `root`.
pub fn folder_dialog(root: Element, form: &FolderForm, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let swatches = FOLDER_COLORS
        .iter()
        .map(|(color, name)| {
            let chosen = *color == form.color;
            Element::clickable(format!("{}{:06X}", ids::COLOR, color.0), Element::spacer())
                .size(Some(28.0), Some(28.0))
                .radius(14.0)
                .background(look.ink(*color))
                .border(if chosen { look.text } else { look.border }, 2.0)
                .accessible(
                    Access::new(Role::RadioButton)
                        .name(t(name))
                        .selected(chosen),
                )
        })
        .collect();
    let field = TextField::new(
        ids::NAME,
        form.name.clone(),
        form.placeholder.clone(),
        palette,
    )
    .accessibility_label(t("Nombre de la carpeta"))
    .size(ControlSize::Spacious);
    let count = form.name.trim().chars().count();
    let note = match &form.denied {
        Some(reason) => text(reason.clone(), 12.0, look.danger),
        None if count > FOLDER_NAME_MAX => text(
            tr!(
                "Tiene {count} caracteres; el máximo es {max}.",
                count,
                max = FOLDER_NAME_MAX
            ),
            12.0,
            look.danger,
        ),
        None => text(
            t("Vacío, se llama como sus servidores. Se guarda en tu cuenta."),
            12.0,
            look.muted,
        ),
    };
    let can_save = form.denied.is_none() && count <= FOLDER_NAME_MAX;
    Modal::new(root, true, t("Ajustes de carpeta"), "", ids::CLOSE, palette)
        .width(width)
        .body(
            Element::column(vec![
                label(t("Nombre de la carpeta"), look),
                if form.denied.is_some() {
                    field.disabled()
                } else {
                    field
                }
                .render(),
                note,
                label(t("Color de la carpeta"), look),
                Element::row(swatches).gap(8.0),
            ])
            .gap(8.0),
        )
        .footer(buttons(
            (t("Cancelar"), ids::CLOSE),
            (t("Guardar"), ids::SAVE, can_save),
            look,
        ))
        .initial_focus(ids::NAME)
        .render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    #[test]
    fn the_dialog_names_and_colors_a_folder_and_says_what_is_too_long() {
        let form = FolderForm {
            name: "Juegos".into(),
            placeholder: "Café, Club".into(),
            color: FOLDER_COLORS[0].0,
            denied: None,
        };
        let (_, shown) = frame(
            folder_dialog(Element::spacer(), &form, 440.0, &Look::DARK),
            Size::new(900.0, 700.0),
        );
        let ids = controls(&shown);
        assert!(ids.contains(&ids::SAVE.to_owned()) && ids.contains(&ids::NAME.to_owned()));
        assert_eq!(
            ids.iter().filter(|id| id.starts_with(ids::COLOR)).count(),
            FOLDER_COLORS.len()
        );
        let long = FolderForm {
            name: "x".repeat(FOLDER_NAME_MAX + 1),
            ..form
        };
        let (_, shown) = frame(
            folder_dialog(Element::spacer(), &long, 440.0, &Look::DARK),
            Size::new(900.0, 700.0),
        );
        assert!(texts(&shown)
            .iter()
            .any(|text| text.contains("el máximo es 100")));
    }
}
