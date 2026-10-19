//! A composer's selection is valid only for the exact channel and source text
//! observed in limn. Positions count Unicode scalar values, never UTF-8 bytes.

use std::time::Instant;

use limn::UiTree;
use rune_i18n::t;
use rune_model::ChannelId;

use crate::{editor_id, Intent, Ui};

#[derive(Debug)]
pub(crate) struct ComposerSelection {
    channel: ChannelId,
    draft: String,
    range: (usize, usize),
}

pub(crate) fn capture(ui: &mut Ui, tree: &UiTree) {
    let Some(channel) = ui.channel() else {
        ui.messages.selection = None;
        return;
    };
    let id = editor_id(channel);
    let draft = ui.drafts.get(&channel).map_or("", String::as_str);
    let range = tree.input_selection(&id).filter(|(start, end)| {
        start <= end
            && *end <= draft.chars().count()
            && tree.input_value(&id) == Some(draft)
            && tree.frame().regions.iter().any(|region| region.id == id)
    });
    let Some(range) = range else {
        ui.messages.selection = None;
        return;
    };
    match &mut ui.messages.selection {
        Some(saved) if saved.channel == channel && saved.draft == draft => saved.range = range,
        saved => {
            *saved = Some(ComposerSelection {
                channel,
                draft: draft.to_owned(),
                range,
            })
        }
    }
}

pub(crate) fn insert(
    ui: &mut Ui,
    channel: ChannelId,
    text: &str,
    separated: bool,
    now: Instant,
) -> Vec<Intent> {
    let old = ui.drafts.get(&channel).map_or("", String::as_str);
    let count = old.chars().count();
    let (start, end) = ui
        .messages
        .selection
        .as_ref()
        .filter(|saved| {
            saved.channel == channel
                && saved.draft == old
                && saved.range.0 <= saved.range.1
                && saved.range.1 <= count
        })
        .map_or((count, count), |saved| saved.range);
    let offset = |index| {
        old.char_indices()
            .nth(index)
            .map_or(old.len(), |(offset, _)| offset)
    };
    let before = &old[..offset(start)];
    let after = &old[offset(end)..];
    let lead = if separated && !before.is_empty() && !before.ends_with(char::is_whitespace) {
        " "
    } else {
        ""
    };
    let tail = if separated && !after.is_empty() && !after.starts_with(char::is_whitespace) {
        " "
    } else {
        ""
    };
    let value = format!("{before}{lead}{text}{tail}{after}");
    if value.encode_utf16().count() > 2000 {
        ui.notify(
            t("El borrador es demasiado largo"),
            Some(t("Dejá espacio antes de insertar este elemento.").into()),
            true,
            now,
        );
        return Vec::new();
    }
    let cursor = start + lead.chars().count() + text.chars().count() + tail.chars().count();
    ui.messages.selection = Some(ComposerSelection {
        channel,
        draft: value.clone(),
        range: (cursor, cursor),
    });
    ui.set_draft(channel, Some(value.clone()));
    ui.emoji_open = false;
    vec![
        Intent::SetInput {
            id: editor_id(channel),
            value,
        },
        Intent::FocusInput {
            id: editor_id(channel),
            cursor,
        },
    ]
}

// Message actions (P4).
/// Wrap the composer's selection in `open` and `close`, or take them away
/// when they already wrap it, inside or just outside; with nothing
/// selected, an empty pair goes in at the caret, with the caret between.
/// What was selected stays selected.
pub(crate) fn wrap(
    ui: &mut Ui,
    channel: ChannelId,
    open: &str,
    close: &str,
    now: Instant,
) -> Vec<Intent> {
    let old = ui
        .drafts
        .get(&channel)
        .map_or("", String::as_str)
        .to_owned();
    let chars: Vec<char> = old.chars().collect();
    let count = chars.len();
    let (start, end) = ui
        .messages
        .selection
        .as_ref()
        .filter(|saved| {
            saved.channel == channel
                && saved.draft == old
                && saved.range.0 <= saved.range.1
                && saved.range.1 <= count
        })
        .map_or((count, count), |saved| saved.range);
    let piece = |from: usize, to: usize| chars[from..to].iter().collect::<String>();
    let (before, inside, after) = (piece(0, start), piece(start, end), piece(end, count));
    let (open_len, close_len) = (open.chars().count(), close.chars().count());
    let (value, range) = if inside.chars().count() >= open_len + close_len
        && inside.starts_with(open)
        && inside.ends_with(close)
        && start != end
    {
        let bare: String = inside
            .chars()
            .skip(open_len)
            .take(end - start - open_len - close_len)
            .collect();
        let length = bare.chars().count();
        (format!("{before}{bare}{after}"), (start, start + length))
    } else if before.ends_with(open) && after.starts_with(close) {
        let kept: String = before.chars().take(start - open_len).collect();
        let rest: String = after.chars().skip(close_len).collect();
        (
            format!("{kept}{inside}{rest}"),
            (start - open_len, end - open_len),
        )
    } else {
        (
            format!("{before}{open}{inside}{close}{after}"),
            (start + open_len, end + open_len),
        )
    };
    if value.encode_utf16().count() > 2000 {
        ui.notify(
            t("El formato no entra en el mensaje"),
            Some(t("Dejá espacio antes de darle formato.").into()),
            true,
            now,
        );
        return Vec::new();
    }
    ui.messages.selection = Some(ComposerSelection {
        channel,
        draft: value.clone(),
        range,
    });
    ui.set_draft(channel, Some(value.clone()));
    vec![
        Intent::SetInput {
            id: editor_id(channel),
            value,
        },
        Intent::SelectInput {
            id: editor_id(channel),
            start: range.0,
            end: range.1,
        },
    ]
}
