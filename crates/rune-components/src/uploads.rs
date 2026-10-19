//! Local outgoing-file metadata, with no filesystem or transport dependency.

use crate::Look;
use limn::{
    Access, ButtonVariant, Component, Element, Icon, IconButton, Role, ScrollArea,
    ScrollbarAppearance, Spinner,
};
use rune_i18n::{t, tr};
use rune_model::{ChannelId, PendingAttachment};

pub struct DraftUploads<'a> {
    pub channel: ChannelId,
    pub files: &'a [PendingAttachment],
    pub picking: Option<u64>,
    pub selecting: bool,
    pub cancelling: bool,
    pub submitting: bool,
    pub width: f32,
    /// Maximum viewport height; shorter file lists keep their natural height.
    pub max_list_height: f32,
}

pub fn draft_uploads(view: DraftUploads<'_>, look: &Look) -> Element {
    let inner = (view.width - 20.0).max(0.0);
    let mut rows = Vec::new();
    for file in view.files {
        let remove = IconButton::new(
            format!("composer:{}:upload:{}:remove", view.channel, file.token.0),
            Icon::Close,
            look.palette(),
        )
        .variant(ButtonVariant::Ghost)
        .tooltip(tr!("Quitar {name}", name = file.filename))
        .enabled(!view.submitting)
        .render();
        rows.push(
            Element::row(vec![
                file_info(file, (inner - 40.0).max(0.0), look).flex(1.0),
                remove.centered_y(),
            ])
            .gap(8.0),
        );
    }
    let files = Element::column(rows).gap(8.0);
    let mut rows = if view.files.is_empty() {
        Vec::new()
    } else {
        vec![
            ScrollArea::new(format!("composer:{}:uploads", view.channel), files)
                .appearance(ScrollbarAppearance::overlay().buttons(false))
                .render()
                .size(Some(inner), None)
                .max_height(view.max_list_height),
        ]
    };
    if let Some(request) = view.picking {
        rows.push(
            Element::row(vec![
                Spinner::new(
                    format!("composer:{}:pick:{request}:progress", view.channel),
                    look.accent,
                )
                .render()
                .centered_y(),
                crate::line(
                    if view.cancelling && !view.selecting {
                        t("Cancelando preparación…")
                    } else if view.cancelling {
                        t("Cancelando selección…")
                    } else if !view.selecting {
                        t("Preparando archivos…")
                    } else {
                        t("Seleccionando y preparando archivos…")
                    },
                    12.0,
                    look.muted,
                )
                .flex(1.0)
                .centered_y(),
                IconButton::new(
                    format!("composer:{}:pick:{request}:cancel", view.channel),
                    Icon::Close,
                    look.palette(),
                )
                .variant(ButtonVariant::Ghost)
                .tooltip(if view.selecting {
                    t("Cancelar selección y preparación")
                } else {
                    t("Cancelar preparación")
                })
                .enabled(!view.cancelling)
                .render()
                .centered_y(),
            ])
            .gap(8.0),
        );
    }
    if view.submitting {
        rows.push(crate::line(t("Preparando el envío…"), 12.0, look.muted));
    }
    Element::panel(Element::column(rows).gap(8.0))
        .padding(10.0)
        .size(Some(view.width), None)
        .background(look.card)
        .border(look.border, 1.0)
        .radius(8.0)
}

pub(crate) fn file_info(file: &PendingAttachment, width: f32, look: &Look) -> Element {
    Element::row(vec![
        crate::icon(Icon::Paperclip, look.muted).centered_y(),
        Element::column(vec![
            crate::line(&file.filename, 13.0, look.text).size(Some((width - 28.0).max(0.0)), None),
            crate::line(crate::file_size(file.size_bytes), 11.0, look.muted),
        ])
        .gap(2.0)
        .flex(1.0),
    ])
    .gap(8.0)
    .size(Some(width), None)
    .accessible(Access::new(Role::Group).name(format!(
        "{}, {}",
        file.filename,
        crate::file_size(file.size_bytes)
    )))
}

pub fn pending_uploads(
    files: &[PendingAttachment],
    progress: Option<(u64, u64)>,
    width: f32,
    look: &Look,
) -> Element {
    let mut rows: Vec<_> = files
        .iter()
        .map(|file| file_info(file, width, look))
        .collect();
    if let Some((sent, total)) = progress {
        let said = if sent == total {
            t("Esperando confirmación…").to_owned()
        } else {
            tr!(
                "Enviando adjuntos: {sent} de {total}",
                sent = crate::file_size(sent),
                total = crate::file_size(total)
            )
        };
        rows.push(crate::line(said, 12.0, look.muted).size(Some(width), None));
    }
    Element::column(rows).gap(6.0).size(Some(width), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use rune_model::UploadToken;
    #[test]
    fn a_prepared_file_and_selection_have_scoped_remove_and_cancel_controls() {
        let files = [PendingAttachment {
            token: UploadToken(7),
            filename: "fixture.txt".into(),
            size_bytes: 12,
        }];
        let (_, frame) = frame(
            draft_uploads(
                DraftUploads {
                    channel: ChannelId(10),
                    files: &files,
                    picking: Some(90),
                    selecting: true,
                    cancelling: false,
                    submitting: false,
                    width: 320.0,
                    max_list_height: 152.0,
                },
                &Look::DARK,
            ),
            limn::Size::new(320.0, 300.0),
        );
        assert!(texts(&frame).iter().any(|text| text == "fixture.txt"));
        for id in ["composer:10:upload:7:remove", "composer:10:pick:90:cancel"] {
            assert!(controls(&frame).contains(&id.to_string()), "missing {id}");
        }
    }
    #[test]
    fn complete_body_progress_still_waits_for_confirmation() {
        let (_, frame) = frame(
            pending_uploads(&[], Some((12, 12)), 320.0, &Look::DARK),
            limn::Size::new(320.0, 300.0),
        );
        assert!(texts(&frame)
            .iter()
            .any(|text| text.contains("Esperando confirmación")));
    }

    #[test]
    fn eight_prepared_files_keep_the_editor_and_selection_cancel_visible() {
        let files: Vec<_> = (1..=8)
            .map(|id| PendingAttachment {
                token: UploadToken(id),
                filename: format!("fixture-{id}.txt"),
                size_bytes: 12,
            })
            .collect();
        let (mut tree, frame) = frame(
            Element::column(vec![
                Element::clickable(
                    "fixture:uploads",
                    draft_uploads(
                        DraftUploads {
                            channel: ChannelId(10),
                            files: &files,
                            picking: Some(90),
                            selecting: true,
                            cancelling: false,
                            submitting: false,
                            width: 320.0,
                            max_list_height: 152.0,
                        },
                        &Look::DARK,
                    ),
                ),
                Element::clickable("fixture:editor", Element::text("Nuevo mensaje"))
                    .size(Some(320.0), Some(48.0)),
            ])
            .gap(8.0),
            limn::Size::new(320.0, 400.0),
        );
        let region = |id: &str| frame.regions.iter().find(|region| region.id == id).unwrap();
        assert!(
            region("fixture:uploads").rect.height <= 224.0,
            "the file list must leave room for the editor: {:?}",
            region("fixture:uploads").rect
        );
        assert!(region("fixture:editor").rect.y + 48.0 <= 400.0);
        assert!(region("composer:10:pick:90:cancel").rect.y + 32.0 <= 224.0);
        tree.scroll_to_end("composer:10:uploads");
        let scrolled = tree.layout(limn::Size::new(320.0, 400.0));
        let last = scrolled
            .regions
            .iter()
            .find(|region| region.id == "composer:10:upload:8:remove")
            .expect("the final file remains reachable by scrolling");
        assert!(last.rect.y >= 10.0 && last.rect.y + last.rect.height <= 163.0);
    }

    #[test]
    fn a_small_file_list_budget_preserves_picker_and_editor_without_padding_one_file() {
        let files: Vec<_> = (1..=8)
            .map(|token| PendingAttachment {
                token: UploadToken(token),
                filename: format!("fixture-{token}.txt"),
                size_bytes: 12,
            })
            .collect();
        let window = limn::Size::new(320.0, 240.0);
        for count in [1, 8] {
            let (mut tree, frame) = frame(
                Element::column(vec![
                    Element::clickable(
                        "fixture:uploads",
                        draft_uploads(
                            DraftUploads {
                                channel: ChannelId(10),
                                files: &files[..count],
                                picking: Some(90),
                                selecting: true,
                                cancelling: false,
                                submitting: false,
                                width: window.width,
                                max_list_height: 96.0,
                            },
                            &Look::DARK,
                        ),
                    ),
                    Element::clickable("fixture:editor", Element::text("Borrador conservado"))
                        .size(Some(window.width), Some(48.0)),
                ])
                .gap(8.0),
                window,
            );
            let region = |frame: &limn::Frame, id: &str| {
                frame
                    .regions
                    .iter()
                    .find(|region| region.id == id)
                    .unwrap()
                    .rect
            };
            let editor = region(&frame, "fixture:editor");
            let cancel = region(&frame, "composer:10:pick:90:cancel");
            let center = |rect: limn::Rect| {
                limn::Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
            };
            if count == 1 {
                assert!(
                    region(&frame, "fixture:uploads").height < 96.0 + cancel.height,
                    "one file must not reserve the whole list budget"
                );
            } else {
                assert!(
                    region(&frame, "composer:10:uploads").height <= 96.0,
                    "the requested file-list height must be respected"
                );
            }
            for (id, rect) in [
                ("fixture:editor", editor),
                ("composer:10:pick:90:cancel", cancel),
            ] {
                assert!(rect.y >= 0.0 && rect.y + rect.height <= window.height);
                assert_eq!(frame.hit_test(center(rect)), Some(id));
            }
            let first = region(&frame, "composer:10:upload:1:remove");
            assert_eq!(
                frame.hit_test(center(first)),
                Some("composer:10:upload:1:remove")
            );
            if count == 8 {
                tree.scroll_to_end("composer:10:uploads");
                let frame = tree.layout(window);
                let last = region(&frame, "composer:10:upload:8:remove");
                assert!(last.y >= 10.0 && last.y + last.height <= 106.0);
                assert_eq!(
                    frame.hit_test(center(last)),
                    Some("composer:10:upload:8:remove")
                );
                assert_eq!(frame.hit_test(center(editor)), Some("fixture:editor"));
                assert_eq!(
                    frame.hit_test(center(cancel)),
                    Some("composer:10:pick:90:cancel")
                );
            }
        }
    }
}
