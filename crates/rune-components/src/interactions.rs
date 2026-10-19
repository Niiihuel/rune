//! Apps' forms (P3): the modal an app opens, to fill and send, and the
//! options of a command being run, over the composer. Pure views: the state
//! of what is written lives in `rune-ui`.

use std::collections::HashMap;

use limn::{
    Button, ButtonVariant, Checkbox, Component, ControlSize, DropdownOption, Element, FontFamily,
    Icon, IconButton, Modal, PopoverPlacement, RadioGroup, RadioOption, SearchField, Select,
    SelectVariant, TextField,
};
use rune_i18n::{t, tr};
use rune_model::components::{MenuKind, Select as Menu};
use rune_model::interactions::{FieldValue, Modal as AppModal, ModalInput, OptionKind};

use crate::message::{menu_label, OpenMenu};
use crate::text::Names;
use crate::{gap, icon, line, strong_line, text, Look};

/// The actions of a modal, by the `custom_id` of the field they belong to.
pub mod modal_ids {
    pub const CLOSE: &str = "modal:close";
    pub const SEND: &str = "modal:send";

    /// A text field.
    pub fn text(custom_id: &str) -> String {
        format!("modal:text:{custom_id}")
    }

    /// A menu opened or closed.
    pub fn menu(custom_id: &str) -> String {
        format!("modal:menu:{custom_id}")
    }

    /// The `index`th option of a menu, a choice or a box among several.
    pub fn pick(index: usize, custom_id: &str) -> String {
        format!("modal:pick:{index}:{custom_id}")
    }

    pub fn menu_close(custom_id: &str) -> String {
        format!("modal:menu-close:{custom_id}")
    }

    /// What a menu of people, roles or channels searches.
    pub fn query(custom_id: &str) -> String {
        format!("modal:query:{custom_id}")
    }

    /// A box on its own.
    pub fn check(custom_id: &str) -> String {
        format!("modal:check:{custom_id}")
    }

    /// Choose files for a file field.
    pub fn files(custom_id: &str) -> String {
        format!("modal:files:{custom_id}")
    }

    /// Take the file `token` out of the field `custom_id`.
    pub fn unfile(token: u64, custom_id: &str) -> String {
        format!("modal:unfile:{token}:{custom_id}")
    }
}

/// A modal as it is being filled.
pub struct ModalView<'a> {
    pub modal: &'a AppModal,
    /// Who opened it.
    pub app: &'a str,
    /// What each field holds, by `custom_id`.
    pub values: &'a HashMap<String, FieldValue>,
    /// It was sent once: the fields say what is missing.
    pub tried: bool,
    /// The menu open, with what it offers.
    pub menu: Option<&'a OpenMenu>,
    /// It is on its way to the app.
    pub sending: bool,
    /// Why it did not go, when it did not.
    pub problem: Option<&'a str>,
    pub names: Option<&'a Names>,
    // Files (comandos-de-apps).
    /// The files chosen for its file fields, which name them by token.
    pub files: &'a [rune_model::PendingAttachment],
    /// The file field whose files are being chosen, while that runs.
    pub picking: Option<&'a str>,
    /// Why files cannot be attached, when they cannot.
    pub files_denied: Option<&'a str>,
}

/// The modal an app opened, over `root`, `width` wide.
pub fn modal_dialog(root: Element, view: &ModalView<'_>, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let inner = Modal::body_width(width) - 2.0;
    let mut body = vec![line(
        tr!("{app} · aplicación", app = view.app),
        12.0,
        look.muted,
    )];
    let mut first = None;
    let mut sendable = !view.sending;
    for field in &view.modal.fields {
        let custom_id = field.input.custom_id().unwrap_or_default();
        let value = view
            .values
            .get(custom_id)
            .cloned()
            .unwrap_or_else(|| field.input.starting());
        let problem = field.input.problem(&value);
        sendable &= problem.is_none();
        let control = input(&field.input, &value, view, inner, look);
        if first.is_none() {
            if let ModalInput::TextInput { custom_id, .. } = &field.input {
                first = Some(modal_ids::text(custom_id));
            }
        }
        let mut rows = Vec::new();
        if let Some(label) = &field.label {
            let required = match &field.input {
                ModalInput::TextInput { required, .. } | ModalInput::Radio { required, .. } => {
                    *required
                }
                ModalInput::Menu(menu) => menu.required,
                ModalInput::Files { required, min, .. } => *required && *min > 0,
                _ => false,
            };
            rows.push(Element::row(vec![
                strong_line(label.to_uppercase(), 12.0, look.muted).shrink_to_fit(),
                if required {
                    line(" *", 12.0, look.danger)
                } else {
                    Element::spacer().size(Some(0.0), None)
                },
            ]));
        }
        if let Some(description) = &field.description {
            rows.push(text(description.clone(), 12.0, look.muted).size(Some(inner), None));
        }
        rows.push(control);
        if let Some(problem) = problem.filter(|_| view.tried) {
            rows.push(line(problem, 12.0, look.danger));
        }
        body.push(Element::column(rows).gap(6.0));
    }
    if let Some(problem) = view.problem {
        body.push(
            Element::row(vec![
                icon(Icon::WarningTriangle, look.danger).centered_y(),
                text(problem.to_owned(), 12.0, look.danger).flex(1.0),
            ])
            .gap(6.0),
        );
    }
    let send = if view.sending {
        t("Enviando…")
    } else {
        t("Enviar")
    };
    // Files on their way still reach the app: they cannot be called back.
    let uploading = view.sending && !view.files.is_empty();
    let footer = Element::row(vec![
        Button::new(modal_ids::CLOSE, t("Cancelar"), palette)
            .variant(ButtonVariant::Secondary)
            .size(ControlSize::Spacious)
            .enabled(!uploading)
            .render()
            .flex(1.0),
        Button::new(modal_ids::SEND, send, look.accent_palette())
            .size(ControlSize::Spacious)
            .enabled(sendable || !view.tried)
            .loading(view.sending)
            .render()
            .flex(1.0),
    ])
    .gap(9.0);
    let mut dialog = Modal::new(
        root,
        true,
        view.modal.title.clone(),
        "",
        modal_ids::CLOSE,
        palette,
    )
    .width(width)
    .body(Element::column(body).gap(16.0))
    .footer(footer);
    if let Some(first) = first {
        dialog = dialog.initial_focus(first);
    }
    dialog.render()
}

/// The control of one field.
fn input(
    input: &ModalInput,
    value: &FieldValue,
    view: &ModalView<'_>,
    width: f32,
    look: &Look,
) -> Element {
    let palette = look.menu_palette();
    match (input, value) {
        (ModalInput::Text(said), _) => text(said.clone(), 14.0, look.text).size(Some(width), None),
        (
            ModalInput::TextInput {
                custom_id,
                long,
                placeholder,
                max_length,
                ..
            },
            FieldValue::Text(written),
        ) => {
            let placeholder = placeholder.clone().unwrap_or_default();
            let mut rows = Vec::new();
            if *long {
                rows.push(
                    Element::panel(
                        Element::text_area(modal_ids::text(custom_id), written, placeholder)
                            .text_area_auto_grow(88.0, 220.0)
                            .foreground(look.text)
                            .font_family(FontFamily::Sans)
                            .font_size(14.0)
                            .padding(10.0),
                    )
                    .background(look.canvas)
                    .border(look.border, 1.0)
                    .radius(4.0)
                    .size(Some(width), None),
                );
            } else {
                rows.push(
                    TextField::new(
                        modal_ids::text(custom_id),
                        written.clone(),
                        placeholder,
                        palette,
                    )
                    .width(width)
                    .render(),
                );
            }
            if let Some(max) = max_length {
                rows.push(line(
                    format!("{}/{max}", written.chars().count()),
                    11.0,
                    look.faint,
                ));
            }
            Element::column(rows).gap(4.0)
        }
        (ModalInput::Menu(menu), FieldValue::Values(picked)) => {
            menu_select(menu, picked, view, width, look)
        }
        (
            ModalInput::Radio {
                custom_id, options, ..
            },
            FieldValue::Values(picked),
        ) => {
            let chosen = options
                .iter()
                .position(|option| picked.contains(&option.value))
                .map(|index| modal_ids::pick(index, custom_id));
            RadioGroup::new(
                format!("modal:radio:{custom_id}"),
                options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| {
                        let radio = RadioOption::new(
                            modal_ids::pick(index, custom_id),
                            option.label.clone(),
                        );
                        match &option.description {
                            Some(description) => radio.description(description.clone()),
                            None => radio,
                        }
                    })
                    .collect(),
                chosen.as_deref(),
                palette,
            )
            .render()
        }
        (
            ModalInput::Checks {
                custom_id, options, ..
            },
            FieldValue::Values(picked),
        ) => Element::column(
            options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    Checkbox::new(
                        modal_ids::pick(index, custom_id),
                        picked.contains(&option.value),
                        palette,
                    )
                    .label(option.label.clone())
                    .render()
                })
                .collect(),
        )
        .gap(6.0),
        (ModalInput::Check { custom_id, .. }, FieldValue::Check(on)) => {
            Checkbox::new(modal_ids::check(custom_id), *on, palette)
                .label(t("Sí"))
                .render()
        }
        (
            ModalInput::Files {
                custom_id,
                max,
                file_types,
                ..
            },
            FieldValue::Files(tokens),
        ) => files_field(custom_id, *max, file_types, tokens, view, width, look),
        (ModalInput::Unsupported(kind), _) => line(
            tr!("RUNE todavía no completa este campo (tipo {kind})", kind),
            12.0,
            look.faint,
        ),
        _ => line(t("Este campo no se puede mostrar"), 12.0, look.faint),
    }
}

/// A file field: the files chosen, each with its cross, and the button
/// that chooses more, up to `max`, of the kinds `file_types` lists, if any.
fn files_field(
    custom_id: &str,
    max: u8,
    file_types: &[String],
    tokens: &[rune_model::UploadToken],
    view: &ModalView<'_>,
    width: f32,
    look: &Look,
) -> Element {
    let mut rows: Vec<Element> = tokens
        .iter()
        .filter_map(|token| view.files.iter().find(|file| file.token == *token))
        .map(|file| {
            let remove = IconButton::new(
                modal_ids::unfile(file.token.0, custom_id),
                Icon::Close,
                look.palette(),
            )
            .variant(ButtonVariant::Ghost)
            .tooltip(tr!("Quitar {name}", name = file.filename))
            .enabled(!view.sending)
            .render();
            Element::row(vec![
                crate::uploads::file_info(file, (width - 40.0).max(0.0), look).flex(1.0),
                remove.centered_y(),
            ])
            .gap(8.0)
        })
        .collect();
    let picking = view.picking == Some(custom_id);
    let choose = Button::new(
        modal_ids::files(custom_id),
        if picking {
            t("Preparando archivos…")
        } else {
            t("Elegir archivos")
        },
        look.menu_palette(),
    )
    .variant(ButtonVariant::Secondary)
    .size(ControlSize::Compact)
    .icon(Icon::Paperclip)
    .loading(picking)
    .enabled(
        !view.sending
            && view.picking.is_none()
            && view.files_denied.is_none()
            && tokens.len() < usize::from(max),
    )
    .render();
    let said = match view.files_denied {
        Some(reason) => reason.to_owned(),
        None if max <= 1 => t("Un archivo").to_owned(),
        None => tr!("Hasta {max} archivos", max),
    };
    rows.push(
        Element::row(vec![
            choose.centered_y(),
            text(said, 12.0, look.muted).flex(1.0).centered_y(),
        ])
        .gap(10.0),
    );
    if !file_types.is_empty() {
        let kinds = file_types
            .iter()
            .map(|kind| match kind.to_ascii_lowercase().as_str() {
                "image" => t("imágenes").to_owned(),
                "video" => t("videos").to_owned(),
                "audio" => t("audio").to_owned(),
                _ => kind.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        rows.push(text(
            tr!("Tipos permitidos: {kinds}", kinds),
            12.0,
            look.muted,
        ));
    }
    Element::column(rows).gap(6.0).size(Some(width), None)
}

/// A modal's menu: like a message's, as a field.
fn menu_select(
    menu: &Menu,
    picked: &[String],
    view: &ModalView<'_>,
    width: f32,
    look: &Look,
) -> Element {
    let custom_id = menu.custom_id.as_deref().unwrap_or_default();
    let open = view.menu.filter(|open| open.custom_id == custom_id);
    let label = menu_label(menu, picked, open, view.names);
    let options: Vec<DropdownOption> = match menu.kind {
        MenuKind::Text => menu
            .options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                DropdownOption::new(modal_ids::pick(index, custom_id), option.label.clone())
                    .selected(picked.contains(&option.value))
            })
            .collect(),
        _ => open
            .map(|open| {
                open.options
                    .iter()
                    .enumerate()
                    .map(|(index, (value, shown))| {
                        DropdownOption::new(modal_ids::pick(index, custom_id), shown.clone())
                            .selected(picked.contains(value))
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    let mut select = Select::new(
        modal_ids::menu(custom_id),
        label,
        options,
        open.is_some(),
        modal_ids::menu_close(custom_id),
        look.menu_palette(),
    )
    .variant(SelectVariant::Field)
    .placement(PopoverPlacement::BottomStart)
    .width(width)
    .menu_width(width)
    .enabled(!menu.disabled && !view.sending);
    if let Some(open) = open.filter(|_| menu.kind != MenuKind::Text) {
        select = select.header(
            SearchField::new(
                modal_ids::query(custom_id),
                open.query.clone(),
                t("Buscar"),
                look.menu_palette(),
            )
            .width(width - 16.0)
            .render()
            .padding(8.0),
        );
    }
    select.render()
}

/// The actions of the options of a command being run.
pub mod command_ids {
    /// Leave the command.
    pub const CANCEL: &str = "cmd:cancel";
    pub const SEND: &str = "cmd:send";

    /// What is written for the option `name`.
    pub fn option(name: &str) -> String {
        format!("cmd:opt:{name}")
    }

    /// The option's choices or people, opened or closed.
    pub fn menu(name: &str) -> String {
        format!("cmd:menu:{name}")
    }

    pub fn menu_close(name: &str) -> String {
        format!("cmd:menu-close:{name}")
    }

    /// The `index`th choice of the option's menu, or the app's suggestion.
    pub fn pick(index: usize, name: &str) -> String {
        format!("cmd:pick:{index}:{name}")
    }

    /// What the option's menu of people, roles or channels searches.
    pub fn query(name: &str) -> String {
        format!("cmd:query:{name}")
    }
}

/// One option of a command being run.
pub struct OptionField<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub kind: OptionKind,
    pub required: bool,
    /// What is written, or picked, as its label.
    pub written: &'a str,
    /// What is picked, read for the person, for a menu.
    pub shown: Option<String>,
    /// Why it cannot go, once the command was tried.
    pub problem: Option<String>,
    /// It is picked from a menu (choices, people, a yes or a no).
    pub menu: bool,
    /// Its menu open: what it offers, value and label.
    pub open: Option<&'a [(String, String)]>,
    /// What its menu searches.
    pub query: &'a str,
    /// The app's suggestions for it, while it is written.
    pub suggestions: &'a [String],
    /// Why RUNE does not fill it, when it does not.
    pub unsupported: Option<&'a str>,
}

/// A command being run, over the composer.
pub struct CommandForm<'a> {
    /// `/name sub`, and its app.
    pub name: &'a str,
    pub app: &'a str,
    pub description: &'a str,
    pub fields: Vec<OptionField<'a>>,
    /// It is on its way to the app, or the app is thinking.
    pub waiting: Option<&'a str>,
    /// Why it did not go or failed.
    pub problem: Option<&'a str>,
}

/// The options of a command being run, `width` wide, over the composer:
/// Enter in the composer or in a field sends it.
pub fn command_form(form: &CommandForm<'_>, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let head = Element::row(vec![
        icon(Icon::Code, look.accent).centered_y(),
        strong_line(form.name.to_owned(), 14.0, look.text)
            .shrink_to_fit()
            .centered_y(),
        line(format!("· {}", form.app), 12.0, look.muted)
            .flex(1.0)
            .centered_y(),
        IconButton::new(command_ids::CANCEL, Icon::CloseCircle, palette)
            .variant(ButtonVariant::Ghost)
            .tooltip(t("Salir del comando"))
            .tooltip_shortcut("Esc")
            .render()
            .centered_y(),
    ])
    .gap(6.0);
    let mut rows = vec![head];
    if !form.description.is_empty() {
        rows.push(line(form.description.to_owned(), 12.0, look.muted));
    }
    let field_width = (width - 24.0).max(120.0);
    for field in &form.fields {
        rows.push(option_field(field, field_width, look));
    }
    if form.fields.is_empty() {
        rows.push(line(
            t("Sin opciones: presioná Enter para enviarlo."),
            12.0,
            look.faint,
        ));
    }
    if let Some(waiting) = form.waiting {
        rows.push(
            Element::row(vec![
                Element::spinner("cmd:waiting", look.muted, limn::SpinnerVariant::Circle)
                    .size(Some(14.0), Some(14.0))
                    .centered_y(),
                line(waiting.to_owned(), 12.0, look.muted).centered_y(),
            ])
            .gap(6.0),
        );
    } else if let Some(problem) = form.problem {
        rows.push(
            Element::row(vec![
                icon(Icon::WarningTriangle, look.danger).centered_y(),
                text(problem.to_owned(), 12.0, look.danger).flex(1.0),
            ])
            .gap(6.0),
        );
    }
    rows.push(Element::row(vec![
        Element::spacer().flex(1.0),
        Button::new(
            command_ids::SEND,
            t("Enviar comando"),
            look.accent_palette(),
        )
        .icon(Icon::Send)
        .enabled(form.waiting.is_none())
        .render(),
    ]));
    Element::panel(Element::column(rows).gap(8.0))
        .padding(12.0)
        .background(look.menu)
        .border(look.border, 1.0)
        .radius(8.0)
        .size(Some(width), None)
}

/// One option: its name, and the field or menu that fills it.
fn option_field(field: &OptionField<'_>, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let mut title = vec![strong_line(field.name.to_owned(), 13.0, look.text).shrink_to_fit()];
    if field.required {
        title.push(line(" *", 13.0, look.danger));
    }
    title.push(gap(8.0, 0.0));
    title.push(line(field.description.to_owned(), 12.0, look.muted).flex(1.0));
    let control = if let Some(reason) = field.unsupported {
        line(reason.to_owned(), 12.0, look.faint)
    } else if field.menu {
        let options: Vec<DropdownOption> = field
            .open
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(index, (value, label))| {
                DropdownOption::new(command_ids::pick(index, field.name), label.clone())
                    .selected(value == field.written)
            })
            .collect();
        let mut select = Select::new(
            command_ids::menu(field.name),
            field.shown.clone().unwrap_or_else(|| t("Elegí…").into()),
            options,
            field.open.is_some(),
            command_ids::menu_close(field.name),
            look.palette(),
        )
        .variant(SelectVariant::Field)
        .menu_palette(look.menu_palette())
        .placement(PopoverPlacement::TopStart)
        .width(width)
        .menu_width(width.min(360.0))
        .accessibility_label(field.name.to_owned());
        if field.open.is_some()
            && matches!(
                field.kind,
                OptionKind::User | OptionKind::Role | OptionKind::Channel | OptionKind::Mentionable
            )
        {
            select = select.header(
                SearchField::new(
                    command_ids::query(field.name),
                    field.query.to_owned(),
                    t("Buscar"),
                    look.menu_palette(),
                )
                .width(width.min(360.0) - 16.0)
                .render()
                .padding(8.0),
            );
        }
        select.render()
    } else {
        let placeholder = match field.kind {
            OptionKind::Integer => t("Un número entero"),
            OptionKind::Number => t("Un número"),
            _ => "",
        };
        TextField::new(
            command_ids::option(field.name),
            field.written.to_owned(),
            placeholder,
            palette,
        )
        .width(width)
        .accessibility_label(field.name.to_owned())
        .render()
    };
    let mut rows = vec![Element::row(title), control];
    if !field.suggestions.is_empty() {
        rows.push(
            Element::row(
                field
                    .suggestions
                    .iter()
                    .enumerate()
                    .map(|(index, said)| {
                        Button::new(command_ids::pick(index, field.name), said.clone(), palette)
                            .variant(ButtonVariant::Outline)
                            .size(ControlSize::Compact)
                            .render()
                    })
                    .collect(),
            )
            .gap(6.0)
            .wrap(),
        );
    }
    if let Some(problem) = &field.problem {
        rows.push(line(problem.clone(), 12.0, look.danger));
    }
    Element::column(rows).gap(4.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;
    use rune_model::components::MenuOption;
    use rune_model::interactions::ModalField;
    use rune_model::UserId;

    fn modal() -> AppModal {
        AppModal {
            id: 1,
            app: UserId(9),
            custom_id: "f".into(),
            title: "Inscripción".into(),
            fields: vec![
                ModalField {
                    label: Some("Nombre".into()),
                    description: None,
                    input: ModalInput::TextInput {
                        custom_id: "nombre".into(),
                        long: false,
                        required: true,
                        min_length: None,
                        max_length: Some(40),
                        value: None,
                        placeholder: None,
                    },
                },
                ModalField {
                    label: Some("Nivel".into()),
                    description: Some("Elegí uno".into()),
                    input: ModalInput::Radio {
                        custom_id: "nivel".into(),
                        required: false,
                        options: vec![MenuOption {
                            label: "Empiezo".into(),
                            value: "a".into(),
                            ..MenuOption::default()
                        }],
                    },
                },
                ModalField {
                    label: Some("Fotos".into()),
                    description: None,
                    input: ModalInput::Files {
                        custom_id: "fotos".into(),
                        required: true,
                        min: 1,
                        max: 2,
                        file_types: vec!["image".into(), ".pdf".into()],
                    },
                },
                ModalField {
                    label: None,
                    description: None,
                    input: ModalInput::Unsupported(24),
                },
            ],
        }
    }

    #[test]
    fn a_modal_shows_its_fields_and_says_what_is_missing_once_tried() {
        let modal = modal();
        let mut values = HashMap::new();
        values.insert(
            "fotos".to_owned(),
            FieldValue::Files(vec![rune_model::UploadToken(3)]),
        );
        let files = [rune_model::PendingAttachment {
            token: rune_model::UploadToken(3),
            filename: "mate.png".into(),
            size_bytes: 2048,
        }];
        let view = |tried| ModalView {
            modal: &modal,
            app: "RuneBot",
            values: &values,
            tried,
            menu: None,
            sending: false,
            problem: None,
            names: None,
            files: &files,
            picking: None,
            files_denied: None,
        };
        let (_, shown) = frame(
            modal_dialog(Element::spacer(), &view(false), 480.0, &Look::DARK),
            Size::new(900.0, 800.0),
        );
        let ids = controls(&shown);
        for id in [
            modal_ids::text("nombre"),
            modal_ids::pick(0, "nivel"),
            modal_ids::files("fotos"),
            modal_ids::unfile(3, "fotos"),
            modal_ids::SEND.into(),
            modal_ids::CLOSE.into(),
        ] {
            assert!(ids.contains(&id), "{id} in {ids:?}");
        }
        let painted = texts(&shown).join(" ");
        assert!(
            painted.contains("Inscripción")
                && painted.contains("tipo 24")
                && painted.contains("mate.png")
                && painted.contains("Hasta 2 archivos")
                && painted.contains("Tipos permitidos: imágenes, .pdf"),
            "{painted}"
        );
        let (_, tried) = frame(
            modal_dialog(Element::spacer(), &view(true), 480.0, &Look::DARK),
            Size::new(900.0, 800.0),
        );
        assert!(texts(&tried).join(" ").contains("Completá este campo"));
    }

    #[test]
    fn a_command_form_offers_its_fields_and_waits_while_it_goes() {
        let options = vec![("5".to_owned(), "5 minutos".to_owned())];
        let form = |waiting| CommandForm {
            name: "/recordatorio",
            app: "RuneBot",
            description: "Pedile un recordatorio",
            fields: vec![
                OptionField {
                    name: "texto",
                    description: "Qué",
                    kind: OptionKind::Text,
                    required: true,
                    written: "",
                    shown: None,
                    problem: Some("Completá este campo".into()),
                    menu: false,
                    open: None,
                    query: "",
                    suggestions: &[],
                    unsupported: None,
                },
                OptionField {
                    name: "en",
                    description: "Cuándo",
                    kind: OptionKind::Integer,
                    required: true,
                    written: "",
                    shown: None,
                    problem: None,
                    menu: true,
                    open: Some(&options),
                    query: "",
                    suggestions: &[],
                    unsupported: None,
                },
            ],
            waiting,
            problem: None,
        };
        let (_, shown) = frame(
            Element::column(vec![
                Element::spacer().flex(1.0),
                command_form(&form(None), 600.0, &Look::DARK),
            ]),
            Size::new(700.0, 700.0),
        );
        let ids = controls(&shown);
        for id in [
            command_ids::option("texto"),
            command_ids::pick(0, "en"),
            command_ids::SEND.into(),
            command_ids::CANCEL.into(),
        ] {
            assert!(ids.contains(&id), "{id} in {ids:?}");
        }
        assert!(texts(&shown).join(" ").contains("Completá este campo"));
        let (_, waiting) = frame(
            command_form(&form(Some("RuneBot está pensando…")), 600.0, &Look::DARK),
            Size::new(700.0, 700.0),
        );
        assert!(texts(&waiting).join(" ").contains("pensando"));
        assert!(!controls(&waiting).contains(&command_ids::SEND.to_owned()));
    }
}
