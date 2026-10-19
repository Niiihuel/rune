//! The card of an emoji clicked in a message, or of a reaction's on a
//! secondary click (2026-10-18), as Serein shows it: what it is, the
//! server a server's comes from, and «Copiar emoji», which copies it as
//! written (a server's token pastes back as that emoji).

use limn::{Element, Point, UiAction, UiTree};
use rune_components::{emoji_card as parts, message, Look};
use rune_model::{ChannelId, MessageId, TextTarget};
use rune_state::Store;

use crate::{Intent, Ui};

/// The card open.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Open {
    /// The region it stands under: the emoji's, or the reaction's.
    anchor: String,
    channel: ChannelId,
    message: MessageId,
    /// The emoji as written: a server's token, or a Unicode sequence.
    emoji: String,
    /// Under the anchor, once laid out.
    at: Option<Point>,
}

/// Open, copy or close the card.
pub(crate) fn update(ui: &mut Ui, action: &UiAction) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    match id.as_str() {
        parts::ids::DISMISS => {
            ui.emoji_card = None;
            return Some(vec![Intent::Repaint]);
        }
        parts::ids::COPY => {
            let open = ui.emoji_card.take()?;
            return Some(vec![Intent::Copy(open.emoji), Intent::Repaint]);
        }
        _ => {}
    }
    let (message, what) = id.strip_prefix("msg:")?.split_once(':')?;
    let (_, emoji) = what.strip_prefix(message::EMOJI)?.split_once(':')?;
    ui.emoji_card = Some(Open {
        anchor: id.clone(),
        channel: ui.channel()?,
        message: MessageId(message.parse().ok()?),
        emoji: emoji.to_owned(),
        at: None,
    });
    Some(vec![Intent::Repaint])
}

/// A secondary click on a reaction (`react:{message}:{emoji}`): its
/// emoji's card, where the click was.
pub(crate) fn secondary_click(ui: &mut Ui, target: &str, at: Point) -> Option<Vec<Intent>> {
    let (message, emoji) = target.strip_prefix("react:")?.split_once(':')?;
    ui.emoji_card = Some(Open {
        anchor: target.to_owned(),
        channel: ui.channel()?,
        message: MessageId(message.parse().ok()?),
        emoji: emoji.to_owned(),
        at: Some(at),
    });
    Some(vec![Intent::Repaint])
}

/// After a layout: where the card stands, under its emoji, kept while it
/// is open (the rows move as messages arrive); a frame more shows it there.
/// Another conversation, or its message gone, closes it (2026-10-18: it
/// came back where it stood, or stayed open unseen).
pub(crate) fn hold(ui: &mut Ui, store: &Store, tree: &UiTree) -> Option<Intent> {
    let gone = ui.emoji_card.as_ref().is_some_and(|open| {
        ui.channel() != Some(open.channel)
            || store
                .timeline(open.channel)
                .and_then(|timeline| timeline.get(open.message))
                .is_none()
    });
    if gone {
        ui.emoji_card = None;
        return Some(Intent::Repaint);
    }
    let open = ui.emoji_card.as_mut().filter(|open| open.at.is_none())?;
    let buttons = || {
        tree.frame()
            .regions
            .iter()
            .filter(|region| region.kind == limn::HitKind::Button)
    };
    // The emoji clicked, else the first of its message that is the same
    // emoji, whatever its number.
    let same = format!("msg:{}:{}", open.message, message::EMOJI);
    open.at = buttons()
        .find(|region| region.id == open.anchor)
        .or_else(|| {
            buttons().find(|region| {
                region
                    .id
                    .strip_prefix(&same)
                    .and_then(|rest| rest.split_once(':'))
                    .is_some_and(|(_, emoji)| emoji == open.emoji)
            })
        })
        .map(|region| Point::new(region.rect.x, region.rect.y + region.rect.height));
    open.at.map(|_| Intent::Repaint)
}

/// The card over `list`, the open conversation's, once it has its place.
pub(crate) fn over(ui: &Ui, store: &Store, list: Element, look: &Look) -> Element {
    let Some((open, at)) = ui
        .emoji_card
        .as_ref()
        .filter(|open| ui.channel() == Some(open.channel))
        .and_then(|open| Some((open, open.at?)))
    else {
        return list;
    };
    let card = card_of(ui, store, open).map(|card| parts::card(&card, look));
    parts::popout(list, card, at, look).flex(1.0)
}

/// What the card says of the emoji open: a server's by the message's
/// reference to it and the catalog that has it, a Unicode one by its name.
fn card_of(ui: &Ui, store: &Store, open: &Open) -> Option<parts::EmojiCard> {
    let message = store.timeline(open.channel)?.get(open.message)?;
    let reference = message
        .text_references
        .iter()
        .find(|reference| reference.source == open.emoji);
    if let Some(reference) = reference {
        let TextTarget::Emoji { name, id, .. } = &reference.target else {
            return None;
        };
        // Its catalog's, by its id: the token keeps the name it was
        // written with (2026-10-18: a renamed emoji was not found).
        let known = (*id).and_then(|id| {
            store.servers().iter().find_map(|server| {
                Some((server, server.emojis.iter().find(|emoji| emoji.id == id)?))
            })
        });
        // The picture the message shows: its larger one when it is emoji alone.
        let image = reference
            .target
            .emoji_picture(crate::images::jumbo(message));
        return Some(parts::EmojiCard {
            name: format!(
                ":{}:",
                known.map_or(name.as_str(), |(_, emoji)| emoji.name.as_str())
            ),
            image: ui.picture(image),
            custom: true,
            server: known.map(|(server, _)| server.name.clone()),
        });
    }
    Some(parts::EmojiCard {
        name: rune_components::emoji::code(&open.emoji),
        image: rune_components::twemoji::image(&open.emoji),
        custom: false,
        server: None,
    })
}
