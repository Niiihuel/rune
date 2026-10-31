//! The open conversation's messages, as a measured list: only the rows in
//! view are built, each from the shared snapshot of the timeline, so a
//! conversation of thousands of messages costs what a screenful costs.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use limn::{Component, Element, ScrollArea, ScrollMotion, ScrollbarVisibility};
use rune_components::message::{
    group_starts_at, live_row, Abilities, Author, Live, MessageStyle, MessageView, Reply,
};
use rune_components::text::{self, Names};
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_model::{
    permissions, ChannelId, Message, MessageId, Permissions, ServerId, Timestamp, UserId,
};
use rune_state::Store;

use crate::{timeline_id, Ui};

/// A guess at a row's height before it is measured.
const ESTIMATE: f32 = 48.0;

/// How `user` shows as an author in `server`: their name there, the color of
/// their top role when the account wants it, their accent.
pub(crate) fn author(
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    user: UserId,
    look: &Look,
) -> Author {
    let Some(found) = store.user(user) else {
        return Author {
            marks: Default::default(),
            image: None,
            name: t("desconocido").into(),
            ink: look.muted,
            accent: look.faint,
            bot: false,
        };
    };
    let ink = server
        .and_then(|server| store.top_role(user, server))
        .and_then(|role| role.color)
        .filter(|_| ui.prefs.role_colors)
        .map_or(look.text, |color| look.ink(color));
    Author {
        image: ui.picture(store.avatar_in(server, user)),
        name: store.display_name(user, server),
        ink,
        accent: look.ink(found.accent),
        bot: found.bot,
        marks: crate::identity::marks(ui, store, server, user, look),
    }
}

/// Why a run of messages folds: its author is blocked, or only ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fold {
    Blocked,
    Ignored,
}

/// What every row of the list reads, shared by the closure that builds them.
struct Rows {
    pending_files: HashMap<MessageId, Vec<rune_model::PendingAttachment>>,
    upload_progress: HashMap<MessageId, (u64, u64)>,
    attachment_images: HashMap<rune_model::RemoteImage, limn::RasterImage>,
    attachment_failures: HashSet<rune_model::RemoteImage>,
    sticker_images: HashMap<rune_model::RemoteImage, limn::RasterImage>,
    messages: Arc<Vec<Arc<Message>>>,
    authors: HashMap<UserId, Author>,
    me: Option<UserId>,
    new_marker: Option<MessageId>,
    /// The marker was put by hand: it shows before the account's own messages too.
    marked_by_hand: bool,
    reacting: Option<MessageId>,
    highlighted: Option<MessageId>,
    /// The card of the author of a message, open from their mark or name.
    author_card: Option<(MessageId, bool, rune_components::profile::Profile)>,
    reached_start: bool,
    start: (String, String),
    /// The start's «Editar canal», when the account may.
    edit_channel: Option<String>,
    invitation_denied: Option<String>,
    invitation_cards: HashMap<MessageId, Vec<rune_components::message::InvitationCard>>,
    vote_denied: Option<String>,
    poll_choice: Option<(MessageId, Vec<u32>)>,
    /// Why the account may not join each voice channel a card names.
    voice_channels: HashMap<ChannelId, Option<String>>,
    animated: HashSet<String>,
    revealed_files: HashSet<rune_model::AttachmentId>,
    voice: Option<crate::VoicePlayer>,
    voice_speed: usize,
    /// Where each voice message was left, in seconds.
    voice_left: Vec<(rune_model::AttachmentId, f32)>,
    can: Abilities,
    manage: bool,
    names: Names,
    editing: Option<(MessageId, String)>,
    more: Option<MessageId>,
    recent: Vec<String>,
    query: String,
    revealed: HashMap<MessageId, Vec<usize>>,
    width: f32,
    now: Timestamp,
    look: Look,
    style: MessageStyle,
    /// The loaded authors the account blocked: their messages fold.
    blocked: HashSet<UserId>,
    /// The loaded authors the account ignores: their messages fold too.
    ignored: HashSet<UserId>,
    /// Runs of blocked messages shown anyway, by their first message.
    blocked_shown: HashSet<MessageId>,
    /// What apps' parts may do (P3).
    app: Option<crate::interactions::AppRows>,
    // limn adoption (2026-10-18).
    context: Option<(MessageId, limn::Point)>,
    // Message content (contenido-de-mensajes).
    /// The loaded authors whose explicit pictures the account's filter hides here.
    obscured: HashSet<UserId>,
    // Rail and notifications.
    /// The messages that tell the account through its roles.
    to_my_roles: HashSet<MessageId>,
    // limn-teclado-y-superficies.
    /// Shift is held: the bars over messages show their direct actions.
    shift: bool,
}

impl Rows {
    /// The server emoji `references` name, as pictures to draw in a line.
    fn text_pictures(
        &self,
        references: &[rune_model::TextReference],
    ) -> Vec<rune_components::identity::TextPicture> {
        references
            .iter()
            .filter_map(|reference| match &reference.target {
                rune_model::TextTarget::Emoji { name, image, .. } => {
                    let image = image
                        .as_ref()
                        .and_then(|image| self.attachment_images.get(image))
                        .cloned();
                    Some(rune_components::identity::TextPicture {
                        token: reference.source.clone(),
                        name: name.clone(),
                        playing: image
                            .as_ref()
                            .is_some_and(|image| self.animated.contains(image.sha256())),
                        image,
                    })
                }
                _ => None,
            })
            .collect()
    }

    /// How the message at `index` folds: its author is blocked, or only ignored.
    fn fold(&self, index: usize) -> Option<Fold> {
        let author = self.messages[index].author;
        if self.blocked.contains(&author) {
            Some(Fold::Blocked)
        } else if self.ignored.contains(&author) {
            Some(Fold::Ignored)
        } else {
            None
        }
    }

    /// Where the run of folded messages holding `index` starts, how many it
    /// has, and how it folds; `None` when the message does not fold. A run
    /// is of one kind: blocked and ignored people never share one.
    fn folded_run(&self, index: usize) -> Option<(Fold, usize, usize)> {
        let fold = self.fold(index)?;
        let mut first = index;
        while first > 0 && self.fold(first - 1) == Some(fold) {
            first -= 1;
        }
        let mut end = index + 1;
        while end < self.messages.len() && self.fold(end) == Some(fold) {
            end += 1;
        }
        Some((fold, first, end - first))
    }

    /// A row of the list: a folded message folds into the line of its run,
    /// as the official client folds them, until it is shown.
    fn row(&self, index: usize) -> Element {
        let Some((fold, first, count)) = self.folded_run(index) else {
            return self.build(index);
        };
        let id = self.messages[first].id;
        let shown = self.blocked_shown.contains(&id);
        let line = (index == first).then(|| match fold {
            Fold::Blocked => rune_components::blocked::run(id, count, shown, &self.look),
            Fold::Ignored => rune_components::blocked::ignored_run(id, count, shown, &self.look),
        });
        match (line, shown) {
            (Some(line), true) => Element::column(vec![line, self.build(index)]),
            (Some(line), false) => line,
            (None, true) => self.build(index),
            (None, false) => Element::spacer().size(None, Some(0.0)),
        }
    }

    fn author(&self, user: UserId) -> Author {
        self.authors.get(&user).cloned().unwrap_or(Author {
            marks: Default::default(),
            image: None,
            name: t("desconocido").into(),
            ink: self.look.muted,
            accent: self.look.faint,
            bot: false,
        })
    }

    fn continued(&self, index: usize) -> bool {
        // shortcut: a run of one author longer than this starts a new group
        // where it is cut; raise it if someone posts hundreds of messages in minutes.
        !group_starts_at(&self.messages, index, 200)
    }

    fn build(&self, index: usize) -> Element {
        let message = &self.messages[index];
        let previous = index
            .checked_sub(1)
            .map(|previous| &self.messages[previous]);
        let continued = self.continued(index);
        let day = match previous {
            Some(previous)
                if rune_components::time::day(previous.timestamp)
                    == rune_components::time::day(message.timestamp) =>
            {
                None
            }
            _ => Some(rune_components::time::day_label(
                message.timestamp,
                self.now,
            )),
        };
        // The marker sits on the first message after the last one read.
        let first_new = self.new_marker.is_some_and(|marker| {
            message.id >= marker
                && previous.is_none_or(|previous| previous.id < marker)
                && (self.marked_by_hand || Some(message.author) != self.me)
        });
        let reply = message.reply_to.map(|answered| {
            let original = self
                .messages
                .binary_search_by_key(&answered, |message| message.id)
                .ok()
                .map(|index| &self.messages[index]);
            Reply {
                author: original.map(|original| self.author(original.author)),
                snippet: original
                    .map(|original| match snippet(&original.content) {
                        text if text.is_empty() && original.is_voice() => {
                            t("Mensaje de voz").into()
                        }
                        // Only a card: its first words stand for it.
                        text if text.is_empty() => rune_model::card_words(original)
                            .map(snippet)
                            .find(|said| !said.is_empty())
                            .unwrap_or_default(),
                        text => text,
                    })
                    .unwrap_or_default(),
                emoji: original
                    .map(|original| self.text_pictures(&original.text_references))
                    .unwrap_or_default(),
            }
        });
        // An app's answer to a command says who ran which (P3).
        let reply = reply.or_else(|| {
            let invoked = message.components.invoked.as_ref()?;
            Some(Reply {
                author: Some(self.author(invoked.user)),
                snippet: tr!("usó /{command}", command = invoked.command),
                emoji: Vec::new(),
            })
        });
        let mine = Some(message.author) == self.me;
        let abilities = Abilities {
            react: self.can.react,
            vote: self.can.vote,
            reply: self.can.reply,
            edit: self.can.edit && mine && !message.is_voice(),
            delete: self.can.delete && (mine || self.manage),
            pin: self.can.pin,
            read_sync: self.can.read_sync,
            thread: self.can.thread,
            forward: self.can.forward,
            copy_link: self.can.copy_link,
            reaction_list: self.can.reaction_list,
            poll_voters: self.can.poll_voters,
            end_poll: self.can.end_poll
                && mine
                && message
                    .poll
                    .as_ref()
                    .is_some_and(|poll| !poll.ended(self.now)),
        };
        let upload_cancel = (self
            .pending_files
            .get(&message.id)
            .is_some_and(|files| !files.is_empty())
            && matches!(message.delivery, rune_model::Delivery::Sending))
        .then(|| format!("msg:{}:upload:cancel", message.id));
        let live = Live {
            pending_files: self
                .pending_files
                .get(&message.id)
                .map_or(&[], Vec::as_slice),
            upload_progress: self.upload_progress.get(&message.id).copied(),
            upload_cancel: upload_cancel.as_deref(),
            attachment_images: Some(&self.attachment_images),
            attachment_failures: Some(&self.attachment_failures),
            sticker_images: Some(&self.sticker_images),
            animated: Some(&self.animated),
            revealed_files: Some(&self.revealed_files),
            voice: self
                .voice
                .as_ref()
                .filter(|player| player.message == message.id)
                .map(crate::VoicePlayer::now),
            voice_speed: self.voice_speed,
            editing: self
                .editing
                .as_ref()
                .filter(|(editing, _)| *editing == message.id)
                .map(|(_, draft)| draft.as_str()),
            more: self.more == Some(message.id),
            recent: &self.recent,
            query: &self.query,
            names: Some(&self.names),
            revealed: self.revealed.get(&message.id).map_or(&[], Vec::as_slice),
            edit_channel: self.edit_channel.as_deref(),
            invitation_denied: self.invitation_denied.as_deref(),
            invitations: self
                .invitation_cards
                .get(&message.id)
                .map_or(&[], Vec::as_slice),
            vote_denied: self.vote_denied.as_deref(),
            poll_choice: self
                .poll_choice
                .as_ref()
                .filter(|(chosen, _)| *chosen == message.id)
                .map_or(&[], |(_, answers)| answers.as_slice()),
            voice_channels: Some(&self.voice_channels),
            app: self.app.as_ref().map(|app| app.parts(message.id)),
            context: self
                .context
                .filter(|(open, _)| *open == message.id)
                .map(|(_, at)| at),
            me: self.me,
            obscure_explicit: self.obscured.contains(&message.author),
            voice_left: &self.voice_left,
            shift: self.shift,
        };
        let row = live_row(
            MessageView {
                message,
                author: self.author(message.author),
                continued: continued && !first_new,
                day,
                first_new,
                start: (index == 0 && self.reached_start).then(|| self.start.clone()),
                reply,
                mentions_me: self.me.is_some_and(|me| message.mentions(me))
                    || self.to_my_roles.contains(&message.id)
                    || message.mention_everyone
                    || (self.names.everyone.contains(&message.author)
                        && text::mentions_everyone(&message.content)),
                abilities,
                reacting: self.reacting == Some(message.id),
                author_card: self
                    .author_card
                    .as_ref()
                    .filter(|(open, _, _)| *open == message.id)
                    .map(|(_, on_mark, card)| {
                        (*on_mark, rune_components::profile::card(card, &self.look))
                    }),
                width: self.width,
                now: self.now,
                style: self.style,
            },
            live,
            &self.look,
        );
        if self.highlighted == Some(message.id) {
            Element::stack(vec![
                Element::panel(Element::spacer()).background(Look::alpha(self.look.accent, 35)),
                row,
            ])
        } else {
            row
        }
    }
}

/// The first line of a message, short, for the line above a reply.
fn snippet(content: &str) -> String {
    let first = content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    let mut cut: String = first.chars().take(90).collect();
    if first.chars().count() > 90 {
        // A token cut in half (an emoji, a mention) is left out whole.
        if let Some(open) = cut.rfind('<').filter(|open| !cut[*open..].contains('>')) {
            cut.truncate(open);
        }
        cut.push('…');
    }
    cut
}

/// The people a forward's copy names in its text.
fn forward_mentions(message: &Message) -> impl Iterator<Item = UserId> + '_ {
    message
        .forwarded
        .iter()
        .flat_map(|copy| &copy.text_references)
        .filter_map(|reference| match reference.target {
            rune_model::TextTarget::User(user) => Some(user),
            _ => None,
        })
}

/// What the loaded messages of `channel` can mention, as [`names`] says.
pub(crate) fn names_for(store: &Store, channel: ChannelId, look: &Look) -> Names {
    let server = store.channel(channel).and_then(|found| found.server);
    let messages = store
        .timeline(channel)
        .map(|timeline| timeline.messages())
        .unwrap_or_default();
    names(store, server, channel, &messages, look)
}

/// What the messages of `server` can mention: the people they mention, the
/// roles, the conversations, and who may mention everyone in `channel`.
fn names(
    store: &Store,
    server: Option<ServerId>,
    channel: ChannelId,
    messages: &[Arc<Message>],
    look: &Look,
) -> Names {
    let mut people = HashMap::new();
    let mut display_names = HashMap::new();
    // The people apps' menus start with picked are named too (P3), and
    // those a forward's copy mentions (contenido-de-mensajes).
    for user in messages.iter().flat_map(|message| {
        message
            .mentions
            .iter()
            .copied()
            .chain(crate::interactions::menu_people(message))
            .chain(forward_mentions(message))
    }) {
        if let (std::collections::hash_map::Entry::Vacant(entry), Some(found)) =
            (people.entry(user), store.user(user))
        {
            entry.insert(found.name.clone());
            display_names.insert(user, store.display_name(user, server));
        }
    }
    let Some(server) = server else {
        return Names {
            people,
            display_names,
            ..Names::default()
        };
    };
    let roles = store
        .server(server)
        .map(|found| {
            let everyone = found.everyone_id();
            found
                .roles
                .iter()
                .filter(|role| role.id != everyone)
                .map(|role| {
                    (
                        role.name.clone(),
                        role.color.map_or(look.accent, |color| look.ink(color)),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let channels = store
        .channel_tree(server)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .map(|channel| (channel.name.clone(), channel.id.0))
        .collect();
    // One look per author, not per message.
    let authors: HashSet<UserId> = messages.iter().map(|message| message.author).collect();
    let everyone = match (store.server(server), store.channel(channel)) {
        (Some(found), Some(conversation)) => authors
            .into_iter()
            .filter(|user| {
                store.member_of(server, *user).is_some_and(|member| {
                    permissions::in_channel(found, member, conversation)
                        .contains(Permissions::MENTION_EVERYONE)
                })
            })
            .collect(),
        _ => HashSet::new(),
    };
    Names {
        people,
        display_names,
        role_ids: store
            .server(server)
            .map(|server| {
                server
                    .roles
                    .iter()
                    .map(|role| {
                        (
                            role.id,
                            (
                                role.name.clone(),
                                role.color.map_or(look.accent, |color| look.ink(color)),
                            ),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        roles,
        channels,
        everyone,
        ..Names::default()
    }
}

/// `names` with how each conversation a link of `messages` names reads on
/// its pill, and `now` for the moments relative to it (P4).
fn places(
    mut names: Names,
    store: &Store,
    here: Option<ServerId>,
    messages: &[Arc<Message>],
    now: Timestamp,
) -> Names {
    names.now = now;
    for reference in messages.iter().flat_map(|message| &message.text_references) {
        let rune_model::TextTarget::Link { channel, .. } = reference.target else {
            continue;
        };
        let Some(found) = store.channel(channel).filter(|found| Store::visible(found)) else {
            continue;
        };
        let name = match found.kind {
            rune_model::ChannelKind::Direct => format!("@{}", store.channel_title(channel)),
            rune_model::ChannelKind::Group => store.channel_title(channel),
            _ => format!("#{}", found.name),
        };
        let label = match found.server.filter(|server| Some(*server) != here) {
            Some(server) => match store.server(server) {
                Some(server) => format!("{} › {name}", server.shown_name()),
                None => name,
            },
            None => name,
        };
        names.places.insert(channel.0, label);
    }
    names
}

/// The list of the open conversation, `width` wide.
/// `height`: the window's, which the skeleton of a loading history fills.
pub fn timeline(
    ui: &Ui,
    store: &Store,
    channel: ChannelId,
    width: f32,
    height: f32,
    now: Timestamp,
) -> Element {
    // Own themes (temas-propios): see-through with the window.
    let look = ui.panes_look().0;
    let Some(conversation) = store.channel(channel) else {
        return Element::spacer();
    };
    if let Some(permit) = store
        .abilities(channel)
        .filter(|abilities| !abilities.read_history.allowed())
        .map(|abilities| abilities.read_history)
    {
        return limn::Empty::new(
            t("No puedes leer el historial de esa conversación."),
            permit.reason().unwrap_or_default(),
            look.palette(),
        )
        .render()
        .flex(1.0);
    }
    if store
        .timeline(channel)
        .is_some_and(|timeline| timeline.is_empty() && timeline.context_source().is_some())
    {
        return limn::Empty::new(
            t("Este contexto ya no tiene mensajes disponibles"),
            t("Puedes volver a los mensajes recientes de la conversación."),
            look.palette(),
        )
        .render()
        .flex(1.0);
    }
    let Some(timeline) = store
        .timeline(channel)
        .filter(|timeline| !timeline.is_empty())
    else {
        if let Some(reason) = store
            .timeline(channel)
            .and_then(|timeline| timeline.load_error())
        {
            return limn::Empty::new(t("No se pudo cargar el historial."), reason, look.palette())
                .render()
                .flex(1.0);
        }
        // Shapes of messages as many as fill the window, against the end.
        return rune_components::message::history_skeleton(
            format!("{}:loading", timeline_id(channel)),
            channel.0,
            width,
            height,
            &look,
        )
        .flex(1.0);
    };
    let server = conversation.server;
    let messages = timeline.messages();
    // Only the people who wrote what is loaded: a server of thousands costs
    // what its conversation's authors cost. One look per run of an author.
    let mut authors = HashMap::new();
    for user in messages
        .chunk_by(|a, b| a.author == b.author)
        .map(|run| run[0].author)
        // Who ran the commands apps answer (P3).
        .chain(
            messages
                .iter()
                .filter_map(|message| Some(message.components.invoked.as_ref()?.user)),
        )
    {
        if authors.contains_key(&user) || store.user(user).is_none() {
            continue;
        }
        authors.insert(user, author(ui, store, server, user, &look));
    }
    let Some(allowed) = store.abilities(channel) else {
        return Element::spacer();
    };
    let title = store.channel_title(channel);
    let start = match server {
        Some(_) => (
            tr!("¡Te damos la bienvenida a #{title}!", title),
            tr!("Este es el inicio del canal #{title}.", title),
        ),
        None => (
            title.clone(),
            tr!("Este es el inicio de tu conversación con {title}.", title),
        ),
    };
    let count = messages.len();
    let scrollbar = look.chat_scrollbar();
    // The author's card, once its place is known (see `Profiles::author_at`).
    let held = ui
        .profiles
        .author_at
        .filter(|(target, _)| ui.profiles.author == Some(*target));
    let held_author = held.and_then(|((message, _), _)| Some(timeline.get(message)?.author));
    let rows = Arc::new(Rows {
        pending_files: messages
            .iter()
            .filter_map(|message| {
                let files = store.pending_attachments(channel, message.id);
                (!files.is_empty()).then(|| (message.id, files.to_vec()))
            })
            .collect(),
        upload_progress: messages
            .iter()
            .filter_map(|message| {
                store
                    .attachment_progress(channel, message.id)
                    .map(|progress| (message.id, progress))
            })
            .collect(),
        // The host bounds this cache; avoid scanning the full loaded history
        // on every rebuild just to find the few decoded thumbnails.
        attachment_images: ui
            .images
            .iter()
            // Previews, their ThumbHash and their blurred form (contenido-de-mensajes).
            .filter(|(source, _)| {
                matches!(
                    source.kind,
                    rune_model::ImageKind::AttachmentPreview
                        | rune_model::ImageKind::ThumbHash
                        | rune_model::ImageKind::Obscured
                )
            })
            .map(|(source, image)| (source.clone(), image.clone()))
            .chain(
                messages
                    .iter()
                    // Also a line's pictures of a message of emoji alone:
                    // a reply quotes it at that size (2026-10-18).
                    .flat_map(|message| {
                        crate::images::emoji_pictures(message)
                            .into_iter()
                            .chain(crate::images::text_emoji(&message.text_references, false))
                    })
                    .filter_map(|source| Some((source.clone(), ui.images.get(source)?.clone()))),
            )
            .collect(),
        attachment_failures: ui.image_failures.clone(),
        sticker_images: messages
            .iter()
            .flat_map(|message| message.stickers.iter())
            .filter_map(|sticker| sticker.image.as_ref())
            .filter_map(|source| {
                ui.images
                    .get(source)
                    .map(|image| (source.clone(), image.clone()))
            })
            .collect(),
        messages: messages.clone(),
        authors,
        me: store.me(),
        new_marker: ui.new_marker.get(&channel).copied(),
        marked_by_hand: crate::messages::held_unread(store, channel),
        reacting: ui.reacting,
        highlighted: crate::context::highlighted(ui, channel),
        // Once its place is known, it floats over the conversation there.
        author_card: ui.profiles.author.filter(|_| held.is_none()).and_then(
            |(message, on_mark)| {
                let author = timeline.get(message)?.author;
                crate::profile::popout_card(ui, store, server, author, &look)
                    .map(|card| (message, on_mark, card))
            },
        ),
        reached_start: timeline.reached_start,
        start,
        edit_channel: allowed
            .manage
            .allowed()
            .then(|| format!("cs:open:{channel}")),
        invitation_denied: store.invitation_preview().reason().map(str::to_owned),
        invitation_cards: messages
            .iter()
            .map(|message| {
                (
                    message.id,
                    crate::invitation_cards::cards(ui, store, message),
                )
            })
            .filter(|(_, cards)| !cards.is_empty())
            .collect(),
        vote_denied: allowed.vote.reason().map(str::to_owned),
        poll_choice: ui.messages.poll_choice.clone(),
        voice_channels: messages
            .iter()
            .flat_map(|message| &message.embeds)
            .filter_map(|embed| embed.channel)
            .filter_map(|voice| {
                let permit = store.abilities(voice)?.join_voice;
                Some((voice, permit.reason().map(str::to_owned)))
            })
            .collect(),
        animated: ui.animated.clone(),
        revealed_files: ui.messages.revealed_files.clone(),
        voice: ui.voice_player.clone(),
        voice_speed: ui.voice_memory.speed,
        voice_left: ui.voice_memory.left().to_vec(),
        can: Abilities {
            react: allowed.react.allowed(),
            vote: allowed.vote.allowed(),
            reply: allowed.write.allowed(),
            edit: allowed.edit_own.allowed(),
            delete: allowed.delete_own.allowed(),
            pin: allowed.pin.allowed(),
            read_sync: store.remote_unread(),
            thread: store.create_thread(channel, false, true).allowed(),
            // Message actions (P4); ending a poll is decided per message.
            forward: allowed.forward.allowed(),
            copy_link: store
                .provider
                .as_ref()
                .is_some_and(|info| info.capabilities.channel_links.is_some()),
            reaction_list: allowed.reaction_list.allowed(),
            poll_voters: allowed.poll_voters.allowed(),
            end_poll: store
                .provider
                .as_ref()
                .is_some_and(|info| info.capabilities.create_polls),
        },
        manage: allowed.delete_others.allowed(),
        names: places(
            names(store, server, channel, &messages, &look),
            store,
            server,
            &messages,
            now,
        ),
        editing: ui
            .editing
            .map(|message| (message, ui.messages.edit_draft.clone())),
        more: ui.messages.more,
        recent: ui.messages.recent.clone(),
        query: ui.messages.query.clone(),
        revealed: ui.messages.revealed.clone(),
        // Keep message wrapping stable when the native rail appears. Rows
        // must stop before its controls instead of painting underneath them.
        width: (width - scrollbar.rail_width).max(0.0),
        now,
        look,
        style: ui.prefs.message_style(),
        blocked: messages
            .iter()
            .map(|message| message.author)
            .filter(|author| store.blocked(*author))
            .collect(),
        ignored: messages
            .iter()
            .map(|message| message.author)
            .filter(|author| store.is_ignored(*author))
            .collect(),
        blocked_shown: ui.conversations.blocked_shown.clone(),
        app: Some(crate::interactions::rows(ui, store, channel)),
        context: ui.messages.context,
        obscured: messages
            .iter()
            .map(|message| message.author)
            .filter(|author| store.obscures_explicit(channel, *author))
            .collect(),
        to_my_roles: messages
            .iter()
            .filter(|message| store.mentions_my_roles(message))
            .map(|message| message.id)
            .collect(),
        shift: ui.messages.shift_held,
    });
    let list = ScrollArea::measured_list(
        timeline_id(channel),
        count,
        ESTIMATE,
        move |index| messages[index].id.0,
        move |index| rows.row(index),
    )
    .follow_end(timeline.context_source().is_none())
    .appearance(scrollbar)
    // Shown while the pointer is over the chat, it scrolls or the bar is
    // used, then it fades.
    .visibility(ScrollbarVisibility::AutoHideUnlessHovered {
        hold_ms: 600,
        fade_ms: 200,
    })
    .motion(ScrollMotion::default())
    .render()
    .flex(1.0);
    // The card of an emoji clicked (2026-10-18).
    let list = crate::emoji_card::over(ui, store, list, &look);
    match held {
        Some((_, at)) => rune_components::profile::card_popout(
            list,
            held_author
                .and_then(|author| crate::profile::popout_card(ui, store, server, author, &look))
                .map(|card| rune_components::profile::card(&card, &look)),
            rune_components::profile::ids::AUTHOR_DISMISS,
            limn::PopoverPlacement::RightStart,
            &look,
        )
        .popover_at(at)
        .flex(1.0),
        None => list,
    }
}

/// A thin line at the top of the list while an older page is on its way.
pub fn loading_older(look: &Look) -> Element {
    Element::row(vec![
        Element::spacer().flex(1.0),
        Element::panel(rune_components::line(
            t("Cargando mensajes anteriores…"),
            12.0,
            look.muted,
        ))
        .padding(6.0)
        .radius(12.0)
        .background(look.raised)
        .border(look.border, 1.0),
        Element::spacer().flex(1.0),
    ])
    .padding(8.0)
}
