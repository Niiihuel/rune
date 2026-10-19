//! The dialogs of message actions (P4): forwarding a message to other
//! conversations, starting a poll, who reacted or voted, and the question
//! before opening a link outside RUNE.

use limn::{
    Button, ButtonVariant, Checkbox, Color, Component, ControlSize, Element, Icon, ListRow, Modal,
    Popover, PopoverPlacement, ScrollArea, SearchField, TextField,
};
use rune_i18n::{t, tr};
use rune_model::{ChannelId, ChannelKind, UserId};

use crate::{emoji, icon, line, strong_line, text, Look};

/// Action IDs of the dialogs.
pub mod ids {
    use rune_model::ChannelId;

    pub const FORWARD_QUERY: &str = "fw:query";
    pub const FORWARD_NOTE: &str = "fw:note";
    pub const FORWARD_SEND: &str = "fw:send";
    pub const FORWARD_CLOSE: &str = "fw:close";
    /// Prefix of a destination's checkbox.
    pub const FORWARD_PICK: &str = "fw:pick:";

    pub fn forward_pick(channel: ChannelId) -> String {
        format!("{FORWARD_PICK}{channel}")
    }

    pub const POLL_QUESTION: &str = "poll:question";
    /// Prefixes of an answer's field, its emoji button, its picker's
    /// choices (`{prefix}{index}={glyph}`) and its removal; the index follows.
    pub const POLL_ANSWER: &str = "poll:answer:";
    pub const POLL_EMOJI: &str = "poll:emoji:";
    pub const POLL_EMOJI_PICK: &str = "poll:emoji-pick:";
    pub const POLL_EMOJI_DISMISS: &str = "poll:emoji-dismiss";
    pub const POLL_REMOVE: &str = "poll:remove:";
    pub const POLL_ADD: &str = "poll:add";
    /// Prefix of a length's choice; its hours follow.
    pub const POLL_HOURS: &str = "poll:hours:";
    pub const POLL_MULTI: &str = "poll:multi";
    pub const POLL_POST: &str = "poll:post";
    pub const POLL_CLOSE: &str = "poll:close";

    /// Prefix of a tab of who reacted or voted; its index follows.
    pub const PEOPLE_TAB: &str = "people:tab:";
    pub const PEOPLE_MORE: &str = "people:more";
    pub const PEOPLE_CLOSE: &str = "people:close";

    pub const LINK_OPEN: &str = "link:open";
    pub const LINK_TRUST: &str = "link:trust";
    pub const LINK_CLOSE: &str = "link:close";
}

/// A conversation a message may be forwarded to.
#[derive(Clone, Debug, PartialEq)]
pub struct Destination {
    pub channel: ChannelId,
    /// «#general», «Ada».
    pub title: String,
    /// Its server, or «Mensaje directo».
    pub detail: String,
    pub kind: ChannelKind,
    pub chosen: bool,
}

/// What the forwarding dialog shows.
pub struct ForwardView<'a> {
    pub query: &'a str,
    pub destinations: &'a [Destination],
    /// How many are chosen, of at most `max`.
    pub chosen: usize,
    pub max: usize,
    pub note: &'a str,
    /// The first words of what is forwarded.
    pub preview: &'a str,
    /// Why it cannot be sent as it is, when it cannot.
    pub problem: Option<&'a str>,
}

/// The dialog to forward a message, over `root`, `width` wide.
pub fn forward_dialog(root: Element, view: ForwardView<'_>, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let inner = Modal::body_width(width);
    let rows: Vec<Element> = view
        .destinations
        .iter()
        .map(|destination| {
            let leading = Checkbox::new(
                ids::forward_pick(destination.channel),
                destination.chosen,
                palette,
            )
            .accessibility_label(tr!("Reenviar a {title}", title = destination.title))
            .render();
            ListRow::new(
                ids::forward_pick(destination.channel),
                destination.title.clone(),
                palette,
            )
            .leading(leading)
            .icon(match destination.kind {
                ChannelKind::Direct => Icon::At,
                ChannelKind::Group => Icon::Users,
                _ => Icon::Hash,
            })
            .subtitle(destination.detail.clone())
            .render()
        })
        .collect();
    let list = if rows.is_empty() {
        line(t("No hay conversaciones con ese nombre."), 14.0, look.muted)
    } else {
        ScrollArea::new("fw:list", Element::column(rows).gap(2.0))
            .appearance(look.scrollbar(true))
            .render()
            .size(Some(inner), Some(300.0))
    };
    let preview = Element::row(vec![
        Element::panel(Element::spacer())
            .size(Some(3.0), None)
            .background(look.accent),
        text(view.preview.to_owned(), 13.0, look.muted).flex(1.0),
    ])
    .gap(8.0)
    .padding(8.0)
    .background(look.raised)
    .radius(6.0)
    .size(Some(inner), None);
    let body = Element::column(vec![
        SearchField::new(
            ids::FORWARD_QUERY,
            view.query.to_owned(),
            t("Buscar conversaciones"),
            palette,
        )
        .render(),
        line(
            tr!(
                "{chosen} / {max} conversaciones elegidas",
                chosen = view.chosen,
                max = view.max
            ),
            12.0,
            look.muted,
        ),
        list,
        preview,
        TextField::new(
            ids::FORWARD_NOTE,
            view.note.to_owned(),
            t("Agregá un mensaje (opcional)"),
            palette,
        )
        .accessibility_label(t("Mensaje opcional"))
        .render(),
    ])
    .gap(10.0);
    let mut send = Button::new(ids::FORWARD_SEND, t("Enviar"), look.accent_palette())
        .size(ControlSize::Spacious)
        .enabled(view.problem.is_none());
    if let Some(problem) = view.problem {
        send = send.tooltip(problem.to_owned());
    }
    let footer = Element::row(vec![
        Element::spacer().flex(1.0),
        Button::new(ids::FORWARD_CLOSE, t("Cancelar"), palette)
            .variant(ButtonVariant::Secondary)
            .size(ControlSize::Spacious)
            .render(),
        send.render(),
    ])
    .gap(9.0);
    Modal::new(
        root,
        true,
        t("Reenviar a"),
        t("Elegí dónde compartir este mensaje."),
        ids::FORWARD_CLOSE,
        palette,
    )
    .width(width)
    .body(body)
    .footer(footer)
    .initial_focus(ids::FORWARD_QUERY)
    .render()
}

/// One answer of a poll being written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraftAnswer {
    pub text: String,
    pub emoji: Option<String>,
}

/// What the dialog that starts a poll shows.
pub struct PollView<'a> {
    pub question: &'a str,
    pub answers: &'a [DraftAnswer],
    pub hours: u32,
    pub multiselect: bool,
    /// The answer whose emoji is being picked.
    pub picking: Option<usize>,
    /// Sent, waiting for the provider.
    pub posting: bool,
    /// Why it cannot be posted yet, when it cannot.
    pub problem: Option<&'a str>,
    /// Why the provider refused it.
    pub error: Option<&'a str>,
    /// Lengths offered, in hours.
    pub lengths: &'a [u32],
    pub max_answers: usize,
}

/// «1 hora», «3 días», «1 semana».
pub fn poll_length(hours: u32) -> String {
    match hours {
        1 => t("1 hora").into(),
        168 => t("1 semana").into(),
        hours if hours >= 48 && hours % 24 == 0 => tr!("{days} días", days = hours / 24),
        24 => t("1 día").into(),
        hours => tr!("{hours} horas", hours),
    }
}

/// The dialog to start a poll, over `root`, `width` wide.
pub fn poll_dialog(root: Element, view: PollView<'_>, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let mut parts = vec![
        strong_line(t("Pregunta"), 12.0, look.muted),
        TextField::new(
            ids::POLL_QUESTION,
            view.question.to_owned(),
            t("¿Qué querés preguntar?"),
            palette,
        )
        .accessibility_label(t("Pregunta"))
        .render(),
        strong_line(t("Respuestas"), 12.0, look.muted),
    ];
    for (index, answer) in view.answers.iter().enumerate() {
        let glyph = Element::clickable(
            format!("{}{index}", ids::POLL_EMOJI),
            crate::centered(match &answer.emoji {
                Some(glyph) => Element::text(glyph.clone())
                    .font_family(limn::FontFamily::Sans)
                    .font_size(20.0)
                    .foreground(look.text),
                None => icon(Icon::Emoji, look.muted),
            }),
        )
        .size(Some(36.0), Some(36.0))
        .radius(6.0)
        .hover_background(look.hover)
        .accessibility_label(tr!("Emoji de la respuesta {n}", n = index + 1));
        let glyph = Popover::new(
            glyph,
            if view.picking == Some(index) {
                emoji::picker(
                    &format!("{}{index}=", ids::POLL_EMOJI_PICK),
                    t("Emoji de la respuesta"),
                    look,
                )
            } else {
                Element::spacer()
            },
            view.picking == Some(index),
            ids::POLL_EMOJI_DISMISS,
            palette,
        )
        .placement(PopoverPlacement::BottomStart)
        .width(emoji::WIDTH)
        .render();
        let mut row = vec![
            glyph,
            TextField::new(
                format!("{}{index}", ids::POLL_ANSWER),
                answer.text.clone(),
                tr!("Respuesta {n}", n = index + 1),
                palette,
            )
            .accessibility_label(tr!("Respuesta {n}", n = index + 1))
            .render()
            .flex(1.0),
        ];
        if view.answers.len() > 2 {
            row.push(
                limn::IconButton::new(format!("{}{index}", ids::POLL_REMOVE), Icon::Trash, palette)
                    .variant(ButtonVariant::Ghost)
                    .tooltip(t("Quitar respuesta"))
                    .render(),
            );
        }
        parts.push(Element::row(row).gap(6.0));
    }
    if view.answers.len() < view.max_answers {
        parts.push(
            Button::new(ids::POLL_ADD, t("Agregar otra respuesta"), palette)
                .variant(ButtonVariant::Ghost)
                .icon(Icon::Add)
                .render(),
        );
    }
    parts.push(strong_line(t("Duración"), 12.0, look.muted));
    parts.push(
        Element::row(
            view.lengths
                .iter()
                .map(|hours| {
                    Button::new(
                        format!("{}{hours}", ids::POLL_HOURS),
                        poll_length(*hours),
                        palette,
                    )
                    .variant(if *hours == view.hours {
                        ButtonVariant::Default
                    } else {
                        ButtonVariant::Ghost
                    })
                    .size(ControlSize::Compact)
                    .render()
                })
                .collect(),
        )
        .gap(4.0),
    );
    parts.push(
        Checkbox::new(ids::POLL_MULTI, view.multiselect, palette)
            .label(t("Permitir varias respuestas"))
            .render(),
    );
    if let Some(error) = view.error {
        parts.push(text(error.to_owned(), 13.0, look.danger));
    }
    let label = if view.posting {
        t("Publicando…")
    } else {
        t("Publicar")
    };
    let mut post = Button::new(ids::POLL_POST, label, look.accent_palette())
        .size(ControlSize::Spacious)
        .enabled(view.problem.is_none() && !view.posting);
    if let Some(problem) = view.problem {
        post = post.tooltip(problem.to_owned());
    }
    let footer = Element::row(vec![
        Element::spacer().flex(1.0),
        Button::new(ids::POLL_CLOSE, t("Cancelar"), palette)
            .variant(ButtonVariant::Secondary)
            .size(ControlSize::Spacious)
            .render(),
        post.render(),
    ])
    .gap(9.0);
    Modal::new(
        root,
        true,
        t("Crear encuesta"),
        t("Hasta 10 respuestas, con texto, un emoji o ambos."),
        ids::POLL_CLOSE,
        palette,
    )
    .width(width)
    .body(
        ScrollArea::new("poll:body", Element::column(parts).gap(8.0))
            .appearance(look.scrollbar(true))
            .render()
            .size(None, Some(440.0)),
    )
    .footer(footer)
    .initial_focus(ids::POLL_QUESTION)
    .render()
}

/// A tab of who reacted or voted: an emoji or an answer, with its count.
#[derive(Clone, Debug, PartialEq)]
pub struct PeopleTab {
    pub label: String,
    pub count: u32,
    pub selected: bool,
}

/// Someone in the list.
#[derive(Clone)]
pub struct Person {
    pub user: UserId,
    pub name: String,
    /// Their handle, beside their name.
    pub handle: String,
    pub image: Option<limn::RasterImage>,
    pub accent: Color,
}

/// What the dialog of who reacted or voted shows.
pub struct PeopleView<'a> {
    pub title: &'a str,
    pub tabs: &'a [PeopleTab],
    pub people: &'a [Person],
    pub loading: bool,
    /// More can be read.
    pub more: bool,
    pub error: Option<&'a str>,
    /// What an empty list says.
    pub empty: &'a str,
    /// A note under the list, such as that it is simulated.
    pub note: Option<&'a str>,
}

/// The dialog of who reacted or voted, over `root`, `width` wide.
pub fn people_dialog(root: Element, view: PeopleView<'_>, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let inner = Modal::body_width(width);
    let tabs = Element::row(
        view.tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                Button::new(
                    format!("{}{index}", ids::PEOPLE_TAB),
                    format!("{}  {}", tab.label, tab.count),
                    palette,
                )
                .variant(if tab.selected {
                    ButtonVariant::Default
                } else {
                    ButtonVariant::Ghost
                })
                .size(ControlSize::Compact)
                .render()
            })
            .collect(),
    )
    .gap(4.0);
    let rows: Vec<Element> = view
        .people
        .iter()
        .map(|person| {
            ListRow::new(
                format!("member:{}", person.user),
                person.name.clone(),
                palette,
            )
            .leading(crate::avatar_with_image(
                &person.name,
                person.accent,
                32.0,
                person.image.clone(),
            ))
            .meta(person.handle.clone())
            .render()
        })
        .collect();
    let mut list = Vec::new();
    if rows.is_empty() && !view.loading && view.error.is_none() {
        list.push(line(view.empty.to_owned(), 14.0, look.muted));
    }
    list.extend(rows);
    if view.loading {
        list.push(line(t("Cargando…"), 13.0, look.muted));
    } else if let Some(error) = view.error {
        list.push(text(error.to_owned(), 13.0, look.danger));
    } else if view.more {
        list.push(
            Button::new(ids::PEOPLE_MORE, t("Cargar más"), palette)
                .variant(ButtonVariant::Ghost)
                .render(),
        );
    }
    let body = vec![
        Element::horizontal_scroll("people:tabs", tabs),
        ScrollArea::new("people:list", Element::column(list).gap(2.0))
            .appearance(look.scrollbar(true))
            .render()
            .size(Some(inner), Some(320.0)),
    ];
    Modal::new(
        root,
        true,
        view.title.to_owned(),
        // Under the title, where it shows whatever the list's length.
        view.note.unwrap_or_default().to_owned(),
        ids::PEOPLE_CLOSE,
        palette,
    )
    .width(width)
    .body(Element::column(body).gap(10.0))
    .render()
}

/// What the question before opening a link outside RUNE shows.
pub struct LinkView<'a> {
    pub url: &'a str,
    pub host: &'a str,
    /// Its text was not its address.
    pub masked: bool,
    /// «Confiar en este dominio» is checked.
    pub trust: bool,
}

/// The question before opening a link outside RUNE, over `root`.
pub fn link_dialog(root: Element, view: LinkView<'_>, width: f32, look: &Look) -> Element {
    let palette = look.menu_palette();
    let inner = Modal::body_width(width);
    let mut body = Vec::new();
    if view.masked {
        body.push(
            Element::row(vec![
                icon(Icon::WarningTriangle, look.idle).centered_y(),
                text(
                    t("El texto de este enlace no es su dirección: fijate bien a dónde lleva."),
                    13.0,
                    look.text,
                )
                .flex(1.0),
            ])
            .gap(8.0),
        );
    }
    body.push(
        Element::panel(
            Element::text(view.url.to_owned())
                .font_family(limn::FontFamily::Monospace)
                .font_size(13.0)
                .foreground(look.text),
        )
        .padding(10.0)
        .background(look.raised)
        .radius(6.0)
        .size(Some(inner), None),
    );
    body.push(
        Checkbox::new(ids::LINK_TRUST, view.trust, palette)
            .label(tr!("Confiar en {host} de ahora en más", host = view.host))
            .render(),
    );
    let footer = Element::row(vec![
        Element::spacer().flex(1.0),
        Button::new(ids::LINK_CLOSE, t("Volver"), palette)
            .variant(ButtonVariant::Secondary)
            .size(ControlSize::Spacious)
            .render(),
        Button::new(
            ids::LINK_OPEN,
            t("Abrir en el navegador"),
            look.accent_palette(),
        )
        .size(ControlSize::Spacious)
        .icon(Icon::ExternalLink)
        .render(),
    ])
    .gap(9.0);
    Modal::new(
        root,
        true,
        t("¿Abrir este enlace?"),
        t("Se abre fuera de RUNE, en tu navegador."),
        ids::LINK_CLOSE,
        palette,
    )
    .width(width)
    .body(Element::column(body).gap(12.0))
    .footer(footer)
    .initial_focus(ids::LINK_OPEN)
    .render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    fn base() -> Element {
        Element::spacer()
    }

    #[test]
    fn the_forward_dialog_lists_destinations_with_their_boxes() {
        let destinations = [Destination {
            channel: ChannelId(7),
            title: "#general".into(),
            detail: "Taller".into(),
            kind: ChannelKind::Text,
            chosen: true,
        }];
        let (_, laid) = frame(
            forward_dialog(
                base(),
                ForwardView {
                    query: "",
                    destinations: &destinations,
                    chosen: 1,
                    max: 5,
                    note: "",
                    preview: "Hola",
                    problem: None,
                },
                520.0,
                &Look::DARK,
            ),
            Size::new(900.0, 800.0),
        );
        let shown = texts(&laid);
        assert!(
            shown.iter().any(|text| text.contains("#general")),
            "{shown:?}"
        );
        assert!(shown.iter().any(|text| text.contains("1 / 5")), "{shown:?}");
        assert!(controls(&laid).contains(&ids::forward_pick(ChannelId(7))));
        assert!(controls(&laid).contains(&ids::FORWARD_SEND.to_owned()));
    }

    #[test]
    fn the_poll_dialog_offers_every_length_and_removes_only_past_two() {
        let answers = vec![DraftAnswer::default(), DraftAnswer::default()];
        let view = |answers: &[DraftAnswer]| {
            frame(
                poll_dialog(
                    base(),
                    PollView {
                        question: "",
                        answers,
                        hours: 24,
                        multiselect: false,
                        picking: None,
                        posting: false,
                        problem: Some("Escribí una pregunta"),
                        error: None,
                        lengths: &[1, 4, 8, 24, 72, 168],
                        max_answers: 10,
                    },
                    520.0,
                    &Look::DARK,
                ),
                Size::new(900.0, 900.0),
            )
            .1
        };
        let laid = view(&answers);
        let shown = texts(&laid);
        for length in ["1 hora", "1 día", "3 días", "1 semana"] {
            assert!(shown.iter().any(|text| text == length), "{shown:?}");
        }
        assert!(!controls(&laid).contains(&format!("{}0", ids::POLL_REMOVE)));
        let three = vec![DraftAnswer::default(); 3];
        assert!(controls(&view(&three)).contains(&format!("{}0", ids::POLL_REMOVE)));
        assert_eq!(poll_length(4), "4 horas");
    }

    #[test]
    fn the_link_question_warns_of_masked_links() {
        let laid = |masked| {
            frame(
                link_dialog(
                    base(),
                    LinkView {
                        url: "https://ejemplo.test/a",
                        host: "ejemplo.test",
                        masked,
                        trust: false,
                    },
                    480.0,
                    &Look::DARK,
                ),
                Size::new(900.0, 700.0),
            )
            .1
        };
        let warned = |masked| {
            texts(&laid(masked))
                .iter()
                .any(|text| text.contains("no es su dirección"))
        };
        assert!(warned(true) && !warned(false));
        assert!(texts(&laid(false))
            .iter()
            .any(|text| text.contains("Confiar en ejemplo.test")));
    }
}
