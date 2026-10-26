//! Where messages are written: limn's composer, sending on Enter, with the
//! message being answered above it, the «+» menu, the emoji picker,
//! completions for @mentions, #channels and :emoji:, and who is typing below.

use limn::{
    ButtonVariant, Color, Component, Composer, CornerRadii, DropdownMenu, DropdownMenuEntry,
    DropdownMenuItem, Element, FontFamily, Icon, IconButton, Popover, PopoverPlacement,
    SuggestionItem, SuggestionList, TextHighlight, TextSpan,
};
use rune_i18n::{t, tr};

use crate::{emoji, gap, icon, line, Look};

/// One completion offered for what is being written.
#[derive(Clone, Debug, PartialEq)]
pub struct Completion {
    /// What takes the place of the `@`, `#` or `:` and what follows it.
    pub insert: String,
    pub label: String,
    /// The quiet line: a handle, what an option does, a category.
    pub detail: String,
    pub icon: Icon,
    /// The heading of the group it starts, after the first group.
    pub section: Option<String>,
    // limn suggestions (P8).
    /// A person's face in the icon's place: their color, and their
    /// picture once it is loaded.
    pub face: Option<(Color, Option<limn::RasterImage>)>,
    /// The label's ink in place of the text's, such as a role's color.
    pub ink: Option<Color>,
    /// A quiet text at the end of the row, such as a member's handle.
    pub aside: String,
}

/// The completions shown: the heading of the first group, and its rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Completions {
    pub title: String,
    pub items: Vec<Completion>,
    pub selected: usize,
}

/// Everything the composer shows.
pub struct ComposerView<'a> {
    /// The editor's ID: one per conversation, so each keeps its draft.
    pub editor: String,
    /// The text it starts with the first time it is shown.
    pub draft: String,
    pub placeholder: String,
    pub width: f32,
    /// Who is being answered: their name and its ink.
    pub reply: Option<(String, Color)>,
    /// Why the account cannot write here, when it cannot.
    pub disabled: Option<String>,
    /// Why files cannot be sent here, when they cannot.
    pub attach_denied: Option<String>,
    /// Current provider limits, independent of the reason an action is denied.
    pub attach_hint: Option<String>,
    pub plus_open: bool,
    /// Who is typing, as a sentence.
    pub typing: Option<String>,
    /// The emoji picker, while open: what is searched and the emoji used lately.
    pub emoji: Option<emoji::PickerView<'a>>,
    pub completions: Option<Completions>,
    /// The inks of the draft as it is written.
    pub highlights: Vec<TextHighlight>,
    // Message actions (P4).
    pub actions: ComposerActions,
}

/// What message actions add to the composer (P4).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComposerActions {
    /// Replying notifies its author (`Some(true)`) or not, when the
    /// provider lets the account choose; the reply strip's switch.
    pub mention_reply: Option<bool>,
    /// The draft goes without notifying anyone.
    pub silent: bool,
    /// «Crear encuesta» in the «+» menu: `None` hides it, `Some(reason)`
    /// says why it cannot be used.
    pub poll: Option<Option<String>>,
}

/// Action IDs the composer reports.
pub mod ids {
    /// Cancels the reply.
    pub const CANCEL: &str = "composer:cancel";
    /// The emoji button; every action of its picker starts with it too.
    pub const EMOJI: &str = "composer:emoji";
    pub const EMOJI_DISMISS: &str = "composer:emoji:dismiss";
    /// Prefix of a chosen emoji.
    pub const EMOJI_PICK: &str = "composer:emoji=";
    pub const PLUS: &str = "composer:plus";
    pub const PLUS_DISMISS: &str = "composer:plus:dismiss";
    pub const ATTACH: &str = "composer:attach";
    /// Attach the files or the picture on the clipboard.
    pub const PASTE: &str = "composer:paste";
    pub const MENTIONS: &str = "composer:mentions";
    pub const MENTIONS_DISMISS: &str = "composer:mentions:dismiss";
    /// Prefix of a chosen completion, followed by what it inserts.
    pub const MENTION_PICK: &str = "composer:mention=";
    // Message actions (P4).
    /// The reply strip's switch: notify who is answered or not.
    pub const MENTION_REPLY: &str = "composer:mention-reply";
    /// «Crear encuesta» in the «+» menu.
    pub const POLL: &str = "composer:poll";
}

/// A compact single-line box; the editor grows from its actual font metrics.
const MIN_HEIGHT: f32 = 48.0;
/// The strip of a reply over the box.
const STRIP: f32 = 37.5;
/// Room around the text: it starts where the messages' text does.
const EDITOR_PADDING: f32 = 14.0;
/// The column of «+», with the glyph centered in it.
const PLUS_WIDTH: f32 = 60.0;
/// The column of the emoji, with the glyph centered in it.
const EMOJI_WIDTH: f32 = 52.0;
/// An icon button's side.
const CONTROL: f32 = 32.0;
/// A person's face among the completions.
const FACE: f32 = 24.0;
/// limn's composer: its padding, between its controls and the editor, and
/// around the editor's text, which together make [`EDITOR_PADDING`].
const CARD_PADDING: f32 = 8.0;
const CONTROL_GAP: f32 = 6.0;
const TEXT_PADDING: f32 = EDITOR_PADDING - CARD_PADDING;

/// The composer, with the line of who is typing under it.
pub fn composer(view: ComposerView<'_>, look: &Look) -> Element {
    let palette = raised_palette(look);
    let enabled = view.disabled.is_none();
    let plus = plus_menu(&view, enabled, look);
    let emoji_button = IconButton::new(ids::EMOJI, Icon::Emoji, palette)
        .variant(ButtonVariant::Ghost)
        .tooltip(t("Emoji"))
        .enabled(enabled)
        .render();
    let picker_open = view.emoji.is_some();
    let picker = view.emoji.map(|picker| emoji::full_picker(picker, look));
    let emoji_button = Popover::new(
        emoji_button,
        picker.unwrap_or_else(Element::spacer),
        picker_open,
        ids::EMOJI_DISMISS,
        look.menu_palette(),
    )
    .placement(PopoverPlacement::TopEnd)
    .width(emoji::picker_size(true).0)
    .render();
    let placeholder = view.disabled.clone().unwrap_or(view.placeholder);
    // Each control centered in a column `width` wide at the card's edge,
    // as before limn's composer took the box: its padding and its gap stand
    // in the column, so the text starts where the messages' text does.
    let column = |control: Element, width: f32, leading: bool| {
        let outer = width / 2.0 - CARD_PADDING - CONTROL / 2.0;
        let inner = width - CONTROL_GAP - CONTROL - outer;
        let (before, after) = if leading {
            (outer, inner)
        } else {
            (inner, outer)
        };
        Element::row(vec![gap(before, 0.0), control, gap(after, 0.0)])
    };
    let radii = if view.reply.is_some() {
        CornerRadii::bottom(8.0)
    } else {
        CornerRadii::same(8.0)
    };
    // limn's composer: the editor grows between «+» and the emoji, which
    // stay beside its first line; a long reason not to write wraps.
    let mut card = Composer::new(
        view.editor.clone(),
        view.draft,
        placeholder,
        view.width,
        limn::ComponentPalette {
            selected: look.raised,
            ..palette
        },
    )
    .pill_controls(
        column(plus, PLUS_WIDTH, true),
        column(emoji_button, EMOJI_WIDTH, false),
    )
    .inline_controls(true)
    .auto_grow(MIN_HEIGHT - 2.0 * CARD_PADDING, 300.0 - 2.0 * CARD_PADDING)
    .editor_padding(TEXT_PADDING)
    .editor_font_size(16.0)
    .editor_highlights(view.highlights)
    .enter_submits(true)
    .border_color(look.border)
    .card_corner_radii(radii)
    .enabled(enabled);
    if view.attach_denied.is_none() {
        card = card.drop_target();
    }
    let card = card.render();
    let card = match &view.reply {
        Some((name, ink)) => {
            let said = Element::formatted_text(vec![
                TextSpan::new(t("Respondiendo a "), look.text, 14.0, FontFamily::Sans),
                TextSpan::new(name.clone(), *ink, 14.0, FontFamily::SansMedium),
            ]);
            let mention = view.actions.mention_reply.map(|on| {
                let (label, tip) = if on {
                    (
                        t("@ ACTIVADO"),
                        t("Le llega una notificación a quien respondes"),
                    )
                } else {
                    (
                        t("@ DESACTIVADO"),
                        t("Respondes sin notificar a quien escribió"),
                    )
                };
                limn::Tooltip::new(
                    Element::clickable(
                        ids::MENTION_REPLY,
                        crate::strong_line(label, 12.0, if on { look.accent } else { look.muted })
                            .padding(6.0),
                    )
                    .radius(6.0)
                    .hover_background(look.hover)
                    .accessibility_label(tip),
                    ids::MENTION_REPLY,
                    tip,
                    look.menu_palette(),
                )
                .render()
                .centered_y()
            });
            let strip = Element::row(vec![
                gap(18.0, 0.0),
                said.flex(1.0).centered_y(),
                mention.unwrap_or_else(Element::spacer),
                IconButton::new(ids::CANCEL, Icon::CloseCircle, palette)
                    .variant(ButtonVariant::Ghost)
                    .tooltip(t("Cancelar respuesta"))
                    .tooltip_shortcut("Esc")
                    .render()
                    .centered_y(),
                gap(10.0, 0.0),
            ]);
            let strip = Element::panel(strip)
                .background(look.menu)
                .border(look.border, 1.0)
                .corner_radii(CornerRadii::top(8.0))
                .size(Some(view.width), Some(STRIP));
            Element::column(vec![strip, card])
        }
        None => card,
    };
    let (items, selected, title, open) = match view.completions {
        Some(completions) => {
            let items = completions
                .items
                .into_iter()
                .map(|completion| {
                    let face = completion.face.map(|(color, image)| {
                        crate::avatar_with_image(&completion.label, color, FACE, image)
                    });
                    let mut item = SuggestionItem::new(
                        format!("{}{}", ids::MENTION_PICK, completion.insert),
                        completion.label,
                    )
                    .description(completion.detail)
                    .detail(completion.aside)
                    .icon(completion.icon);
                    if let Some(face) = face {
                        item = item.leading(face);
                    }
                    if let Some(ink) = completion.ink {
                        item = item.ink(ink);
                    }
                    if let Some(section) = completion.section {
                        item = item.section(section);
                    }
                    item
                })
                .collect();
            (items, completions.selected, completions.title, true)
        }
        None => (Vec::new(), 0, String::new(), false),
    };
    let card = SuggestionList::new(
        card,
        ids::MENTIONS,
        items,
        selected,
        open,
        ids::MENTIONS_DISMISS,
        look.menu_palette(),
    )
    .title(title)
    .width(view.width)
    .render();
    let typing = match view.typing {
        Some(sentence) => Element::row(vec![
            icon(Icon::More, look.muted).centered_y(),
            line(sentence, 12.0, look.muted).flex(1.0).centered_y(),
        ])
        .gap(6.0),
        None => Element::spacer(),
    };
    // A silent message says so under the box, before who is typing (P4).
    let typing = if view.actions.silent {
        Element::row(vec![
            icon(Icon::BellSlash, look.muted).centered_y(),
            line(
                t("Mensaje silencioso: no le llega notificación a nadie"),
                12.0,
                look.muted,
            )
            .centered_y(),
            typing.flex(1.0),
        ])
        .gap(6.0)
    } else {
        typing
    };
    Element::column(vec![card, typing.size(None, Some(20.0))]).gap(2.0)
}

/// The «+» before the editor and its menu: what can be added to a message.
fn plus_menu(view: &ComposerView<'_>, enabled: bool, look: &Look) -> Element {
    let button = IconButton::new(ids::PLUS, Icon::Add, raised_palette(look))
        .variant(ButtonVariant::Ghost)
        .tooltip(t("Más opciones"))
        .enabled(enabled)
        .render();
    let mut upload =
        DropdownMenuItem::new(ids::ATTACH, t("Subir un archivo")).icon(Icon::Paperclip);
    let mut paste = DropdownMenuItem::new(ids::PASTE, t("Pegar del portapapeles")).icon(Icon::Copy);
    if let Some(reason) = &view.attach_denied {
        // The line under the label may be cut; the hint says it whole.
        upload = upload
            .enabled(false)
            .description(reason.clone())
            .tooltip_with(limn::TooltipSpec::new(reason.clone()));
        paste = paste.enabled(false);
    } else if let Some(hint) = &view.attach_hint {
        upload = upload
            .description(hint.clone())
            .tooltip_with(limn::TooltipSpec::new(hint.clone()));
    }
    let mut entries = vec![
        DropdownMenuEntry::Item(upload),
        DropdownMenuEntry::Item(paste.description(t("Archivos copiados o una imagen"))),
    ];
    // Message actions (P4).
    if let Some(denied) = &view.actions.poll {
        let mut poll = DropdownMenuItem::new(ids::POLL, t("Crear encuesta")).icon(Icon::ChartBar);
        if let Some(reason) = denied {
            poll = poll
                .enabled(false)
                .description(reason.clone())
                .tooltip_with(limn::TooltipSpec::new(reason.clone()));
        }
        entries.push(DropdownMenuEntry::Item(poll));
    }
    DropdownMenu::new(
        button,
        entries,
        view.plus_open && enabled,
        ids::PLUS_DISMISS,
        look.menu_palette(),
    )
    .context_menu_style()
    .width(300.0)
    .placement(PopoverPlacement::TopStart)
    .render()
}

/// The palette of controls on the composer's raised card.
fn raised_palette(look: &Look) -> limn::ComponentPalette {
    limn::ComponentPalette {
        surface: look.raised,
        raised: look.raised,
        ..look.palette()
    }
}

/// The sentence of who is typing: "Ada está escribiendo…",
/// "Ada y Bruno están escribiendo…", "Varias personas están escribiendo…".
pub fn typing_sentence(names: &[String]) -> Option<String> {
    match names {
        [] => None,
        [one] => Some(tr!("{one} está escribiendo…", one)),
        [one, two] => Some(tr!("{one} y {two} están escribiendo…", one, two)),
        [one, two, three] => Some(tr!(
            "{one}, {two} y {three} están escribiendo…",
            one,
            two,
            three
        )),
        _ => Some(t("Varias personas están escribiendo…").into()),
    }
}

/// What is being completed at the end of `text`: where its `@`, `#` or `:`
/// starts in bytes, which one it is, and what follows it. It starts a word
/// and nothing but a name follows it; an emoji code needs two letters.
pub fn trigger_at_end(text: &str) -> Option<(usize, char, &str)> {
    let at = text.rfind(['@', '#', ':'])?;
    let sigil = text[at..].chars().next()?;
    let query = &text[at + 1..];
    let starts_word = text[..at]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    let name = query
        .chars()
        .all(|character| character.is_alphanumeric() || matches!(character, '_' | '.' | '-'));
    let long_enough = sigil != ':' || query.chars().count() >= 2;
    (starts_word && name && long_enough && query.chars().count() <= 32)
        .then_some((at, sigil, query))
}

#[cfg(test)]
#[path = "composer_layout_tests.rs"]
mod layout_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    fn view(disabled: Option<String>) -> ComposerView<'static> {
        ComposerView {
            editor: "composer:1".into(),
            draft: String::new(),
            placeholder: "Enviar mensaje a #general".into(),
            width: 600.0,
            reply: Some(("Ada".into(), Color::rgb(46, 204, 113))),
            disabled,
            attach_denied: Some("Este proveedor no permite adjuntar archivos".into()),
            attach_hint: None,
            plus_open: false,
            typing: Some("Ada está escribiendo…".into()),
            emoji: None,
            completions: None,
            highlights: Vec::new(),
            actions: Default::default(),
        }
    }

    #[test]
    fn replies_switch_their_mention_and_silent_messages_say_so() {
        let shown = laid(ComposerView {
            actions: ComposerActions {
                mention_reply: Some(false),
                silent: true,
                poll: Some(None),
            },
            plus_open: true,
            ..view(None)
        });
        let painted = texts(&shown);
        assert!(
            painted.iter().any(|text| text == "@ DESACTIVADO"),
            "{painted:?}"
        );
        assert!(painted
            .iter()
            .any(|text| text.starts_with("Mensaje silencioso")));
        assert!(controls(&shown).contains(&ids::MENTION_REPLY.to_owned()));
        assert!(controls(&shown).contains(&ids::POLL.to_owned()));
        let plain = laid(view(None));
        assert!(!controls(&plain).contains(&ids::MENTION_REPLY.to_owned()));
    }

    fn laid(view: ComposerView<'_>) -> limn::Frame {
        frame(
            Element::column(vec![
                Element::spacer().flex(1.0),
                composer(view, &Look::DARK),
            ]),
            Size::new(600.0, 700.0),
        )
        .1
    }

    #[test]
    fn it_offers_its_editor_plus_emoji_and_cancel_unless_writing_is_not_allowed() {
        let open = laid(view(None));
        let ids = controls(&open);
        for id in ["composer:1", ids::PLUS, ids::CANCEL, ids::EMOJI] {
            assert!(ids.contains(&id.to_owned()), "{id} in {ids:?}");
        }
        assert!(
            texts(&open).iter().any(|text| text == "Ada"),
            "who is answered"
        );
        let closed = laid(view(Some("No puedes escribir aquí".into())));
        let ids = controls(&closed);
        assert!(
            !ids.contains(&"composer:1".to_owned()) && !ids.contains(&ids::PLUS.to_owned()),
            "{ids:?}"
        );
    }

    #[test]
    fn the_box_of_one_line_keeps_its_compact_minimum() {
        let laid = laid(ComposerView {
            reply: None,
            typing: None,
            ..view(None)
        });
        let editor = laid
            .regions
            .iter()
            .find(|region| region.id == "composer:1")
            .unwrap();
        assert!(
            (editor.rect.height - (MIN_HEIGHT - 2.0 * CARD_PADDING)).abs() < 1.0,
            "{:?}",
            editor.rect
        );
    }

    #[test]
    fn the_plus_menu_says_why_files_cannot_be_sent() {
        let laid = laid(ComposerView {
            plus_open: true,
            ..view(None)
        });
        let painted = texts(&laid);
        assert!(
            painted.iter().any(|text| text == "Subir un archivo"),
            "{painted:?}"
        );
        assert!(painted
            .iter()
            .any(|text| text.starts_with("Este proveedor no")));
        assert!(
            !controls(&laid).contains(&ids::ATTACH.to_owned()),
            "it cannot be chosen"
        );
    }

    #[test]
    fn completions_show_their_heading_and_insert_what_they_name() {
        let laid = laid(ComposerView {
            completions: Some(Completions {
                title: "MIEMBROS".into(),
                items: vec![
                    Completion {
                        insert: "@ada".into(),
                        label: "Ada".into(),
                        detail: "ada".into(),
                        icon: Icon::At,
                        section: None,
                        face: None,
                        ink: None,
                        aside: String::new(),
                    },
                    Completion {
                        insert: "@everyone".into(),
                        label: "@everyone".into(),
                        detail: "Notificar a todos".into(),
                        icon: Icon::At,
                        section: Some("OPCIONES".into()),
                        face: None,
                        ink: None,
                        aside: String::new(),
                    },
                ],
                selected: 0,
            }),
            ..view(None)
        });
        assert!(texts(&laid).iter().any(|text| text == "MIEMBROS"));
        assert!(controls(&laid).contains(&"composer:mention=@everyone".to_owned()));
    }

    #[test]
    fn a_trigger_is_a_sigil_starting_a_word_and_a_name_after_it() {
        assert_eq!(trigger_at_end("hola @ad"), Some((5, '@', "ad")));
        assert_eq!(trigger_at_end("@"), Some((0, '@', "")));
        assert_eq!(trigger_at_end("en #gen"), Some((3, '#', "gen")));
        assert_eq!(trigger_at_end("qué :he"), Some((5, ':', "he")));
        assert_eq!(
            trigger_at_end("qué :h"),
            None,
            "an emoji code needs two letters"
        );
        assert_eq!(trigger_at_end("a las 10:30"), None);
        assert_eq!(trigger_at_end("correo@ejemplo"), None, "inside a word");
        assert_eq!(
            trigger_at_end("@ada hola"),
            None,
            "a finished one is not completed"
        );
    }

    #[test]
    fn typing_names_up_to_three_people() {
        let names = |list: &[&str]| list.iter().map(|name| name.to_string()).collect::<Vec<_>>();
        assert_eq!(typing_sentence(&[]), None);
        assert_eq!(
            typing_sentence(&names(&["Ada"])).unwrap(),
            "Ada está escribiendo…"
        );
        assert_eq!(
            typing_sentence(&names(&["Ada", "Bruno"])).unwrap(),
            "Ada y Bruno están escribiendo…"
        );
        assert_eq!(
            typing_sentence(&names(&["a", "b", "c", "d"])).unwrap(),
            "Varias personas están escribiendo…"
        );
    }
}
