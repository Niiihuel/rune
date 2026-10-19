//! Public artwork requested by currently visible native components.

use crate::{timeline_id, Ui};
use limn::UiTree;
use rune_model::{ChannelId, RemoteImage, ServerId, UserId};
use rune_state::Store;
use std::collections::HashSet;

/// The host calls this after layout. Virtualized timelines contribute only
/// their visible rows; navigation uses hit regions already cut to the viewport.
pub fn wanted_images(ui: &Ui, store: &Store, tree: &UiTree) -> Vec<RemoteImage> {
    let mut wanted = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |source: Option<&RemoteImage>| {
        if wanted.len() >= 256 {
            return;
        }
        if let Some(source) =
            source.filter(|source| source.url.len() <= 2048 && seen.insert((*source).clone()))
        {
            wanted.push(source.clone());
        }
    };
    // The remembered accounts' pictures, where they are listed.
    if crate::auth::visible(store) || ui.settings.is_some() {
        for account in &store.accounts {
            add(account.avatar.as_ref());
        }
    }
    if let Some(me) = store.me_user() {
        add(me.picture());
        if ui.status_menu || ui.profiles.editor.is_some() {
            add(me.banner.image());
            // The popout's, from the full profile when the identity had none.
            add(crate::profile::banner(me, store.profile(me.id, None)));
            // What a picture taken away in the editor leaves.
            add(me.default_avatar.as_ref());
        }
    }
    // What the profile shown carries: activities, badges, marks (P5).
    crate::identity::wanted(ui, store, &mut add);
    // Scheduled events (eventos-programados): the cover of the one shown.
    crate::events::wanted(ui, store, &mut add);
    if let Some(id) = ui.profile {
        if let Some(profile) = store.user(id) {
            add(store.avatar_in(ui.server, id));
            let loaded = store.profile(id, ui.server);
            add(crate::profile::banner(profile, loaded));
        }
    }
    if let Some((id, server)) = ui.profiles.full {
        if let Some(profile) = store.user(id) {
            add(store.avatar_in(server, id));
            add(crate::profile::banner(profile, store.profile(id, server)));
        }
        // The faces of the tab shown: the provider bounds both lists.
        let loaded = store.profile(id, server);
        match ui.profiles.full_tab {
            rune_components::profile::FullTab::Servers => {
                for server in loaded
                    .and_then(|loaded| loaded.mutual_servers.as_ref())
                    .into_iter()
                    .flatten()
                {
                    add(store
                        .server(*server)
                        .and_then(|server| server.icon.as_ref()));
                }
            }
            rune_components::profile::FullTab::Friends => {
                for friend in loaded
                    .and_then(|loaded| loaded.mutual_friends.as_ref())
                    .into_iter()
                    .flatten()
                {
                    add(store.user(*friend).and_then(|friend| friend.picture()));
                }
            }
            rune_components::profile::FullTab::About => {}
        }
    }
    if let Some(author) = ui.profiles.author.and_then(|(message, _)| {
        store
            .timeline(ui.channel()?)?
            .get(message)
            .and_then(|message| store.user(message.author))
    }) {
        add(store.avatar_in(ui.server, author.id));
        add(crate::profile::banner(
            author,
            store.profile(author.id, ui.server),
        ));
    }
    // A voice channel's card: the face and banner the others' cards ask for.
    if let Some((user, channel, _)) = ui.voice.card {
        let server = store.channel(channel).and_then(|found| found.server);
        if let Some(account) = store.user(user) {
            add(store.avatar_in(server, user));
            add(crate::profile::banner(account, store.profile(user, server)));
        }
    }
    // The pictures of the streams under their owners in the column.
    for image in crate::video::preview_images(ui, store) {
        add(Some(&image));
    }
    // The modal keeps its source wanted even if the timeline scrolls away.
    for source in crate::attachments::viewer_sources(ui, store, true) {
        add(Some(&source));
    }
    add(crate::invitations::image(ui, store));
    // The server settings' emoji or sticker page lists their pictures.
    if let Some(settings) = &ui.server_settings {
        let server = store.server(settings.server);
        let page = settings.page;
        for emoji in server
            .filter(|_| page == crate::server_settings::Page::Emojis)
            .map_or(&[][..], |server| &server.emojis)
        {
            add(emoji.image.as_ref());
        }
        for sticker in server
            .filter(|_| page == crate::server_settings::Page::Stickers)
            .map_or(&[][..], |server| &server.stickers)
        {
            add(sticker.image.as_ref());
        }
        // The banner on the profile's card (2026-10-19).
        add(server
            .filter(|_| page == crate::server_settings::Page::Profile)
            .and_then(|server| server.options.banner.as_ref()));
    }
    // The open group's picture, in its header and its dialog.
    add(ui
        .channel()
        .and_then(|channel| store.channel(channel))
        .and_then(|channel| channel.icon.as_ref()));
    // The emoji of the open conversation's topic (2026-10-17).
    for picture in ui
        .channel()
        .and_then(|channel| store.channel(channel))
        .map_or(&[][..], |channel| &channel.topic_pictures)
    {
        add(Some(&picture.image));
    }
    for emoji in pinned_emoji(ui, store) {
        add(Some(emoji));
    }
    // The provider's GIFs and their categories, while the tab shows them (P5).
    for picture in crate::gifs::pictures(ui, store) {
        add(Some(picture));
    }
    // Closing the picker must not cancel the selected sticker's preview.
    add(crate::expressions::staged_sticker(ui, store).and_then(|sticker| sticker.image.as_ref()));
    if let Some(channel) = ui.channel() {
        if let (Some(timeline), Some(rows)) = (
            store.timeline(channel),
            tree.visible_rows(&timeline_id(channel)),
        ) {
            let messages = timeline.messages();
            for message in messages.iter().take(rows.end).skip(rows.start) {
                add(store.avatar_in(ui.server, message.author));
                crate::identity::face_marks(ui, store, ui.server, message.author, false, &mut add);
                if let Some(reply) = message.reply_to.and_then(|id| timeline.get(id)) {
                    add(store.avatar_in(ui.server, reply.author));
                    // The emoji of the line it answers.
                    for emoji in text_emoji(&reply.text_references, false) {
                        add(Some(emoji));
                    }
                }
                for sticker in &message.stickers {
                    add(sticker.image.as_ref());
                }
                for emoji in emoji_pictures(message) {
                    add(Some(emoji));
                }
                for image in crate::invitation_cards::images(ui, store, message) {
                    add(Some(image));
                }
                if ui.prefs.image_previews {
                    // An explicit picture the account's filter hides loads
                    // blurred; its ThumbHash stands in until the picture is
                    // here (contenido-de-mensajes).
                    let obscure = store.obscures_explicit(channel, message.author);
                    // shortcut: a message made of parts may show a file only
                    // through a part, but its previews still load with the row.
                    for attachment in message.files() {
                        if crate::attachments::hidden(ui, attachment) {
                            continue;
                        }
                        let Some(preview) = &attachment.preview else {
                            continue;
                        };
                        let shown = if obscure && crate::attachments::unrevealed(ui, attachment) {
                            preview.obscured()
                        } else {
                            preview.clone()
                        };
                        if !ui.images.contains_key(&shown) {
                            add(attachment.placeholder.as_ref());
                        }
                        add(Some(&shown));
                    }
                    for source in card_sources(ui, message, obscure) {
                        add(Some(&source));
                    }
                    let revealed = ui.messages.revealed.get(&message.id);
                    let copy = message.forwarded.as_ref();
                    let mut lists = vec![(&message.components.parts, false, None)];
                    lists.extend(copy.map(|copy| (&copy.components.parts, true, None)));
                    for (forwarded, embeds) in [
                        (false, Some(&message.embeds)),
                        (true, copy.map(|copy| &copy.embeds)),
                    ] {
                        for (index, embed) in embeds.into_iter().flatten().enumerate() {
                            lists.push((&embed.components, forwarded, Some(index)));
                        }
                    }
                    // An app's pictures behind a spoiler wait for their own
                    // reveal; explicit ones the filter hides load blurred.
                    for (parts, forwarded, card) in lists {
                        let first = rune_components::message::PARTS_SPOILER
                            + rune_components::message::first_spoiler(forwarded, card);
                        let shown = |n| revealed.is_some_and(|shown| shown.contains(&(first + n)));
                        for picture in rune_model::components::pictures(parts, obscure, shown) {
                            add(Some(&picture));
                        }
                    }
                }
            }
        }
    }
    // The faces in the hint of the server under the pointer, as many as
    // it shows (2026-10-17).
    if let Some(server) = tree
        .hovered_id()
        .and_then(|id| id.strip_prefix("server:"))
        .and_then(|id| id.parse().ok())
        .map(ServerId)
    {
        if let Some(voice) = store.voice_by_server().get(&server) {
            for state in voice
                .people
                .iter()
                .take(2 * rune_components::rail::VOICE_SHOWN)
            {
                add(store.avatar_in(Some(server), state.user));
            }
        }
    }
    for region in &tree.frame().regions {
        // The picker's cells: any server's emoji and stickers, and the
        // packs' (P5); the picker offers only what it may.
        if ui.emoji_open {
            let base = rune_components::composer::ids::EMOJI;
            if let Some(id) = region
                .id
                .strip_prefix(&format!("{base}:custom="))
                .and_then(|id| id.parse::<u64>().ok())
            {
                add(store
                    .servers()
                    .iter()
                    .flat_map(|server| &server.emojis)
                    .find(|emoji| emoji.id.0 == id)
                    .and_then(|emoji| emoji.image.as_ref()));
            }
            if let Some(id) = region
                .id
                .strip_prefix(&format!("{base}:sticker="))
                .and_then(|id| id.parse::<u64>().ok())
            {
                add(store
                    .known_sticker(rune_model::StickerId(id))
                    .and_then(|(_, sticker)| sticker.image.as_ref()));
            }
        }
        // Someone in a voice channel of the server shown, in the column or
        // on the stage (2026-10-17: only their initial was drawn).
        if let Some(user) = region
            .id
            .strip_prefix("vz:user:")
            .or_else(|| region.id.strip_prefix("vz:tile:"))
            .and_then(|id| id.parse::<u64>().ok())
        {
            add(store.avatar_in(ui.server, UserId(user)));
            // A tile draws their decoration too (2026-10-18).
            if region.id.starts_with("vz:tile:") {
                let marks = crate::identity::identity(ui, store, ui.server, UserId(user));
                add(marks.decoration.as_ref());
            }
            continue;
        }
        // The emoji of the custom status under a name in the lists of
        // members, conversations and friends (2026-10-18).
        let listed = match region.id.split_once(':') {
            Some(("member", id)) => id.parse().ok(),
            Some(("channel", id)) => id
                .parse()
                .ok()
                .and_then(|id| store.channel(ChannelId(id)))
                .filter(|channel| channel.kind == rune_model::ChannelKind::Direct)
                .and_then(|channel| channel.recipients.first().map(|user| user.0)),
            Some(("friend", rest)) => rest.split_once(':').and_then(|(id, _)| id.parse().ok()),
            _ => None,
        };
        if let Some(user) = listed {
            add(store
                .presence(UserId(user))
                .custom_emoji
                .as_ref()
                .map(|emoji| &emoji.image));
        }
        let Some((kind, id)) = region.id.split_once(':') else {
            continue;
        };
        let Ok(id) = id.parse::<u64>() else {
            continue;
        };
        match kind {
            "server" => add(store
                .server(ServerId(id))
                .and_then(|server| server.icon.as_ref())),
            "channel" => {
                if let Some(channel) = store
                    .channel(ChannelId(id))
                    .filter(|channel| channel.server.is_none())
                {
                    // A group's own picture; a person's face otherwise.
                    add(if channel.kind == rune_model::ChannelKind::Group {
                        channel.icon.as_ref()
                    } else {
                        channel
                            .recipients
                            .first()
                            .and_then(|id| store.user(*id))
                            .and_then(|user| user.picture())
                    });
                }
            }
            "member" => {
                add(store.avatar_in(ui.server, UserId(id)));
                crate::identity::face_marks(ui, store, ui.server, UserId(id), true, &mut add);
            }
            // A forum's card in view: its first message's picture (2026-10-18),
            // not a blocked author's.
            "post" if ui.prefs.image_previews => add(store
                .channel(ChannelId(id))
                .and_then(|post| post.thread.as_ref())
                .filter(|info| !info.owner.is_some_and(|owner| store.blocked(owner)))
                .and_then(|info| info.starter.as_ref()?.image.as_ref())),
            _ => {}
        }
    }
    wanted
}

/// The pictures of a message's cards and of its forward's, a GIF's clip
/// too, but none behind a spoiler not revealed yet; behind the account's
/// explicit content filter (`obscure`), the first one blurred.
fn card_sources(ui: &Ui, message: &rune_model::Message, obscure: bool) -> Vec<RemoteImage> {
    let revealed = ui.messages.revealed.get(&message.id);
    let copy = message.forwarded.as_ref();
    let mut found = Vec::new();
    for (forwarded, embeds, content) in [
        (false, Some(&message.embeds), &message.content),
        (
            true,
            copy.map(|copy| &copy.embeds),
            copy.map_or(&message.content, |copy| &copy.content),
        ),
    ] {
        let embeds = embeds.map_or(&[][..], Vec::as_slice);
        for (first, count) in rune_model::card_groups(embeds) {
            let reveal = rune_components::message::card_spoiler(forwarded, first);
            let shown = revealed.is_some_and(|shown| shown.contains(&reveal));
            if embeds[first].hides_pictures(content) && !shown {
                continue;
            }
            if obscure && !shown && rune_components::message::card_explicit(embeds, first, count) {
                found.extend(
                    embeds[first]
                        .picture()
                        .map(|picture| picture.source.obscured()),
                );
                continue;
            }
            for embed in &embeds[first..first + count] {
                for picture in [&embed.image, &embed.thumbnail, &embed.video] {
                    found.extend(picture.as_ref().map(|picture| picture.source.clone()));
                }
            }
        }
    }
    found
}

/// The pictures of the server emoji a text names (2026-10-17): their
/// larger ones in a message of emoji alone (`jumbo`, 2026-10-18).
pub(crate) fn text_emoji(
    references: &[rune_model::TextReference],
    jumbo: bool,
) -> impl Iterator<Item = &RemoteImage> {
    references
        .iter()
        .filter_map(move |reference| reference.target.emoji_picture(jumbo))
}

/// The emoji of the pinned messages, while their popout is open (2026-10-18).
fn pinned_emoji<'a>(ui: &Ui, store: &'a Store) -> impl Iterator<Item = &'a RemoteImage> {
    ui.channel_list
        .pins
        .and_then(|channel| store.pins(channel))
        .unwrap_or_default()
        .iter()
        .flat_map(|message| text_emoji(&message.text_references, false))
}

/// Whether `message` is emoji alone, drawn with their larger pictures.
pub(crate) fn jumbo(message: &rune_model::Message) -> bool {
    rune_components::text::only_emoji(&message.content, &message.text_references)
}

/// The pictures of server emoji a message's text, its cards and an app's
/// parts show, its forward's too: not previews, they load as stickers do.
pub(crate) fn emoji_pictures(message: &rune_model::Message) -> Vec<&RemoteImage> {
    let copy = message.forwarded.as_ref();
    let cards = message
        .embeds
        .iter()
        .chain(copy.into_iter().flat_map(|copy| &copy.embeds));
    let mut found: Vec<&RemoteImage> =
        text_emoji(&message.text_references, jumbo(message)).collect();
    // The forward's own text names them too, at a line's size (contenido-de-mensajes).
    found.extend(
        copy.into_iter()
            .flat_map(|copy| text_emoji(&copy.text_references, false)),
    );
    for card in cards {
        if card.kind == rune_model::EmbedKind::PollResult {
            found.extend(card.thumbnail.as_ref().map(|image| &image.source));
        }
        found.extend(rune_model::components::emoji(&card.components));
    }
    found.extend(rune_model::components::emoji(&message.components.parts));
    if let Some(copy) = copy {
        found.extend(rune_model::components::emoji(&copy.components.parts));
    }
    found
}

/// The images that may animate now: stickers and attachment previews in the
/// timeline's visible rows, or the one picture an open viewer shows. Nothing
/// plays with reduced motion nor behind a document; scrolled-away rows stop.
pub fn playable_images(
    ui: &Ui,
    store: &Store,
    tree: &UiTree,
    reduced_motion: bool,
) -> HashSet<RemoteImage> {
    let mut playable = HashSet::new();
    if reduced_motion || ui.document_viewer.is_some() {
        return playable;
    }
    // The viewer covers the conversation: only the picture it shows plays.
    if ui.attachment_viewer.is_some() {
        playable.extend(crate::attachments::viewer_sources(ui, store, false));
        return playable;
    }
    // The provider's GIFs play in their tab (P5).
    playable.extend(crate::gifs::pictures(ui, store).into_iter().cloned());
    // A decoration and an effect play on the profile shown.
    playable.extend(crate::identity::playing(ui, store));
    let Some(channel) = ui.channel() else {
        return playable;
    };
    // Its topic's emoji that move, in the header.
    playable.extend(
        store
            .channel(channel)
            .into_iter()
            .flat_map(|found| &found.topic_pictures)
            .map(|picture| picture.image.clone()),
    );
    // Those of the pinned messages that move, while they show (2026-10-18).
    playable.extend(pinned_emoji(ui, store).cloned());
    let (Some(timeline), Some(rows)) = (
        store.timeline(channel),
        tree.visible_rows(&timeline_id(channel)),
    ) else {
        return playable;
    };
    for message in timeline.messages().iter().take(rows.end).skip(rows.start) {
        playable.extend(
            message
                .stickers
                .iter()
                .filter_map(|sticker| sticker.image.clone()),
        );
        // A server's emoji that moves, in the text, its forward's, an app's
        // buttons and menus (limn-teclado-y-superficies) and the line it
        // answers.
        playable.extend(emoji_pictures(message).into_iter().cloned());
        if let Some(reply) = message.reply_to.and_then(|id| timeline.get(id)) {
            playable.extend(text_emoji(&reply.text_references, false).cloned());
        }
        if ui.prefs.image_previews {
            playable.extend(
                message
                    .files()
                    .filter(|attachment| !crate::attachments::hidden(ui, attachment))
                    .filter_map(|attachment| attachment.preview.clone()),
            );
            let obscure = store.obscures_explicit(channel, message.author);
            playable.extend(card_sources(ui, message, obscure));
        }
    }
    playable
}
