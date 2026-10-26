//! Threads from plain props: the list of a channel's threads (followed,
//! active and archived, with «Cargar más»), the dialog that starts a thread
//! or a forum post, the one that renames and tags one, and the question
//! before deleting one. Every control's ID starts with `th:`.

use limn::{
    AlertDialog, Button, ButtonVariant, Checkbox, Component, ControlSize, Element, Icon, Modal,
    TextArea, TextField, Toggle, ToggleVariant,
};

use rune_i18n::{t, tr};

use crate::{gap, icon, line, strong_line, text, Look};

/// The actions these pieces report.
pub mod ids {
    /// The list of threads: closing it.
    pub const CLOSE: &str = "th:close";
    pub const CONFIRM: &str = "th:confirm";
    pub const CONFIRM_CANCEL: &str = "th:confirm:cancel";
    /// The dialog that starts a thread or a post: its fields, choosing a tag
    /// (`th:new:tag:{tag}`), sending and closing.
    pub const NEW_NAME: &str = "th:new:name";
    pub const NEW_CONTENT: &str = "th:new:content";
    pub const NEW_PRIVATE: &str = "th:new:private";
    pub const NEW_TAG: &str = "th:new:tag:";
    pub const NEW_CREATE: &str = "th:new:create";
    pub const NEW_CLOSE: &str = "th:new:close";
    /// Choosing files for a post's first message.
    pub const NEW_ATTACH: &str = "th:new:attach";
    /// The dialog that renames and tags one.
    pub const EDIT_NAME: &str = "th:edit:name";
    pub const EDIT_TAG: &str = "th:edit:tag:";
    pub const EDIT_SAVE: &str = "th:edit:save";
    pub const EDIT_CLOSE: &str = "th:edit:close";

    /// Open the list of `channel`'s threads.
    pub fn list(channel: impl std::fmt::Display) -> String {
        format!("th:list:{channel}")
    }

    /// Open the dialog that starts a thread in `channel`, or a post in a forum.
    pub fn start(channel: impl std::fmt::Display) -> String {
        format!("th:start:{channel}")
    }

    /// Ask for the next page of `parent`'s archived threads.
    pub fn more(parent: impl std::fmt::Display) -> String {
        format!("th:more:{parent}")
    }

    /// `th:{thread}:{action}`, from a thread's menu: `follow`, `unfollow`,
    /// `read`, `link`, `edit`, `close`, `open`, `lock`, `unlock`, `delete`.
    pub fn of(thread: impl std::fmt::Display, action: &str) -> String {
        format!("th:{thread}:{action}")
    }
}

pub const WIDTH: f32 = 520.0;

/// A thread as a list shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadRow {
    pub thread: u64,
    pub name: String,
    /// How many messages and when the last one, as a person reads it.
    pub detail: String,
    pub private: bool,
    pub archived: bool,
    pub locked: bool,
    pub followed: bool,
    pub unread: bool,
}

/// The list of one channel's threads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadList {
    pub channel: u64,
    /// The channel's name, for the title.
    pub title: String,
    /// Why no thread can be started here, when none can.
    pub start_denied: Option<String>,
    pub followed: Vec<ThreadRow>,
    pub active: Vec<ThreadRow>,
    pub archived: Vec<ThreadRow>,
    /// The button that asks for more archived threads: its label, and why it
    /// cannot be used now (loading, none left, not offered).
    pub more: (String, Option<String>),
}

/// One tag a post may carry.
#[derive(Clone, Debug, PartialEq)]
pub struct TagChoice {
    pub tag: u64,
    /// Its emoji and name.
    pub label: String,
    pub chosen: bool,
    /// Only moderation may put it on.
    pub locked: bool,
}

/// What the dialog that starts a thread or a post shows.
#[derive(Clone, Debug, PartialEq)]
pub struct NewThreadForm {
    /// «Crear hilo» or «Nueva publicación».
    pub title: String,
    /// Where it goes: `#canal`, or the message it starts from.
    pub subtitle: String,
    pub name: String,
    /// A post's first message; `None` for a thread.
    pub content: Option<String>,
    /// Whether it is private, and why it cannot be when it cannot; `None`
    /// when the choice does not exist (a post, or from a message).
    pub private: Option<(bool, Option<String>)>,
    pub tags: Vec<TagChoice>,
    /// The forum asks for a tag at least.
    pub tag_required: bool,
    /// Why it cannot be sent now; waiting for the provider too.
    pub denied: Option<String>,
    /// A post's files; `None` for a thread.
    pub files: Option<PostFiles>,
}

/// The files of a post's first message.
#[derive(Clone, Debug, PartialEq)]
pub struct PostFiles {
    /// The forum, for the IDs of the list's controls.
    pub forum: u64,
    pub files: Vec<rune_model::PendingAttachment>,
    /// The choice being prepared, while it runs.
    pub picking: Option<u64>,
    /// The post was asked for: nothing changes until the answer.
    pub waiting: bool,
    /// Why files cannot be added, when they cannot.
    pub denied: Option<String>,
    /// While the post is asked for: the files' bytes handed over, of all.
    pub progress: Option<(u64, u64)>,
}

/// What the dialog that renames and tags a thread shows.
#[derive(Clone, Debug, PartialEq)]
pub struct EditThreadForm {
    pub title: String,
    pub name: String,
    /// A post's tags; empty for a thread of a channel.
    pub tags: Vec<TagChoice>,
    pub denied: Option<String>,
}

fn row(thread: &ThreadRow, look: &Look) -> Element {
    let ink = if thread.archived && !thread.unread {
        look.muted
    } else {
        look.text
    };
    let mut marks = Vec::new();
    for (on, label) in [
        (thread.followed, t("Seguido")),
        (thread.private, t("Privado")),
        // Not «Bloqueado»: that is a blocked person, «Blocked» in English.
        (thread.locked, t("Hilo bloqueado")),
        (thread.archived, t("Archivado")),
    ] {
        if on {
            marks.push(line(label, 12.0, look.faint));
        }
    }
    let mut title = vec![strong_line(&thread.name, 14.0, ink).shrink_to_fit()];
    if thread.unread {
        title.push(
            Element::panel(Element::spacer())
                .size(Some(8.0), Some(8.0))
                .radius(4.0)
                .background(look.text)
                .centered_y(),
        );
    }
    Element::clickable(
        format!("thread:{}", thread.thread),
        Element::row(vec![
            icon(
                if thread.private {
                    Icon::Lock
                } else {
                    Icon::Chat
                },
                look.faint,
            )
            .centered_y(),
            Element::column(vec![
                Element::row(title).gap(6.0),
                Element::row(
                    std::iter::once(line(&thread.detail, 12.0, look.muted))
                        .chain(marks)
                        .collect(),
                )
                .gap(8.0),
            ])
            .gap(2.0)
            .flex(1.0)
            .centered_y(),
        ])
        .gap(10.0)
        .padding(8.0),
    )
    .radius(6.0)
    .hover_background(look.hover)
    .accessible(limn::Access::new(limn::Role::ListItem).name(thread.name.clone()))
}

fn section(title: &str, rows: &[ThreadRow], empty: &str, look: &Look) -> Vec<Element> {
    let mut parts = vec![strong_line(
        format!("{title} — {}", rows.len()),
        13.0,
        look.muted,
    )];
    if rows.is_empty() {
        parts.push(text(empty, 13.0, look.faint));
    }
    parts.extend(rows.iter().map(|thread| row(thread, look)));
    parts
}

/// The list of a channel's threads, over `root`.
pub fn list(root: Element, view: &ThreadList, look: &Look) -> Element {
    let palette = look.menu_palette();
    let start = Button::new(
        ids::start(view.channel),
        t("Crear hilo"),
        look.accent_palette(),
    )
    .size(ControlSize::Compact)
    .enabled(view.start_denied.is_none())
    .render();
    let mut head = vec![start];
    if let Some(reason) = &view.start_denied {
        head.push(
            text(reason.clone(), 12.0, look.muted)
                .flex(1.0)
                .centered_y(),
        );
    }
    let mut parts = vec![Element::row(head).gap(10.0), gap(0.0, 4.0)];
    parts.extend(section(
        t("Seguidos"),
        &view.followed,
        t("No sigues ningún hilo de este canal."),
        look,
    ));
    parts.push(gap(0.0, 6.0));
    parts.extend(section(
        t("Activos"),
        &view.active,
        t("No hay otros hilos activos."),
        look,
    ));
    parts.push(gap(0.0, 6.0));
    parts.extend(section(
        t("Archivados"),
        &view.archived,
        t("Los hilos archivados se cargan cuando los pides."),
        look,
    ));
    let (label, denied) = &view.more;
    let more = Button::new(ids::more(view.channel), label.clone(), palette)
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Compact)
        .enabled(denied.is_none())
        .render();
    let mut footer = vec![more];
    footer.push(match denied {
        Some(reason) => text(reason.clone(), 12.0, look.muted)
            .flex(1.0)
            .centered_y(),
        None => Element::spacer().flex(1.0),
    });
    // Under the list, so it stays in view however long the list grows.
    Modal::new(
        root,
        true,
        t("Hilos"),
        view.title.clone(),
        ids::CLOSE,
        palette,
    )
    .width(WIDTH)
    .body(Element::column(parts).gap(6.0))
    .footer(Element::row(footer).gap(10.0))
    .render()
}

fn tags(choices: &[TagChoice], prefix: &str, look: &Look) -> Element {
    let palette = look.menu_palette();
    Element::row(
        choices
            .iter()
            .map(|choice| {
                let toggle = Toggle::new(
                    format!("{prefix}{}", choice.tag),
                    choice.label.clone(),
                    choice.chosen,
                    palette,
                )
                .variant(ToggleVariant::Outline)
                .size(ControlSize::Compact);
                if choice.locked && !choice.chosen {
                    toggle
                        .tooltip(t("Solo la moderación pone esta etiqueta"))
                        .disabled()
                        .render()
                } else {
                    toggle.render()
                }
            })
            .collect(),
    )
    .gap(6.0)
    .wrap()
}

fn footer(cancel: &str, send: &str, label: &str, denied: &Option<String>, look: &Look) -> Element {
    let mut parts = vec![match denied {
        Some(reason) => text(reason.clone(), 12.0, look.muted)
            .flex(1.0)
            .centered_y(),
        None => Element::spacer().flex(1.0),
    }];
    parts.push(
        Button::new(cancel, t("Cancelar"), look.menu_palette())
            .variant(ButtonVariant::Secondary)
            .size(ControlSize::Spacious)
            .render(),
    );
    parts.push(
        Button::new(send, label, look.accent_palette())
            .size(ControlSize::Spacious)
            .enabled(denied.is_none())
            .render(),
    );
    Element::row(parts).gap(9.0)
}

/// The dialog that starts a thread or a post, over `root`.
pub fn create_dialog(root: Element, form: &NewThreadForm, look: &Look) -> Element {
    let palette = look.menu_palette();
    let post = form.content.is_some();
    let mut parts = vec![
        strong_line(
            if post {
                t("Título")
            } else {
                t("Nombre del hilo")
            },
            14.0,
            look.text,
        ),
        TextField::new(
            ids::NEW_NAME,
            form.name.clone(),
            if post {
                t("¿De qué se trata?")
            } else {
                t("Un nombre corto")
            },
            palette,
        )
        .accessibility_label(if post {
            t("Título")
        } else {
            t("Nombre del hilo")
        })
        .size(ControlSize::Spacious)
        .render(),
    ];
    if !form.tags.is_empty() {
        parts.push(gap(0.0, 4.0));
        parts.push(strong_line(
            if form.tag_required {
                t("Etiquetas (al menos una)")
            } else {
                t("Etiquetas")
            },
            14.0,
            look.text,
        ));
        parts.push(tags(&form.tags, ids::NEW_TAG, look));
    }
    if let Some(content) = &form.content {
        parts.push(gap(0.0, 4.0));
        parts.push(strong_line(t("Primer mensaje"), 14.0, look.text));
        // The files above the text, as the composer shows them.
        if let Some(files) = &form.files {
            parts.push(post_files(files, look));
        }
        parts.push(
            TextArea::new(
                ids::NEW_CONTENT,
                content.clone(),
                t("Cuenta tu pregunta o tu idea."),
                palette,
            )
            .accessibility_label(t("Primer mensaje"))
            .height(110.0)
            .render(),
        );
    }
    if let Some((private, denied)) = &form.private {
        let check = Checkbox::new(ids::NEW_PRIVATE, *private, palette)
            .enabled(denied.is_none())
            .accessibility_label(t("Hilo privado"))
            .render()
            .centered_y();
        let mut beside = vec![
            strong_line(t("Hilo privado"), 14.0, look.text),
            text(
                t("Solo lo ven quienes invites con una mención y la moderación."),
                12.0,
                look.muted,
            ),
        ];
        if let Some(reason) = denied {
            beside.push(text(reason.clone(), 12.0, look.muted));
        }
        parts.push(gap(0.0, 4.0));
        parts.push(Element::row(vec![check, Element::column(beside).gap(2.0).flex(1.0)]).gap(10.0));
    }
    Modal::new(
        root,
        true,
        form.title.clone(),
        form.subtitle.clone(),
        ids::NEW_CLOSE,
        palette,
    )
    .width(WIDTH)
    .body(Element::column(parts).gap(8.0))
    .footer(footer(
        ids::NEW_CLOSE,
        ids::NEW_CREATE,
        if post { t("Publicar") } else { t("Crear hilo") },
        &form.denied,
        look,
    ))
    .render()
}

/// «Adjuntar archivos» and the files chosen for a post, as the composer
/// lists them.
fn post_files(files: &PostFiles, look: &Look) -> Element {
    let attach = Button::new(ids::NEW_ATTACH, t("Adjuntar archivos"), look.menu_palette())
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Compact)
        .icon(Icon::Paperclip)
        .enabled(files.denied.is_none() && files.picking.is_none() && !files.waiting)
        .render();
    let mut row = vec![attach.centered_y()];
    if let Some(reason) = &files.denied {
        row.push(
            text(reason.clone(), 12.0, look.muted)
                .flex(1.0)
                .centered_y(),
        );
    }
    let mut parts = vec![Element::row(row).gap(10.0)];
    if !files.files.is_empty() || files.picking.is_some() {
        parts.push(crate::uploads::draft_uploads(
            crate::uploads::DraftUploads {
                channel: rune_model::ChannelId(files.forum),
                files: &files.files,
                picking: files.picking,
                selecting: files.picking.is_some(),
                cancelling: false,
                submitting: files.waiting,
                width: WIDTH - 48.0,
                max_list_height: 120.0,
            },
            look,
        ));
    }
    if let Some((sent, total)) = files.progress {
        let value = if total == 0 {
            1.0
        } else {
            (sent as f32 / total as f32).clamp(0.0, 1.0)
        };
        let said = if sent >= total {
            t("Esperando confirmación…").to_owned()
        } else {
            tr!(
                "Enviando adjuntos: {sent} de {total}",
                sent = crate::file_size(sent),
                total = crate::file_size(total)
            )
        };
        parts.push(crate::line(said, 12.0, look.muted));
        parts.push(
            limn::Progress::new(value, look.palette())
                .id(format!("th:new:{}:progress", files.forum))
                .render()
                .accessibility_label(tr!("{pct} %", pct = (value * 100.0).round())),
        );
    }
    Element::column(parts).gap(8.0)
}

/// The dialog that renames and tags a thread, over `root`.
pub fn edit_dialog(root: Element, form: &EditThreadForm, look: &Look) -> Element {
    let palette = look.menu_palette();
    let mut parts = vec![
        strong_line(t("Nombre"), 14.0, look.text),
        TextField::new(
            ids::EDIT_NAME,
            form.name.clone(),
            t("Un nombre corto"),
            palette,
        )
        .accessibility_label(t("Nombre"))
        .size(ControlSize::Spacious)
        .render(),
    ];
    if !form.tags.is_empty() {
        parts.push(gap(0.0, 4.0));
        parts.push(strong_line(t("Etiquetas"), 14.0, look.text));
        parts.push(tags(&form.tags, ids::EDIT_TAG, look));
    }
    Modal::new(
        root,
        true,
        t("Editar hilo"),
        form.title.clone(),
        ids::EDIT_CLOSE,
        palette,
    )
    .width(WIDTH)
    .body(Element::column(parts).gap(8.0))
    .footer(footer(
        ids::EDIT_CLOSE,
        ids::EDIT_SAVE,
        t("Guardar"),
        &form.denied,
        look,
    ))
    .render()
}

/// The question before deleting a thread, over `root`.
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

    fn thread(id: u64, name: &str) -> ThreadRow {
        ThreadRow {
            thread: id,
            name: name.into(),
            detail: "12 mensajes · Hoy a las 14:05".into(),
            ..ThreadRow::default()
        }
    }

    #[test]
    fn the_list_opens_each_thread_and_offers_more_only_when_it_can() {
        let view = ThreadList {
            channel: 5,
            title: "#galería".into(),
            start_denied: None,
            followed: vec![ThreadRow {
                followed: true,
                ..thread(7, "Salida del sábado")
            }],
            active: Vec::new(),
            archived: vec![ThreadRow {
                archived: true,
                private: true,
                ..thread(8, "Concurso de agosto")
            }],
            more: ("Cargar más".into(), None),
        };
        let (_, laid) = frame(
            list(Element::spacer(), &view, &Look::DARK),
            Size::new(1000.0, 800.0),
        );
        let found = controls(&laid);
        for id in [
            "thread:7",
            "thread:8",
            "th:start:5",
            "th:more:5",
            "th:close:close",
        ] {
            assert!(found.contains(&id.to_owned()), "{id}: {found:?}");
        }
        let painted = texts(&laid);
        for text in [
            "Seguidos — 1",
            "Activos — 0",
            "Archivados — 1",
            "Privado",
            "Archivado",
        ] {
            assert!(painted.contains(&text.to_owned()), "{text}: {painted:?}");
        }
        let done = ThreadList {
            more: (
                "Cargar más".into(),
                Some("No hay más hilos archivados".into()),
            ),
            start_denied: Some("No tienes permiso".into()),
            ..view
        };
        let (_, laid) = frame(
            list(Element::spacer(), &done, &Look::DARK),
            Size::new(1000.0, 800.0),
        );
        let found = controls(&laid);
        assert!(
            !found.contains(&"th:more:5".to_owned()) && !found.contains(&"th:start:5".to_owned())
        );
        assert!(texts(&laid).contains(&"No hay más hilos archivados".to_owned()));
    }

    #[test]
    fn a_post_asks_for_title_tags_and_first_message_and_a_thread_for_privacy() {
        let post = NewThreadForm {
            title: "Nueva publicación".into(),
            subtitle: "#consultas".into(),
            name: String::new(),
            content: Some(String::new()),
            private: None,
            tags: vec![
                TagChoice {
                    tag: 70,
                    label: "❓ Pregunta".into(),
                    chosen: true,
                    locked: false,
                },
                TagChoice {
                    tag: 71,
                    label: "✅ Resuelto".into(),
                    chosen: false,
                    locked: true,
                },
            ],
            tag_required: true,
            denied: Some("Escribe un título".into()),
            files: Some(PostFiles {
                forum: 23,
                files: vec![rune_model::PendingAttachment {
                    token: rune_model::UploadToken(4),
                    filename: "lente.jpg".into(),
                    size_bytes: 2048,
                }],
                picking: None,
                waiting: false,
                denied: None,
                progress: None,
            }),
        };
        let (_, laid) = frame(
            create_dialog(Element::spacer(), &post, &Look::DARK),
            Size::new(1000.0, 800.0),
        );
        let found = controls(&laid);
        for id in [
            ids::NEW_NAME,
            ids::NEW_CONTENT,
            "th:new:tag:70",
            ids::NEW_ATTACH,
            "composer:23:upload:4:remove",
            ids::NEW_CLOSE,
        ] {
            assert!(found.contains(&id.to_owned()), "{id}: {found:?}");
        }
        // A moderated tag is not offered, and nothing goes while it is denied.
        assert!(!found.contains(&"th:new:tag:71".to_owned()));
        assert!(!found.contains(&ids::NEW_CREATE.to_owned()));
        assert!(texts(&laid).contains(&"Etiquetas (al menos una)".to_owned()));

        let thread = NewThreadForm {
            title: "Crear hilo".into(),
            content: None,
            private: Some((false, None)),
            tags: Vec::new(),
            tag_required: false,
            denied: None,
            name: "Dudas".into(),
            files: None,
            ..post
        };
        let (_, laid) = frame(
            create_dialog(Element::spacer(), &thread, &Look::DARK),
            Size::new(1000.0, 800.0),
        );
        let found = controls(&laid);
        for id in [ids::NEW_NAME, ids::NEW_PRIVATE, ids::NEW_CREATE] {
            assert!(found.contains(&id.to_owned()), "{id}: {found:?}");
        }
        assert!(!found.contains(&ids::NEW_CONTENT.to_owned()));
        assert!(!found.contains(&ids::NEW_ATTACH.to_owned()));
    }

    #[test]
    fn editing_offers_name_and_tags_and_deleting_asks_first() {
        let form = EditThreadForm {
            title: "Lente".into(),
            name: "Lente".into(),
            tags: vec![TagChoice {
                tag: 70,
                label: "❓ Pregunta".into(),
                chosen: false,
                locked: false,
            }],
            denied: None,
        };
        let (_, laid) = frame(
            edit_dialog(Element::spacer(), &form, &Look::DARK),
            Size::new(1000.0, 800.0),
        );
        let found = controls(&laid);
        for id in [
            ids::EDIT_NAME,
            "th:edit:tag:70",
            ids::EDIT_SAVE,
            ids::EDIT_CLOSE,
        ] {
            assert!(found.contains(&id.to_owned()), "{id}: {found:?}");
        }
        let (_, laid) = frame(
            confirm(
                Element::spacer(),
                "Eliminar «Lente»",
                "No se puede deshacer.",
                "Eliminar",
                &Look::DARK,
            ),
            Size::new(900.0, 600.0),
        );
        assert!(controls(&laid).contains(&ids::CONFIRM.to_owned()));
    }
}
