//! What message actions add to the composer: the reply's mention switch,
//! `@silent`, «Crear encuesta», and the formatting shortcuts that wrap the
//! selection in Markdown.

use std::time::Instant;

use limn::{Icon, UiAction};
use rune_components::composer::{ids as composer_ids, Completion, ComposerActions};
use rune_i18n::t;
use rune_model::ChannelId;
use rune_state::Store;

use crate::keybinds::Action;
use crate::{editor_id, Intent, Ui};

pub(crate) fn update(ui: &mut Ui, _store: &mut Store, action: &UiAction) -> Option<Vec<Intent>> {
    match action {
        UiAction::Clicked { id } if id == composer_ids::MENTION_REPLY => {
            let actions = &mut ui.message_actions;
            actions.mention_reply = !actions.mention_reply;
            Some(
                ui.channel()
                    .map(|channel| vec![Intent::Focus(editor_id(channel))])
                    .unwrap_or_default(),
            )
        }
        _ => None,
    }
}

/// What the composer of `channel` shows of message actions, for `draft`.
pub(crate) fn composer_actions(
    ui: &Ui,
    store: &Store,
    channel: ChannelId,
    draft: &str,
) -> ComposerActions {
    let capabilities = store
        .provider
        .as_ref()
        .map(|info| info.capabilities)
        .unwrap_or_default();
    let poll = store.abilities(channel).and_then(|abilities| {
        capabilities
            .create_polls
            .then(|| abilities.create_poll.reason().map(str::to_owned))
    });
    // With files too: `Command::SendFiles` carries it (envios-y-subidas).
    ComposerActions {
        mention_reply: (ui.reply_to.is_some() && capabilities.reply_mentions)
            .then_some(ui.message_actions.mention_reply),
        silent: capabilities.silent_messages && rune_provider::without_silent(draft).is_some(),
        poll,
    }
}

/// Whether a reply sent now notifies who it answers.
pub(crate) fn submit_options(ui: &Ui) -> bool {
    ui.message_actions.mention_reply
}

/// `@silent` among the completions of `@` at the start of a draft, when
/// the provider takes it.
pub(crate) fn silent_completion(store: &Store, at: usize, query: &str) -> Option<Completion> {
    let silent = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.silent_messages);
    let name = rune_provider::SILENT.trim_start_matches('@');
    (silent && at == 0 && name.starts_with(&query.to_lowercase())).then(|| Completion {
        insert: rune_provider::SILENT.into(),
        label: rune_provider::SILENT.into(),
        detail: t("Enviar sin notificar a nadie").into(),
        icon: Icon::BellSlash,
        section: None,
        face: None,
        ink: None,
        aside: String::new(),
    })
}

/// The Markdown each formatting shortcut wraps the selection in, as the
/// official client and Serein do; its key is the table's
/// (atajos-y-navegacion).
fn markers(action: Action) -> Option<(&'static str, &'static str)> {
    Some(match action {
        Action::Bold => ("**", "**"),
        Action::Italic => ("*", "*"),
        Action::Underline => ("__", "__"),
        Action::InlineCode => ("`", "`"),
        Action::Strikethrough => ("~~", "~~"),
        Action::CodeBlock => ("```\n", "\n```"),
        Action::Spoiler => ("||", "||"),
        _ => return None,
    })
}

/// Wrap the composer's selection of `channel`, which has the focus, for a
/// formatting shortcut.
pub(crate) fn format(
    ui: &mut Ui,
    channel: ChannelId,
    action: Action,
    now: Instant,
) -> Option<Vec<Intent>> {
    let (open, close) = markers(action)?;
    Some(crate::composer_selection::wrap(
        ui, channel, open, close, now,
    ))
}
