//! A message of the timeline, as one row of the measured list: the day it
//! starts, the line of what is new, the message it answers, its author
//! (unless it continues the author's group), its Markdown or the editor in
//! its place, its files and its reactions, with a bar of actions on hover
//! and a menu on a secondary click or on the bar's «Más».

use limn::{
    Button, Color, Component, ContextMenu, DropdownMenu, DropdownMenuEntry, DropdownMenuItem,
    Element, FontFamily, Icon, Link, Markdown, MarkdownColors, MessageAction, MessageActions,
    MessageActionsVisibility, TextSpan, Tooltip,
};
use rune_i18n::{es, t, tr};
use rune_model::{Delivery, Message, MessageId, MessageKind, Timestamp};

use crate::text::{self, Names, Spoilers};
use crate::{emoji, gap, icon, line, strong_line, Look};

mod attachments;
mod components;
mod embeds;
mod media;
mod voice;
// The history while it loads (pulido-ui-real).
mod skeleton;
pub use attachments::ids as attachment_ids;
pub use attachments::is_pdf as attachment_is_pdf;
pub use attachments::{animation_surface, copyable, picture, Copied};
pub use components::{
    app_ids, first_spoiler, menu_label, AppParts, OpenMenu, NO_INTERACTIONS, PARTS_SPOILER,
};
pub use embeds::{card_explicit, card_picture, card_spoiler};
pub use media::{controls as media_controls, media_surface, playable, Playable};
pub use skeleton::history_skeleton;
pub use voice::{VoiceNow, VOICE_SPEEDS};

/// The space left of a message's text: the column of avatars, with room on
/// both sides of the avatar.
pub const GUTTER: f32 = LEFT + AVATAR + 22.0;
const LEFT: f32 = 22.0;
const AVATAR: f32 = 42.0;
/// The space right of a message's text, past which only the bar floats.
const RIGHT: f32 = 48.0;
/// Messages of one author within this many minutes of the first of their
/// group read as one group. A design value; see docs/ARCHITECTURE.md.
pub const GROUP_MINUTES: i64 = 7;

/// How an author is shown.
#[derive(Clone, Debug, PartialEq)]
pub struct Author {
    pub name: String,
    pub image: Option<limn::RasterImage>,
    /// Their name's ink: their top role's color, or the text color.
    pub ink: Color,
    /// Behind their initial.
    pub accent: Color,
    pub bot: bool,
    /// Their decoration, role mark and server tag (P5).
    pub marks: crate::identity::Marks,
}

/// The message a reply answers.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    /// `None` when the message is not loaded or was deleted.
    pub author: Option<Author>,
    pub snippet: String,
    /// The server emoji `snippet` names, drawn in its place.
    pub emoji: Vec<crate::identity::TextPicture>,
}

/// What the account may do to a message.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Abilities {
    pub react: bool,
    /// Vote on its poll while it is open.
    pub vote: bool,
    pub reply: bool,
    pub edit: bool,
    pub delete: bool,
    /// Pin it to its channel, or take it out.
    pub pin: bool,
    /// A reminder to read it goes to the provider; otherwise the menu says
    /// that marking unread only changes this RUNE session.
    pub read_sync: bool,
    /// Start a thread from it.
    pub thread: bool,
    // Message actions (P4).
    /// Forward it to other conversations.
    pub forward: bool,
    /// Copy its link: the provider has links.
    pub copy_link: bool,
    /// See who reacted with each emoji.
    pub reaction_list: bool,
    /// See who voted for each answer of its poll.
    pub poll_voters: bool,
    /// End its poll now: the account started it and it is open.
    pub end_poll: bool,
}

/// How the account likes messages shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MessageStyle {
    /// One line a message: its time and author's name before what was
    /// written, and no avatars.
    pub compact: bool,
    /// The size of what is written; [`TEXT_SIZE`] is the usual one, and
    /// names and times follow it.
    pub text_size: f32,
    /// Room above the first message of a group.
    pub group_gap: f32,
    /// Show the reactions under a message.
    pub reactions: bool,
    /// Show a picture's frame; off, a picture is a file card like any other.
    pub image_previews: bool,
    /// Developer mode: the menu offers to copy the message's id.
    pub copy_id: bool,
    /// A sample, as the settings preview it: no menu and no actions.
    pub sample: bool,
    // Reading settings (imagenes-y-lectura).
    /// A text that is only links to pictures its cards show is hidden, as
    /// in Serein and Discord; off, the links stay over their pictures.
    pub hide_media_links: bool,
}

/// The usual size of what a message says.
pub const TEXT_SIZE: f32 = 16.0;
/// The usual room above a group.
pub const GROUP_GAP: f32 = 16.0;
/// The size limn's Markdown writes its body in; it is scaled from there.
const MARKDOWN_BODY: f32 = 14.0;

impl Default for MessageStyle {
    fn default() -> Self {
        Self {
            compact: false,
            text_size: TEXT_SIZE,
            group_gap: GROUP_GAP,
            reactions: true,
            image_previews: true,
            copy_id: false,
            sample: false,
            hide_media_links: true,
        }
    }
}

impl MessageStyle {
    /// How much bigger than usual the text is.
    fn scale(&self) -> f32 {
        self.text_size / TEXT_SIZE
    }
}

/// Everything a row shows.
pub struct MessageView<'a> {
    pub message: &'a Message,
    pub author: Author,
    /// It continues the group of the message above.
    pub continued: bool,
    /// The day it opens, when it is the first of its day.
    pub day: Option<String>,
    /// The first message not read yet.
    pub first_new: bool,
    /// The start of the conversation, above its first message.
    pub start: Option<(String, String)>,
    pub reply: Option<Reply>,
    pub mentions_me: bool,
    pub abilities: Abilities,
    /// The emoji picker is open on its actions, to react.
    pub reacting: bool,
    /// The author's card, open from their mark (`true`) or their name.
    pub author_card: Option<(bool, Element)>,
    pub width: f32,
    pub now: Timestamp,
    pub style: MessageStyle,
}

/// What the account is doing with a row right now, beyond [`MessageView`].
#[derive(Clone, Copy, Default)]
pub struct Live<'a> {
    /// Outgoing metadata has no remote IDs or URLs until confirmation.
    pub pending_files: &'a [rune_model::PendingAttachment],
    pub upload_progress: Option<(u64, u64)>,
    pub upload_cancel: Option<&'a str>,
    pub attachment_images:
        Option<&'a std::collections::HashMap<rune_model::RemoteImage, limn::RasterImage>>,
    pub attachment_failures: Option<&'a std::collections::HashSet<rune_model::RemoteImage>>,
    pub sticker_images:
        Option<&'a std::collections::HashMap<rune_model::RemoteImage, limn::RasterImage>>,
    /// First-frame digests the host is playing as surfaces named by
    /// [`animation_surface`]; the rest show their still first frame.
    pub animated: Option<&'a std::collections::HashSet<String>>,
    /// Spoiler attachments the account revealed.
    pub revealed_files: Option<&'a std::collections::HashSet<rune_model::AttachmentId>>,
    /// The voice message playing, when it is one of this row's.
    pub voice: Option<VoiceNow<'a>>,
    /// The voice messages' speed, as an index into [`VOICE_SPEEDS`].
    pub voice_speed: usize,
    /// What the editor in place of the text holds, while it is edited.
    pub editing: Option<&'a str>,
    /// Its «Más» menu is open.
    pub more: bool,
    /// The emoji used lately, most recent first: the quick reactions and the
    /// start of the picker.
    pub recent: &'a [String],
    /// What the picker of reactions searches, while open on the row.
    pub query: &'a str,
    /// What its text can mention.
    pub names: Option<&'a Names>,
    /// Its spoilers shown, by their order in the text.
    pub revealed: &'a [usize],
    /// The action of the start's «Editar canal», when the account may.
    pub edit_channel: Option<&'a str>,
    /// Why the current account cannot request invitation metadata.
    pub invitation_denied: Option<&'a str>,
    /// What each of its invitation links' cards knows of where it leads, in
    /// the order of [`invitation_links`]; a link without one only offers to
    /// review it.
    pub invitations: &'a [InvitationCard],
    /// Why the account cannot vote on polls here, when it cannot.
    pub vote_denied: Option<&'a str>,
    /// The answers of its multiselect poll picked before «Votar».
    pub poll_choice: &'a [u32],
    /// Why the account may not join each voice channel the cards of the
    /// timeline name (`None` when it may); one missing is not known.
    pub voice_channels:
        Option<&'a std::collections::HashMap<rune_model::ChannelId, Option<String>>>,
    /// What the account may do with its app's parts (P3); without it they
    /// show why they do nothing.
    pub app: Option<AppParts<'a>>,
    // limn adoption (2026-10-18).
    /// Its menu of a secondary click is open, its corner at this point of
    /// the window.
    pub context: Option<limn::Point>,
    // Message content (contenido-de-mensajes).
    /// The account, to tell a call it missed from one it joined.
    pub me: Option<rune_model::UserId>,
    /// The account's filter hides this row's explicit pictures until it
    /// reveals each, as the store's `obscures_explicit` decides.
    pub obscure_explicit: bool,
    // Message audio (audio-de-mensajes).
    /// Where the account left each voice message it played partway, in
    /// seconds: one not playing shows that place until it plays again.
    pub voice_left: &'a [(rune_model::AttachmentId, f32)],
    // limn-teclado-y-superficies.
    /// Shift is held: its bar shows the direct actions in place of the
    /// quick reactions, as Serein's and the official client's do.
    pub shift: bool,
}

/// What the card of an invitation link knows of where it leads.
#[derive(Clone)]
pub enum InvitationCard {
    /// It is being looked up.
    Loading,
    Server(InvitedServer),
    /// It leads nowhere the card can show, and why.
    Unavailable(String),
}

/// Where an invitation leads, as its public preview tells: a server, a
/// guest's way into one of its voice channels, a group or a friendship.
#[derive(Clone)]
pub struct InvitedServer {
    pub kind: InvitedKind,
    /// The server's or group's name, or who offers friendship; empty for a
    /// group without a name.
    pub name: String,
    pub image: Option<limn::RasterImage>,
    pub online: Option<u64>,
    pub members: Option<u64>,
    /// The account is in it already, or they are friends.
    pub member: bool,
    /// «Unirse» opens the invitation in RUNE to accept it; otherwise in the
    /// browser, saying so.
    pub joins_here: bool,
    // Scheduled events (eventos-programados).
    /// The server's event the invitation names: shown over the server, and
    /// «Ver evento» opens its detail once the account is in.
    pub event: Option<crate::events::InvitedEvent>,
}

/// What an invitation leads to, for the words of its card.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InvitedKind {
    #[default]
    Server,
    /// A guest's way into one voice channel of a server, not membership.
    Guest,
    Group,
    Friend,
}

/// Why «Unirse» opens the browser instead of joining from here.
pub const JOIN_HINT: &str =
    es("Se abre en el navegador: esta conexión no permite aceptarla desde RUNE");

/// The most invitation cards one message shows, as the reference client.
pub const INVITATION_CARDS: usize = 10;

/// The action of the `index`th invitation card of a message.
pub fn invitation_action(message: &Message, index: usize, what: &str) -> String {
    action(message, &format!("invitation:{index}:{what}"))
}

/// Action IDs of the emoji picker opened to react to a message.
pub mod react_ids {
    /// Every action of the picker starts with it.
    pub const BASE: &str = "react-pick";
    /// Prefix of the chosen emoji.
    pub const PICK: &str = "react-pick=";
    pub const DISMISS: &str = "react-pick:dismiss";
}

/// Whether `message` can continue the group of `previous` at all: same
/// author, same day, plain messages, and no reply. Groups of such a run are
/// split every [`GROUP_MINUTES`] from the first message of each; see
/// [`group_starts_at`].
pub fn same_run(previous: &Message, message: &Message) -> bool {
    previous.author == message.author
        && previous.kind == MessageKind::Default
        && message.kind == MessageKind::Default
        && message.reply_to.is_none()
        // An app's answer to a command opens its own group, as a reply.
        && message.components.invoked.is_none()
        && crate::time::day(previous.timestamp) == crate::time::day(message.timestamp)
}

/// Whether the message at `index` of `messages` starts a group. It walks
/// back to the start of its run and forward again, so it costs the length
/// of the run, at most `limit` messages back.
pub fn group_starts_at(
    messages: &[impl std::ops::Deref<Target = Message>],
    index: usize,
    limit: usize,
) -> bool {
    if index == 0 || !same_run(&messages[index - 1], &messages[index]) {
        return true;
    }
    let mut run = index;
    while run > 0 && index - run < limit && same_run(&messages[run - 1], &messages[run]) {
        run -= 1;
    }
    let mut group = run;
    for next in run + 1..=index {
        if messages[next]
            .timestamp
            .minutes_since(messages[group].timestamp)
            >= GROUP_MINUTES
        {
            group = next;
        }
    }
    group == index
}

/// The action IDs of a message's controls, derived from its id.
pub fn action(message: &Message, what: &str) -> String {
    format!("msg:{}:{}", message.id, what)
}

/// The row of a message, named for the window to tell which one the
/// pointer is over, also over its text: `msg:{id}:row`.
pub fn row_region(message: MessageId) -> String {
    format!("msg:{message}:row")
}

/// What follows a message's id in the action of an emoji of its text: its
/// number and itself, `msg:{id}:emoji:{n}:{emoji}` (2026-10-18).
pub const EMOJI: &str = "emoji:";

/// The invitation links of a message the adapter recognized, in its order,
/// each still written in the text, which keeps them as selectable text.
/// This does not resolve URLs, fetch metadata, or imply their type.
pub fn invitation_links(message: &Message) -> Vec<&str> {
    message
        .text_references
        .iter()
        .filter(|reference| {
            reference.target == rune_model::TextTarget::Invitation
                && !reference.source.is_empty()
                && reference.source.len() <= 2048
                && message.content.contains(reference.source.as_str())
        })
        .map(|reference| reference.source.as_str())
        .take(INVITATION_CARDS)
        .collect()
}

/// The ID of the editor that takes a message's place while it is edited.
pub fn editor_id(message: MessageId) -> String {
    format!("msg:{message}:editor")
}

/// One row of the timeline, as nothing is being done with it.
pub fn message_row(view: MessageView<'_>, look: &Look) -> Element {
    live_row(view, Live::default(), look)
}

/// One row of the timeline.
pub fn live_row(view: MessageView<'_>, live: Live<'_>, look: &Look) -> Element {
    let message = view.message;
    let mut parts = Vec::new();
    if let Some((title, subtitle)) = &view.start {
        parts.push(start(title, subtitle, live.edit_channel, look));
    }
    if let Some(day) = &view.day {
        parts.push(day_divider(day, look));
    }
    if view.first_new {
        parts.push(new_divider(look));
    }
    let content_width = (view.width - gutter(&view.style) - RIGHT).max(80.0);
    let body = if !message.kind.is_said() {
        system(&view, &live, look)
    } else {
        content(&view, &live, content_width, look)
    };
    // A mention of the account: a tint and a bar down its left edge.
    let body = if view.mentions_me {
        Element::stack(vec![
            Element::panel(Element::spacer()).background(look.mention),
            Element::row(vec![
                Element::panel(Element::spacer())
                    .size(Some(2.0), None)
                    .background(look.accent),
                Element::spacer().flex(1.0),
            ]),
            body,
        ])
    } else {
        body
    };
    if view.style.sample {
        parts.push(body);
        return Element::column(parts).size(Some(view.width), None);
    }
    let hover = Element::panel(Element::spacer())
        .background(look.hover)
        .reveal_on_hover(action(message, "hover"));
    // The bar floats over the row's top: it takes none of its height, which
    // the text alone sets.
    let bar = actions(&view, &live, look).size(None, Some(0.0));
    let row = Element::stack(vec![hover, body, bar]).hover_region_id(row_region(message.id));
    parts.push(row);
    let element = Element::column(parts)
        .size(Some(view.width), None)
        .anchor(format!("anchor:{}", message.id));
    // A secondary click asks for the whole menu (`msg:{id}:context`), quick
    // reactions first, opened where it was; a message with nothing to offer
    // (one still sending, without text) asks for none.
    let offers = !message.delivery.is_local()
        || !choices(message, view.abilities, view.style.copy_id).is_empty();
    let menu = ContextMenu::new(
        action(message, "menu"),
        element,
        Vec::new(),
        look.menu_palette(),
    );
    let element = if offers {
        menu.open_action(action(message, "context"))
    } else {
        menu
    }
    .render();
    let Some(at) = live.context.filter(|_| offers) else {
        return element;
    };
    DropdownMenu::new(
        element,
        menu_entries(
            message,
            view.abilities,
            view.style.copy_id,
            live.recent,
            look,
        ),
        true,
        action(message, "context:dismiss"),
        look.menu_palette(),
    )
    .context_menu_style()
    .context_metrics(crate::look::MENU_METRICS)
    .at(at)
    .render()
}

/// The space left of a message's text: the column of avatars, or the time
/// of a compact message.
pub fn gutter(style: &MessageStyle) -> f32 {
    if style.compact {
        COMPACT_GUTTER * style.scale().max(1.0)
    } else {
        GUTTER
    }
}

/// The time column of a compact message: room for "12:59 p. m.".
const COMPACT_GUTTER: f32 = 84.0;

/// Where the names go in a record's sentence before they become runs:
/// marks of Unicode's private use area, which no translation uses.
const WHO: &str = "\u{E000}";
const TARGET: &str = "\u{E001}";
const MARKS: [char; 2] = ['\u{E000}', '\u{E001}'];

/// What a record of the service says, with its mark and ink: its words,
/// and the names in it as links to their profiles (contenido-de-mensajes).
fn system_line(
    view: &MessageView<'_>,
    live: &Live<'_>,
    look: &Look,
) -> (Icon, Color, Vec<TextSpan>) {
    let message = view.message;
    let (who, target_name) = (WHO, TARGET);
    let target = message.mentions.first().copied();
    let target_shown = target
        .and_then(|user| {
            live.names.and_then(|names| {
                names
                    .display_names
                    .get(&user)
                    .or_else(|| names.people.get(&user))
            })
        })
        .map_or(t("alguien"), String::as_str);
    // What someone wrote cannot pass for a name.
    let content = message.content.trim().replace(MARKS, "");
    let (mark, ink, said) = match message.kind {
        MessageKind::Join => (
            Icon::ArrowRight,
            look.online,
            tr!("{who} se unió al servidor.", who),
        ),
        MessageKind::RecipientAdd => (
            Icon::ArrowRight,
            look.online,
            tr!("{who} añadió a {target_name} al grupo.", who, target_name),
        ),
        MessageKind::RecipientRemove if message.mentions.first() == Some(&message.author) => (
            Icon::ArrowLeft,
            look.danger,
            tr!("{who} abandonó el grupo.", who),
        ),
        MessageKind::RecipientRemove => (
            Icon::ArrowLeft,
            look.danger,
            tr!("{who} eliminó a {target_name} del grupo.", who, target_name),
        ),
        MessageKind::Call => call_line(message, live.me, look),
        MessageKind::ChannelRename => (
            Icon::Edit,
            look.muted,
            tr!("{who} cambió el nombre del canal: {content}", who, content),
        ),
        MessageKind::ChannelIcon => (
            Icon::Edit,
            look.muted,
            tr!("{who} cambió el icono del grupo.", who),
        ),
        MessageKind::Pinned => (
            Icon::Pin,
            look.muted,
            tr!("{who} fijó un mensaje en este canal.", who),
        ),
        MessageKind::Boost { tier } => {
            let times = content.parse::<u32>().ok().filter(|times| *times > 1);
            let mut said = match times {
                Some(times) => tr!("{who} mejoró el servidor {times} veces.", who, times),
                None => tr!("{who} mejoró el servidor.", who),
            };
            if tier > 0 {
                said.push_str(&tr!(" ¡El servidor alcanzó el nivel {tier}!", tier));
            }
            (Icon::Bolt, look.accent, said)
        }
        MessageKind::ThreadCreated => (
            Icon::Chat,
            look.muted,
            tr!("{who} inició un hilo: {content}", who, content),
        ),
        MessageKind::PollResult => {
            let question = poll_result(message)
                .and_then(|card| card.field("poll_question_text"))
                .unwrap_or(t("una encuesta"))
                .replace(MARKS, "");
            (
                Icon::ChartBar,
                look.muted,
                tr!("La encuesta «{question}» de {who} terminó.", question, who),
            )
        }
        MessageKind::Default | MessageKind::Other(_) => {
            (Icon::InfoCircle, look.muted, String::new())
        }
    };
    let spans = named(
        &said,
        [
            (&view.author.name, Some(message.author)),
            (target_shown, target),
        ],
        16.0 * view.style.scale(),
        !view.style.sample,
        look,
    );
    (mark, ink, spans)
}

/// A call's record: started or missed, and how long it lasted once over,
/// as the reference client words it (contenido-de-mensajes). A call that
/// goes on is never missed: the account may still join it.
fn call_line(
    message: &Message,
    me: Option<rune_model::UserId>,
    look: &Look,
) -> (Icon, Color, String) {
    let who = WHO;
    let call = message.call.as_ref();
    let missed = call.is_some_and(|call| {
        call.ended.is_some() && me.is_some_and(|me| !call.participants.contains(&me))
    });
    let lasted = call
        .and_then(|call| call.ended)
        .map(|ended| crate::time::lasted(ended.0.saturating_sub(message.timestamp.0)));
    match (missed, lasted) {
        (true, Some(duration)) => (
            Icon::HangUp,
            look.danger,
            tr!(
                "Te perdiste una llamada de {who} que duró {duration}.",
                who,
                duration
            ),
        ),
        (true, None) => (
            Icon::HangUp,
            look.danger,
            tr!("Te perdiste una llamada de {who}.", who),
        ),
        (false, Some(duration)) => (
            Icon::Phone,
            look.online,
            tr!(
                "{who} inició una llamada que duró {duration}.",
                who,
                duration
            ),
        ),
        (false, None) => (
            Icon::Phone,
            look.online,
            tr!("{who} inició una llamada.", who),
        ),
    }
}

/// `sentence` as runs of muted text with each of `names` in place of its
/// mark, [`WHO`] the first and [`TARGET`] the second: in the text's ink and
/// a heavier face, opening the person's profile when `linked`.
fn named(
    sentence: &str,
    names: [(&str, Option<rune_model::UserId>); 2],
    size: f32,
    linked: bool,
    look: &Look,
) -> Vec<TextSpan> {
    let mut spans = Vec::new();
    let mut rest = sentence;
    while let Some(at) = rest.find(MARKS) {
        if at > 0 {
            spans.push(TextSpan::new(
                &rest[..at],
                look.muted,
                size,
                FontFamily::Sans,
            ));
        }
        let (name, user) = names[usize::from(rest[at..].starts_with(TARGET))];
        let mut span = TextSpan::new(name, look.text, size, FontFamily::SansMedium);
        if let Some(user) = user.filter(|_| linked) {
            span = span.action(format!("member:{user}"));
        }
        spans.push(span);
        rest = &rest[at + WHO.len()..];
    }
    if !rest.is_empty() {
        spans.push(TextSpan::new(rest, look.muted, size, FontFamily::Sans));
    }
    spans
}

/// The card of a closed poll's result, which holds its data.
fn poll_result(message: &Message) -> Option<&rune_model::Embed> {
    message
        .embeds
        .iter()
        .find(|embed| embed.kind == rune_model::EmbedKind::PollResult)
}

/// The line under an app's reply only the account sees, with the way to
/// take it away.
fn ephemeral(message: &Message, scale: f32, look: &Look) -> Element {
    Element::row(vec![
        icon(Icon::Lock, look.muted).centered_y(),
        line(t("Sólo tú puedes ver esto ·"), 12.0 * scale, look.muted).centered_y(),
        Link::new(
            components::app_ids::dismiss(message.id),
            t("Descartar"),
            look.palette(),
        )
        .render()
        .centered_y(),
    ])
    .gap(6.0)
}

/// What every card, poll and part of a row is drawn with.
fn draw<'a>(view: &MessageView<'_>, live: &Live<'a>, look: &'a Look) -> embeds::Draw<'a> {
    embeds::Draw {
        scale: view.style.scale(),
        passive: view.style.sample,
        pictures: view.style.image_previews,
        now: view.now,
        images: live.attachment_images,
        failures: live.attachment_failures,
        names: live.names,
        look,
        base: format!("msg:{}", view.message.id),
        revealed: live.revealed,
        voice: live.voice_channels,
        animated: live.animated,
        app: None,
        obscure_explicit: live.obscure_explicit,
    }
}

/// A record of the service, such as someone joining: a line with its mark
/// in the gutter, and the result under a closed poll's line.
fn system(view: &MessageView<'_>, live: &Live<'_>, look: &Look) -> Element {
    let message = view.message;
    let (mark, ink, mut said_line) = system_line(view, live, look);
    let mut said = Vec::new();
    if let Some(card) = poll_result(message).filter(|_| message.kind == MessageKind::PollResult) {
        said.push(embeds::poll_result(
            card,
            (view.width - GUTTER - RIGHT).max(80.0),
            message.reply_to.map(|_| action(message, "poll-result")),
            &draw(view, live, look),
        ));
    }
    if view.style.reactions && !message.reactions.is_empty() {
        said.push(reactions(
            message,
            view.abilities.react,
            view.style.sample,
            look,
        ));
    }
    Element::column(vec![
        gap(
            0.0,
            if view.continued {
                2.0
            } else {
                view.style.group_gap
            },
        ),
        Element::row(vec![
            gap(LEFT, 0.0),
            Element::row(vec![
                Element::spacer().flex(1.0),
                icon(mark, ink),
                Element::spacer().flex(1.0),
            ])
            .size(Some(AVATAR), Some(22.0)),
            gap(GUTTER - LEFT - AVATAR, 0.0),
            Element::column(
                // One paragraph with its time at the end, so a long one wraps.
                std::iter::once({
                    said_line.extend([
                        TextSpan::new(
                            "  ",
                            look.muted,
                            16.0 * view.style.scale(),
                            FontFamily::Sans,
                        ),
                        TextSpan::new(
                            crate::time::stamp(message.timestamp, view.now),
                            look.faint,
                            12.0 * view.style.scale(),
                            FontFamily::Sans,
                        ),
                    ]);
                    Element::formatted_text(said_line)
                        .size(Some((view.width - GUTTER - RIGHT).max(80.0)), None)
                })
                .chain(said)
                .collect(),
            )
            .gap(4.0)
            .flex(1.0),
            gap(RIGHT, 0.0),
        ]),
        gap(0.0, 2.0),
    ])
}

/// The author, the time and what was written: a group's first message has
/// its author's mark and name; one that continues has its time in the gutter.
fn content(view: &MessageView<'_>, live: &Live<'_>, width: f32, look: &Look) -> Element {
    let message = view.message;
    let style = view.style;
    let scale = style.scale();
    let mut column = Vec::new();
    // The author's name and mark open their card.
    let (on_mark, card) = match view.author_card.clone() {
        Some((on_mark, card)) => (on_mark, Some(card)),
        None => (false, None),
    };
    let (mark_card, name_card) = if on_mark { (card, None) } else { (None, card) };
    if !view.continued && !style.compact {
        let label = crate::identity::inked(
            strong_line(&view.author.name, 16.0 * scale, view.author.ink),
            view.author.ink,
            &view.author.marks.gradient,
        );
        // A sample message, as in the settings' preview, opens nothing.
        let name = if style.sample {
            label
        } else {
            Element::clickable(crate::profile::ids::author_name(message.id.0), label)
                .underline_on_hover()
                .accessible(limn::Access::new(limn::Role::Button).name(view.author.name.clone()))
        };
        let name = crate::profile::card_popout(
            name,
            name_card,
            crate::profile::ids::AUTHOR_DISMISS,
            limn::PopoverPlacement::RightStart,
            look,
        );
        let mut head = vec![name.shrink_to_fit()];
        head.extend(crate::identity::after_name(
            &view.author.marks,
            16.0 * scale,
            look,
        ));
        if view.author.bot {
            head.push(tag("BOT", look).centered_y());
        }
        head.push(when(message, view.now, 12.0 * scale, look).centered_y());
        column.push(
            Element::row(head)
                .gap(8.0)
                .size(None, Some(22.0 * scale.max(1.0))),
        );
    }
    let body_scale = style.text_size / MARKDOWN_BODY;
    let text_width = if style.compact {
        (width - compact_name_width(view, scale)).max(80.0)
    } else {
        width
    };
    // Pictures shown in place of their links stand in for the text, unless
    // the person keeps the links (imagenes-y-lectura).
    let as_text = !style.image_previews || !style.hide_media_links || !only_media_links(message);
    let written = match live.editing {
        Some(draft) => Some(editor(message, draft, text_width, look)),
        None => (!message.content.is_empty() && as_text).then(|| {
            written_as(
                said(view, live, text_width, style.text_size, look),
                style.sample,
            )
        }),
    };
    if style.compact {
        // The author's name leads the line, then what was written.
        let mut head = vec![crate::identity::inked(
            strong_line(&view.author.name, 16.0 * scale, view.author.ink),
            view.author.ink,
            &view.author.marks.gradient,
        )
        .shrink_to_fit()];
        head.extend(crate::identity::after_name(
            &view.author.marks,
            14.0 * scale,
            look,
        ));
        if view.author.bot {
            head.push(tag("BOT", look));
        }
        let mut line_parts = vec![Element::row(head)
            .gap(6.0)
            .size(None, Some(BODY_LINE * body_scale))];
        line_parts.push(written.unwrap_or_else(Element::spacer).flex(1.0));
        column.push(Element::row(line_parts).gap(6.0));
    } else if let Some(written) = written {
        column.push(written);
    }
    if live.editing.is_none() && !style.sample {
        for index in 0..invitation_links(message).len() {
            column.push(match live.invitations.get(index) {
                Some(card) => invited_card(message, index, card, width, scale, look),
                None => invitation_card(message, index, width, live.invitation_denied, look),
            });
        }
    }
    let upload_progress = matches!(message.delivery, Delivery::Sending)
        .then_some(live.upload_progress)
        .flatten();
    if !live.pending_files.is_empty() {
        column.push(crate::uploads::pending_uploads(
            live.pending_files,
            upload_progress,
            width,
            look,
        ));
    }
    if let Some(state) = delivery(message, width, upload_progress, look) {
        column.push(state);
    }
    if let Some(cancel) = live
        .upload_cancel
        .filter(|_| matches!(message.delivery, Delivery::Sending))
    {
        column.push(Link::new(cancel, t("Cancelar envío"), look.palette()).render());
    }
    for attachment in message
        .attachments
        .iter()
        .filter(|_| !message.components.whole)
    {
        column.push(attachments::file(
            attachment, message.id, width, style, *live, look,
        ));
    }
    for sticker in &message.stickers {
        let image = sticker
            .image
            .as_ref()
            .and_then(|source| live.sticker_images?.get(source));
        let preview = image
            .map_or_else(
                || {
                    Element::panel(
                        Element::icon(Icon::Image)
                            .icon_size(32.0)
                            .foreground(look.muted),
                    )
                    .background(look.card)
                    .radius(8.0)
                },
                |image| picture(image, live.animated),
            )
            .size(Some(128.0), Some(128.0))
            .accessible(
                limn::Access::new(limn::Role::Image)
                    .name(tr!("Sticker: {name}", name = sticker.name)),
            );
        column.push(Element::column(vec![preview, line(&sticker.name, 12.0, look.muted)]).gap(4.0));
    }
    let draw = draw(view, live, look);
    if let Some(copy) = &message.forwarded {
        // Its files are drawn and opened as the message's own.
        let file = |attachment: &rune_model::Attachment, width: f32| {
            attachments::file(attachment, message.id, width, style, *live, look)
        };
        column.push(embeds::forwarded(copy, width, &draw, &file));
    }
    if let Some(poll) = &message.poll {
        let voting = embeds::Voting {
            base: action(message, "poll"),
            allowed: view.abilities.vote && !message.delivery.is_local(),
            denied: live.vote_denied,
            chosen: live.poll_choice,
        };
        column.push(embeds::poll(poll, width, &draw, &voting));
    }
    column.extend(embeds::cards(
        &message.embeds,
        false,
        &message.content,
        width,
        &draw,
    ));
    if !message.components.is_empty() {
        // A message made of parts shows its files where they place them.
        let file = |name: &str, width: f32| {
            let found = message
                .attachments
                .iter()
                .find(|attachment| attachment.filename == name)?;
            Some(attachments::file(
                found, message.id, width, style, *live, look,
            ))
        };
        // Only the message's own parts tell its app (P3).
        let own = embeds::Draw {
            app: live.app.filter(|_| message.components.app.is_some()),
            ..draw
        };
        column.push(components::parts(
            &message.components.parts,
            &message.components.text_references,
            first_spoiler(false, None),
            width,
            &own,
            &file,
        ));
    }
    if message.components.ephemeral && !style.sample {
        column.push(ephemeral(message, scale, look));
    }
    if let MessageKind::Other(_) = message.kind {
        if message.content.is_empty()
            && message.embeds.is_empty()
            && message.components.is_empty()
            && message.attachments.is_empty()
            && message.stickers.is_empty()
        {
            column.push(line(
                t("Este tipo de mensaje todavía no se puede mostrar en RUNE."),
                14.0 * scale,
                look.faint,
            ));
        }
    }
    if !message.reactions.is_empty() && style.reactions {
        column.push(reactions(message, view.abilities.react, style.sample, look));
    }
    let gutter = if style.compact {
        Element::column(vec![line(
            crate::time::clock(message.timestamp),
            11.0 * scale,
            look.faint,
        )])
        .size(Some(gutter(&style) - 16.0), Some(BODY_LINE * body_scale))
        .padding(2.0)
    } else if view.continued {
        // The time of a continued message shows while the pointer is over it.
        Element::row(vec![
            Element::spacer().flex(1.0),
            line(
                crate::time::clock(message.timestamp),
                11.0 * scale,
                look.faint,
            )
            .reveal_on_hover(action(message, "time")),
            Element::spacer().flex(1.0),
        ])
        .size(Some(AVATAR + 8.0), Some(BODY_LINE * body_scale))
    } else {
        let face = crate::identity::decorated(
            |side| {
                crate::avatar_with_image(
                    &view.author.name,
                    view.author.accent,
                    side,
                    view.author.image.clone(),
                )
            },
            AVATAR,
            view.author.marks.decoration.clone(),
        );
        let mark = if style.sample {
            face
        } else {
            Element::clickable(crate::profile::ids::author_mark(message.id.0), face)
                .size(Some(AVATAR), Some(AVATAR))
                .radius(AVATAR / 2.0)
                .accessible(
                    limn::Access::new(limn::Role::Button)
                        .name(tr!("Perfil de {name}", name = view.author.name)),
                )
        };
        Element::column(vec![crate::profile::card_popout(
            mark,
            mark_card,
            crate::profile::ids::AUTHOR_DISMISS,
            limn::PopoverPlacement::RightStart,
            look,
        )])
        .size(Some(AVATAR), None)
    };
    let (lead, after) = if style.compact {
        (16.0, 0.0)
    } else if view.continued {
        (LEFT - 4.0, GUTTER - LEFT - AVATAR - 4.0)
    } else {
        (LEFT, GUTTER - LEFT - AVATAR)
    };
    let row = Element::row(vec![
        gap(lead, 0.0),
        gutter,
        gap(after, 0.0),
        Element::column(column).gap(4.0).flex(1.0),
        gap(RIGHT, 0.0),
    ]);
    let mut parts = vec![gap(0.0, if view.continued { 2.0 } else { style.group_gap })];
    if let Some(reply) = &view.reply {
        parts.push(reply_line(reply, &style, look));
    }
    parts.push(row);
    parts.push(gap(0.0, 2.0));
    // A group opens with room above it; its continuations sit close.
    Element::column(parts)
}

/// Whether the text is nothing but links to pictures its cards already show,
/// which then stand in for it.
fn only_media_links(message: &Message) -> bool {
    let media: Vec<_> = message
        .embeds
        .iter()
        .filter(|embed| embed.is_media())
        .filter_map(|embed| embed.url.as_deref())
        .collect();
    !media.is_empty()
        && !message.content.contains("||")
        && message
            .content
            .split_whitespace()
            .all(|word| media.contains(&word))
}

/// The line height of limn's Markdown body at its own size.
const BODY_LINE: f32 = 22.0;

fn invitation_card(
    message: &Message,
    index: usize,
    width: f32,
    denied: Option<&str>,
    look: &Look,
) -> Element {
    let mut parts = vec![
        strong_line(t("Enlace de invitación"), 15.0, look.text),
        line(t("Revisa el destino."), 13.0, look.muted),
    ];
    if let Some(reason) = denied {
        parts.push(line(reason, 12.0, look.muted));
    }
    parts.push(
        Button::new(
            invitation_action(message, index, "review"),
            t("Revisar invitación"),
            look.accent_palette(),
        )
        .enabled(denied.is_none())
        .render(),
    );
    Element::panel(Element::column(parts).gap(8.0))
        .padding(14.0)
        .background(look.card)
        .border(look.border, 1.0)
        .radius(8.0)
        .size(Some(width.min(400.0)), None)
        .accessible(limn::Access::new(limn::Role::Group).name(t("Enlace de invitación")))
}

/// Where an invitation leads, as the reference client draws it: what it
/// invites to, the server's or group's mark (or the person's picture), name
/// and counts, and the way in.
fn invited_card(
    message: &Message,
    index: usize,
    card: &InvitationCard,
    width: f32,
    scale: f32,
    look: &Look,
) -> Element {
    const MARK: f32 = 48.0;
    let outer = width.min(432.0);
    let inner = outer - 32.0;
    // A narrow card stacks its parts; a wide one puts mark, server and the
    // way in side by side, keeping about 120 for the button.
    let narrow = outer < 340.0;
    let text_width = if narrow {
        inner
    } else {
        inner - MARK - 16.0 - 136.0
    };
    // A person's picture is round, as everywhere.
    let mark = |face: Element, round: bool| {
        Element::panel(crate::centered(face))
            .size(Some(MARK), Some(MARK))
            .radius(if round { MARK / 2.0 } else { 16.0 })
            .background(look.canvas)
            .clip_children()
    };
    let (eyebrow, mark, name, details, button) = match card {
        InvitationCard::Loading => (
            t("Enlace de invitación"),
            mark(Element::spacer(), false),
            line(t("Consultando invitación…"), 15.0 * scale, look.muted),
            None,
            None,
        ),
        InvitationCard::Unavailable(reason) => (
            t("Invitación no disponible"),
            mark(icon(Icon::WarningTriangle, look.muted), false),
            text_block(reason, 14.0 * scale, look.muted, text_width),
            None,
            None,
        ),
        InvitationCard::Server(server) => {
            let shown = if server.name.is_empty() {
                t("Grupo sin nombre")
            } else {
                server.name.as_str()
            };
            let initials: String = shown
                .split_whitespace()
                .filter_map(|word| word.chars().next())
                .take(3)
                .collect();
            let face = Element::text(initials)
                .font_family(FontFamily::SansMedium)
                .font_size(16.0)
                .foreground(look.text);
            let face = match &server.image {
                Some(image) => Element::stack(vec![
                    crate::centered(face),
                    Element::raster_image(image.clone().cover()).size(Some(MARK), Some(MARK)),
                ])
                .size(Some(MARK), Some(MARK)),
                None => face,
            };
            let count = |ink: Color, label: String| {
                Element::row(vec![
                    Element::panel(Element::spacer())
                        .size(Some(8.0), Some(8.0))
                        .radius(4.0)
                        .background(ink)
                        .centered_y(),
                    line(label, 13.0 * scale, look.muted).centered_y(),
                ])
                .gap(4.0)
            };
            let mut counts = Vec::new();
            if let Some(online) = server.online {
                counts.push(count(look.online, tr!("{online} en línea", online)));
            }
            if let Some(members) = server.members {
                counts.push(count(look.faint, tr!("{members} miembros", members)));
            }
            let (eyebrow, open, join) = match (server.kind, server.member) {
                (InvitedKind::Server | InvitedKind::Guest, true) => {
                    (t("Ya eres miembro de"), t("Ir al servidor"), t("Unirse"))
                }
                (InvitedKind::Server, false) => (
                    t("Te invitaron a unirte a un servidor"),
                    t("Ir al servidor"),
                    t("Unirse"),
                ),
                (InvitedKind::Guest, false) => (
                    t("Te invitaron a entrar como invitado"),
                    t("Ir al servidor"),
                    t("Unirse"),
                ),
                (InvitedKind::Group, true) => {
                    (t("Ya estás en el grupo"), t("Ir al grupo"), t("Unirse"))
                }
                (InvitedKind::Group, false) => (
                    t("Te invitaron a unirte a un grupo"),
                    t("Ir al grupo"),
                    t("Unirse"),
                ),
                (InvitedKind::Friend, true) => (
                    t("Ya son amigos"),
                    t("Enviar mensaje"),
                    t("Aceptar amistad"),
                ),
                (InvitedKind::Friend, false) => (
                    t("Te invitaron a ser amigos"),
                    t("Enviar mensaje"),
                    t("Aceptar amistad"),
                ),
            };
            let button = if server.member {
                Button::new(
                    invitation_action(message, index, "open"),
                    open,
                    look.accent_palette(),
                )
                .render()
            } else if server.joins_here {
                Button::new(
                    invitation_action(message, index, "join"),
                    join,
                    look.accent_palette(),
                )
                .render()
            } else {
                Tooltip::new(
                    Button::new(
                        invitation_action(message, index, "join"),
                        join,
                        look.accent_palette(),
                    )
                    .render()
                    .accessible(
                        limn::Access::new(limn::Role::Button)
                            .name(join)
                            .description(t(JOIN_HINT)),
                    ),
                    invitation_action(message, index, "join:hint"),
                    t(JOIN_HINT),
                    look.menu_palette(),
                )
                .render()
            };
            (
                eyebrow,
                mark(face, server.kind == InvitedKind::Friend),
                Element::text(shown)
                    .font_family(FontFamily::SansMedium)
                    .font_size(16.0 * scale)
                    .foreground(look.text)
                    .size(Some(text_width.max(40.0)), None),
                {
                    let mut lines = Vec::new();
                    if !counts.is_empty() {
                        lines.push(if narrow {
                            Element::column(counts).gap(4.0)
                        } else {
                            Element::row(counts).gap(12.0)
                        });
                    }
                    // On a line of its own: it does not fit beside the counts.
                    if server.kind == InvitedKind::Guest {
                        lines.push(text_block(
                            t("Como invitado, un canal de voz"),
                            13.0 * scale,
                            look.muted,
                            text_width,
                        ));
                    }
                    (!lines.is_empty()).then(|| Element::column(lines).gap(4.0))
                },
                Some(button),
            )
        }
    };
    let mut about = vec![name];
    about.extend(details);
    // Scheduled events (eventos-programados): the event over its server.
    let event = match card {
        InvitationCard::Server(server) => server.event.as_ref().map(|event| {
            let detail = server
                .member
                .then(|| invitation_action(message, index, "event"));
            crate::events::invited_event(event, detail, inner - 24.0, scale, look)
        }),
        _ => None,
    };
    let eyebrow = match card {
        InvitationCard::Server(server) if server.event.is_some() && !server.member => {
            t("Te invitaron a un evento")
        }
        _ => eyebrow,
    };
    let mut parts = vec![Element::text(eyebrow)
        .font_family(FontFamily::SansMedium)
        .font_size(12.0 * scale)
        .foreground(look.muted)
        .size(Some(inner.max(40.0)), None)];
    parts.extend(event);
    if narrow {
        parts.push(mark);
        parts.extend(about);
        parts.extend(button);
    } else {
        let mut row = vec![mark, Element::column(about).gap(4.0).flex(1.0).centered_y()];
        row.extend(button.map(Element::centered_y));
        parts.push(Element::row(row).gap(16.0));
    }
    Element::panel(Element::column(parts).gap(12.0))
        .padding(16.0)
        .background(look.card)
        .border(look.border, 1.0)
        .radius(8.0)
        .size(Some(outer), None)
        .accessible(limn::Access::new(limn::Role::Group).name(eyebrow))
}

fn text_block(value: &str, size: f32, ink: Color, width: f32) -> Element {
    Element::text(value)
        .font_family(FontFamily::Sans)
        .font_size(size)
        .foreground(ink)
        .size(Some(width.max(40.0)), None)
}

/// About how wide a compact message's name is, to give the rest to the
/// text: on the wide side, since the text keeps the width it is given.
fn compact_name_width(view: &MessageView<'_>, scale: f32) -> f32 {
    let name = view.author.name.chars().count() as f32 * 10.0 * scale;
    name.min(220.0) + if view.author.bot { 40.0 } else { 0.0 } + 12.0
}

/// What the message says, as Markdown builds it with its body `size` px,
/// with its mentions and spoilers drawn and "(editado)" at its end.
fn said(view: &MessageView<'_>, live: &Live<'_>, width: f32, size: f32, look: &Look) -> Element {
    let message = view.message;
    // Its code blocks and quotes are cards: apart from a row under the pointer.
    let palette = limn::ComponentPalette {
        raised: look.card,
        ..look.palette()
    };
    let colors = MarkdownColors {
        text: if message.delivery.is_local() {
            look.muted
        } else {
            look.text
        },
        ..MarkdownColors::for_palette(palette)
    };
    let mut element = Markdown::new(&message.content, palette)
        .width(width)
        .font_size(size)
        .colors(colors)
        .render()
        .element;
    let none = Names::default();
    let names = live.names.unwrap_or(&none);
    let jumbo = text::only_emoji(&message.content, &message.text_references);
    // A click on an emoji opens its card (2026-10-18), but not in a sample.
    let emoji_action = action(message, EMOJI);
    text::decorate(
        &mut element,
        names,
        &message.mentions,
        &message.text_references,
        names.everyone.contains(&message.author),
        &Spoilers {
            revealed: live.revealed,
            action: action(message, "spoiler:"),
            first: 0,
        },
        text::Pictures {
            loaded: live.attachment_images,
            playing: live.animated,
            // Their larger pictures, which the host loads for such a
            // message whatever its style (`images::emoji_pictures`).
            jumbo,
            emoji_action: (!view.style.sample).then_some(emoji_action.as_str()),
        },
        look,
    );
    // Emoji alone are larger, but not in a compact line (as the official
    // client draws them).
    if !view.style.compact && jumbo {
        text::enlarge(&mut element, text::JUMBO);
    }
    if let Some(at) = message.edited {
        let edited = 11.0 * size / MARKDOWN_BODY;
        if !text::append_edited(&mut element, at, edited, look) {
            let mark = TextSpan {
                text: t("(editado)").into(),
                ..text::edited_mark(at, edited, look)
            };
            element = Element::column(vec![element, Element::formatted_text(vec![mark])]);
        }
    }
    element
}

/// What a message says, as Markdown built it; in a sample, its links do
/// nothing.
fn written_as(mut element: Element, passive: bool) -> Element {
    if passive {
        text::walk(&mut element, &mut |spans| {
            for span in spans {
                span.action = None;
            }
        });
    }
    element
}

/// The editor in place of the text, and how to leave it.
fn editor(message: &Message, draft: &str, width: f32, look: &Look) -> Element {
    let field = Element::text_area(editor_id(message.id), draft, "")
        .text_area_enter_submits(true)
        .text_area_auto_grow(44.0, 300.0)
        .foreground(look.text)
        .font_family(FontFamily::Sans)
        .font_size(16.0)
        .padding(12.0);
    let field = Element::panel(field)
        .background(look.raised)
        .border(look.border, 1.0)
        .radius(8.0)
        .size(Some(width), None);
    let link = look.palette().link();
    let span = |said: &str, ink| TextSpan::new(said, ink, 12.0, FontFamily::Sans);
    // One paragraph, so it wraps where the editor is narrow.
    let help = Element::formatted_text(vec![
        span(t("Presiona Escape para "), look.text),
        span(t("cancelar"), link).action(action(message, "edit-cancel")),
        span(t(" • Presiona Enter para "), look.text),
        span(t("guardar"), link).action(action(message, "edit-save")),
    ])
    .size(Some(width), None);
    Element::column(vec![field, help]).gap(6.0)
}

/// When it was written, beside the name, with the whole date on hover.
fn when(message: &Message, now: Timestamp, size: f32, look: &Look) -> Element {
    let at = message.timestamp;
    let said = if crate::time::day(at) == crate::time::day(now) {
        crate::time::clock(at)
    } else {
        crate::time::stamp(at, now)
    };
    let id = action(message, "when");
    Tooltip::new(
        line(said, size, look.faint).shrink_to_fit(),
        id,
        full_date(at),
        look.menu_palette(),
    )
    .render()
}

/// "miércoles, 14 de octubre de 2026, 08:20".
pub fn full_date(at: Timestamp) -> String {
    crate::time::full_date(at)
}

/// Where an unconfirmed message of the account is: sending, refused, or
/// maybe sent, with what can be done about it.
fn delivery(
    message: &Message,
    width: f32,
    progress: Option<(u64, u64)>,
    look: &Look,
) -> Option<Element> {
    if matches!(message.delivery, Delivery::Sending) && progress.is_some() {
        return None;
    }
    let palette = look.palette();
    let (glyph, ink, sentence, retry) = match &message.delivery {
        Delivery::Sent => return None,
        Delivery::Sending => (
            Icon::Clock,
            look.faint,
            if progress.is_some_and(|(sent, total)| sent == total) {
                t("Esperando confirmación…")
            } else {
                t("Enviando…")
            }
            .to_owned(),
            false,
        ),
        Delivery::Failed { reason } => (
            Icon::ErrorCircle,
            look.danger,
            tr!("No se envió: {reason}.", reason),
            true,
        ),
        Delivery::Unknown { reason } => (
            Icon::WarningTriangle,
            look.idle,
            tr!("Puede que no haya llegado: {reason}.", reason),
            true,
        ),
    };
    // The column a message sits in does not bound its rows: each gets the
    // message's width, and what can be done goes under the reason so a long
    // reason never pushes it out of sight. The reason wraps: a server's
    // AutoMod may say a whole sentence of why (automod).
    let status = Element::row(vec![
        icon(glyph, ink),
        crate::text(sentence, 12.0, ink).flex(1.0),
    ])
    .gap(6.0)
    .size(Some(width), None);
    if !retry {
        return Some(status);
    }
    let actions = Element::row(vec![
        Link::new(action(message, "retry"), t("Reintentar"), palette).render(),
        Link::new(action(message, "discard"), t("Descartar"), palette)
            .ink(look.muted)
            .render(),
        Element::spacer().flex(1.0),
    ])
    .gap(12.0)
    .size(Some(width), Some(20.0));
    Some(Element::column(vec![status, actions]).gap(2.0))
}

/// A small mark beside a name, such as BOT.
fn tag(label: &str, look: &Look) -> Element {
    Element::panel(
        Element::text(label)
            .font_family(FontFamily::SansBold)
            .font_size(9.0)
            .foreground(look.on_accent),
    )
    .padding(3.0)
    .radius(3.0)
    .background(look.accent)
}

/// The line above a reply, as the official client draws it (its
/// `messageSpine`): a rule bent with a 6 px radius from the middle of the
/// avatar column, at the height of the line's middle, to 4 px before what
/// it answers, which starts where the author's name does below: their small
/// mark, name and the start of what they said.
fn reply_line(reply: &Reply, style: &MessageStyle, look: &Look) -> Element {
    const HEIGHT: f32 = 22.0;
    const SPINE: f32 = 2.0;
    let text = gutter(style);
    let center = if style.compact {
        // The middle of the time column, which starts 16 in.
        16.0 + (text - 16.0) / 2.0
    } else {
        LEFT + AVATAR / 2.0
    };
    let from = center - SPINE / 2.0;
    let bend = text - 4.0 - from;
    // A rounded rectangle's top-left corner, its other sides clipped away:
    // limn has no per-side border.
    let spine = Element::panel(Element::spacer())
        .size(Some(bend + 12.0), Some(HEIGHT))
        .border(look.border, SPINE)
        .radius(6.0)
        .translate(limn::Point::new(0.0, HEIGHT / 2.0 - SPINE / 2.0))
        .clip_to(bend, HEIGHT);
    let mut parts = vec![gap(from, 0.0), spine, gap(4.0, 0.0)];
    match &reply.author {
        Some(author) => {
            parts.push(
                crate::avatar_with_image(&author.name, author.accent, 16.0, author.image.clone())
                    .centered_y(),
            );
            parts.push(gap(4.0, 0.0));
            parts.push(
                strong_line(&author.name, 14.0, author.ink)
                    .shrink_to_fit()
                    .centered_y(),
            );
            parts.push(gap(4.0, 0.0));
            parts.push(reply_snippet(reply, look).flex(1.0).centered_y());
        }
        None => parts.push(
            line(
                t("No se pudo cargar el mensaje original."),
                14.0,
                look.faint,
            )
            .flex(1.0)
            .centered_y(),
        ),
    }
    parts.push(gap(RIGHT, 0.0));
    Element::row(parts).size(None, Some(HEIGHT))
}

/// What a reply answers, in its one line: its emoji 1.25 times its
/// letters, as the official client draws them there (2026-10-17: their
/// tokens showed as written).
fn reply_snippet(reply: &Reply, look: &Look) -> Element {
    text::line_with_pictures(&reply.snippet, &reply.emoji, 14.0, look.muted, 1.25)
        .unwrap_or_else(|| line(&reply.snippet, 14.0, look.muted))
}

/// Who reacted with an emoji, as far as the account knows: only its count
/// and whether the account is among them.
pub fn reacted_with(reaction: &rune_model::Reaction) -> String {
    let code = emoji::code(&reaction.emoji);
    match (reaction.me, reaction.count) {
        (true, 1) => tr!("Reaccionaste con {code}", code),
        (true, 2) => tr!("Tú y 1 persona más reaccionaron con {code}", code),
        (true, count) => tr!(
            "Tú y {others} personas más reaccionaron con {code}",
            others = count - 1,
            code
        ),
        (false, 1) => tr!("1 persona reaccionó con {code}", code),
        (false, count) => tr!("{count} personas reaccionaron con {code}", count, code),
    }
}

/// Who super-reacted with an emoji, as far as the account knows.
pub fn super_reacted_with(reaction: &rune_model::Reaction) -> String {
    let code = emoji::code(&reaction.emoji);
    match (reaction.me_burst, reaction.burst) {
        (true, 1) => tr!("Hiciste una súper reacción con {code}", code),
        (true, 2) => tr!(
            "Tú y 1 persona más hicieron súper reacciones con {code}",
            code
        ),
        (true, count) => tr!(
            "Tú y {others} personas más hicieron súper reacciones con {code}",
            others = count - 1,
            code
        ),
        (false, 1) => tr!("1 persona hizo una súper reacción con {code}", code),
        (false, count) => tr!(
            "{count} personas hicieron súper reacciones con {code}",
            count,
            code
        ),
    }
}

/// A super reaction's pill: in the colors the service gave it, outlined
/// when the account's, and only shown: RUNE never sends one.
fn burst_pill(message: &Message, reaction: &rune_model::Reaction, look: &Look) -> Element {
    let color = reaction
        .burst_colors
        .first()
        .map_or(look.accent, |color| look.ink(*color));
    let fill = Look::alpha(color, 48);
    let pill = Element::panel(
        Element::row(vec![
            Element::text(reaction.emoji.clone())
                .font_family(FontFamily::Sans)
                .font_size(18.0)
                .foreground(look.text)
                .centered_y(),
            Element::text(reaction.burst.to_string())
                .font_family(FontFamily::SansMedium)
                .font_size(14.0)
                .foreground(color)
                .centered_y(),
        ])
        .gap(6.0)
        .padding(6.0),
    )
    .size(None, Some(26.0))
    .radius(8.0)
    .background(fill)
    .border(if reaction.me_burst { color } else { fill }, 1.0)
    .accessible(limn::Access::new(limn::Role::Status).name(super_reacted_with(reaction)));
    let id = format!("react-burst:{}:{}", message.id, reaction.emoji);
    Tooltip::new(pill, id, super_reacted_with(reaction), look.menu_palette()).render()
}

/// The reactions under a message: each emoji and its count, in the accent
/// when the account reacted, its super reactions apart before it, and a
/// button to add one. A click adds or takes back the account's normal
/// reaction, but not in a sample.
fn reactions(message: &Message, can_react: bool, sample: bool, look: &Look) -> Element {
    let mut pills: Vec<Element> = Vec::new();
    for reaction in &message.reactions {
        if reaction.burst > 0 {
            pills.push(burst_pill(message, reaction, look));
        }
        if reaction.count == 0 {
            continue;
        }
        let (fill, edge, ink) = if reaction.me {
            (Look::alpha(look.accent, 40), look.accent, look.accent)
        } else {
            (look.card, look.border, look.muted)
        };
        let pill = Element::row(vec![
            Element::text(reaction.emoji.clone())
                .font_family(FontFamily::Sans)
                .font_size(18.0)
                .foreground(look.text)
                .centered_y(),
            Element::text(reaction.count.to_string())
                .font_family(FontFamily::SansMedium)
                .font_size(14.0)
                .foreground(ink)
                .centered_y(),
        ])
        .gap(6.0)
        .padding(6.0);
        if sample {
            pills.push(
                Element::panel(pill)
                    .size(None, Some(26.0))
                    .radius(8.0)
                    .background(fill)
                    .border(edge, 1.0),
            );
            continue;
        }
        let id = format!("react:{}:{}", message.id, reaction.emoji);
        let button = Element::clickable(id.clone(), pill)
            .size(None, Some(26.0))
            .radius(8.0)
            .background(fill)
            .hover_background(look.selected)
            .border(edge, 1.0)
            .accessible(
                limn::Access::new(limn::Role::Button)
                    .name(format!("{} {}", reaction.emoji, reaction.count))
                    .toggled(reaction.me),
            );
        pills.push(Tooltip::new(button, id, reacted_with(reaction), look.menu_palette()).render());
    }
    if can_react && !sample {
        let id = action(message, "react-more");
        let button = Element::clickable(id.clone(), crate::centered(icon(Icon::Emoji, look.muted)))
            .size(Some(36.0), Some(26.0))
            .radius(8.0)
            .background(look.card)
            .border(look.border, 1.0)
            .hover_background(look.selected)
            .accessible(limn::Access::new(limn::Role::Button).name(t("Agregar reacción")));
        pills.push(Tooltip::new(button, id, t("Agregar reacción"), look.menu_palette()).render());
    }
    pills.push(Element::spacer().flex(1.0));
    Element::row(pills).gap(6.0)
}

/// The menu's choices the bar shows itself while Shift is held, in the
/// order Serein's bar has them; «Eliminar» then deletes without asking.
const SHIFT_ACTIONS: [&str; 7] = [
    "pin",
    "unpin",
    "unread",
    "copy-link",
    "copy",
    "copy-id",
    "delete",
];

/// The bar over the message's top right corner while the pointer is on it:
/// three quick reactions, then adding a reaction, editing (own) or replying
/// (others'), and «Más», the whole menu. With Shift held the quick
/// reactions give way to the direct actions of [`SHIFT_ACTIONS`].
fn actions(view: &MessageView<'_>, live: &Live<'_>, look: &Look) -> Element {
    let message = view.message;
    let abilities = view.abilities;
    if message.delivery.is_local() {
        return Element::spacer();
    }
    let quick = if abilities.react && !live.shift {
        emoji::frequent(live.recent, 3)
    } else {
        Vec::new()
    };
    let mut bar: Vec<Element> = quick
        .iter()
        .map(|glyph| quick_reaction(message, "quick", glyph, 28.0, look))
        .collect();
    if !bar.is_empty() {
        bar.push(
            Element::panel(Element::spacer())
                .size(Some(1.0), Some(20.0))
                .background(look.border)
                .centered_y(),
        );
    }
    let mut row = MessageActions::new(action(message, "actions"), look.menu_palette())
        .visibility(MessageActionsVisibility::Always);
    if abilities.react {
        row = row.action(MessageAction::new(
            action(message, "react"),
            Icon::Emoji,
            t("Agregar reacción"),
        ));
    }
    if abilities.edit {
        row = row.action(MessageAction::new(
            action(message, "edit"),
            Icon::Edit,
            t("Editar"),
        ));
    } else if abilities.reply {
        row = row.action(MessageAction::new(
            action(message, "reply"),
            Icon::Reply,
            t("Responder"),
        ));
    }
    if live.shift {
        // The menu's own choices, so the bar offers only what it does.
        let offered: Vec<Choice> = choices(message, abilities, view.style.copy_id)
            .into_iter()
            .flatten()
            .collect();
        for what in SHIFT_ACTIONS {
            if let Some(choice) = offered.iter().find(|choice| choice.what == what) {
                row = row.action(MessageAction::new(
                    action(message, what),
                    choice.icon,
                    choice.label,
                ));
            }
        }
    }
    let row = row.overflow(
        menu_entries(message, abilities, view.style.copy_id, live.recent, look),
        live.more,
        t("Más"),
    );
    bar.push(row.render().centered_y());
    let panel = Element::panel(Element::row(bar).gap(2.0))
        .padding(3.0)
        .radius(8.0)
        .background(look.menu)
        .border(look.border, 1.0);
    let picker = view.reacting.then(|| {
        emoji::full_picker(
            emoji::PickerView {
                base: react_ids::BASE,
                query: live.query,
                recent: live.recent,
                placeholder: t("Encuentra la reacción perfecta"),
                tabs: false,
                expressions: None,
            },
            look,
        )
    });
    // Open, the picker and the menu keep the bar shown: a popover inside a
    // revealed element holds it.
    let panel = limn::Popover::new(
        panel.reveal_on_hover(action(message, "actions-panel")),
        picker.unwrap_or_else(Element::spacer),
        view.reacting,
        react_ids::DISMISS,
        look.menu_palette(),
    )
    .placement(limn::PopoverPlacement::LeftStart)
    .width(emoji::picker_size(false).0)
    .render();
    Element::column(vec![
        Element::row(vec![
            Element::spacer().flex(1.0),
            panel,
            gap(RIGHT - 32.0, 0.0),
        ]),
        Element::spacer().flex(1.0),
    ])
}

/// A quick reaction: the emoji, with its name and what a click does on hover.
/// `what` tells the bar's from the menu's, so each has its own ID.
fn quick_reaction(message: &Message, what: &str, glyph: &str, side: f32, look: &Look) -> Element {
    let id = action(message, &format!("{what}={glyph}"));
    let button = Element::clickable(
        id.clone(),
        crate::centered(
            Element::text(glyph.to_owned())
                .font_family(FontFamily::Sans)
                .font_size(side * 0.62)
                .foreground(look.text),
        ),
    )
    .size(Some(side), Some(side))
    .radius(6.0)
    .hover_background(look.selected)
    .accessibility_label(tr!("Reaccionar con {code}", code = emoji::code(glyph)));
    Tooltip::new(button, id, emoji::code(glyph), look.menu_palette())
        .description(t("Haz clic para reaccionar"))
        .render()
}

/// One choice of a message's menu.
struct Choice {
    what: &'static str,
    label: &'static str,
    icon: Icon,
    danger: bool,
}

const fn choice(what: &'static str, label: &'static str, icon: Icon) -> Choice {
    Choice {
        what,
        label,
        icon,
        danger: false,
    }
}

/// The groups of a message's menu, in order: react; edit and reply; copy,
/// pin and mark unread; delete; and the id in developer mode.
fn choices(message: &Message, abilities: Abilities, copy_id: bool) -> Vec<Vec<Choice>> {
    let plain = message.kind == MessageKind::Default;
    let sent = !message.delivery.is_local();
    let mut groups = Vec::new();
    if abilities.react && sent {
        groups.push(vec![choice("react", t("Agregar reacción"), Icon::Emoji)]);
    }
    let mut answer = Vec::new();
    if abilities.edit && plain && sent {
        answer.push(choice("edit", t("Editar mensaje"), Icon::Edit));
    }
    if abilities.reply && plain && sent {
        answer.push(choice("reply", t("Responder"), Icon::Reply));
    }
    if abilities.thread && plain && sent {
        answer.push(choice("thread", t("Crear hilo"), Icon::Chat));
    }
    // Message actions (P4): a poll is not forwarded, as in Discord.
    if abilities.forward && plain && sent && message.poll.is_none() {
        answer.push(choice("forward", t("Reenviar"), Icon::ArrowRight));
    }
    groups.push(answer);
    let mut keep = Vec::new();
    if plain && !message.content.is_empty() {
        keep.push(choice("copy", t("Copiar texto"), Icon::Copy));
    }
    if abilities.pin && plain && sent {
        keep.push(if message.pinned {
            choice("unpin", t("Desfijar mensaje"), Icon::Pin)
        } else {
            choice("pin", t("Fijar mensaje"), Icon::Pin)
        });
    }
    if sent {
        keep.push(choice(
            "unread",
            if abilities.read_sync {
                t("Marcar como no leído")
            } else {
                t("Marcar como no leído (solo en RUNE)")
            },
            Icon::Unread,
        ));
    }
    if abilities.copy_link && sent {
        keep.push(choice(
            "copy-link",
            t("Copiar enlace del mensaje"),
            Icon::ExternalLink,
        ));
    }
    groups.push(keep);
    // Message actions (P4): who reacted and voted, and ending a poll.
    let mut look = Vec::new();
    // Only normal reactions are listed; super reactions alone offer nothing.
    if abilities.reaction_list && sent && message.reactions.iter().any(|r| r.count > 0) {
        look.push(choice("reactions", t("Ver reacciones"), Icon::Emoji));
    }
    if abilities.poll_voters && sent && message.poll.is_some() {
        look.push(choice("voters", t("Ver votos"), Icon::ChartBar));
    }
    if abilities.end_poll && sent && message.poll.is_some() {
        look.push(choice("end-poll", t("Terminar encuesta"), Icon::Stop));
    }
    groups.push(look);
    if abilities.delete && sent {
        groups.push(vec![Choice {
            danger: true,
            ..choice("delete", t("Eliminar mensaje"), Icon::Trash)
        }]);
    }
    if copy_id && sent {
        groups.push(vec![choice(
            "copy-id",
            t("Copiar ID del mensaje"),
            Icon::Hash,
        )]);
    }
    groups.retain(|group| !group.is_empty());
    groups
}

/// The menu of «Más» and of a secondary click: four quick reactions, then
/// the choices, «Agregar reacción» with the frequent emoji beside it.
fn menu_entries(
    message: &Message,
    abilities: Abilities,
    copy_id: bool,
    recent: &[String],
    look: &Look,
) -> Vec<DropdownMenuEntry> {
    let mut entries = Vec::new();
    if abilities.react && !message.delivery.is_local() {
        let quick = emoji::frequent(recent, 4)
            .iter()
            .map(|glyph| {
                Element::panel(quick_reaction(message, "menu-quick", glyph, 46.0, look))
                    .radius(8.0)
                    .background(look.raised)
            })
            .collect();
        entries.push(DropdownMenuEntry::custom(
            Element::row(quick)
                .gap(5.0)
                .padding(4.0)
                .size(None, Some(54.0)),
        ));
    }
    for (index, group) in choices(message, abilities, copy_id).into_iter().enumerate() {
        if index > 0 {
            entries.push(DropdownMenuEntry::Separator);
        }
        for choice in group {
            let mut item =
                DropdownMenuItem::new(action(message, choice.what), choice.label).icon(choice.icon);
            if choice.what == "react" {
                item = item.submenu(
                    emoji::frequent(recent, 8)
                        .iter()
                        .map(|glyph| {
                            DropdownMenuEntry::Item(DropdownMenuItem::new(
                                action(message, &format!("menu-quick={glyph}")),
                                format!("{glyph}  {}", emoji::code(glyph)),
                            ))
                        })
                        .collect(),
                );
            }
            if choice.danger {
                item = item.destructive(look.danger);
            }
            entries.push(DropdownMenuEntry::Item(item));
        }
    }
    entries
}

/// A rule across the timeline with the day in its middle.
fn day_divider(label: &str, look: &Look) -> Element {
    let hairline = || {
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.border)
            .flex(1.0)
            .centered_y()
    };
    let label = Element::text(label.to_owned())
        .font_family(FontFamily::SansMedium)
        .font_size(12.0)
        .foreground(look.muted)
        .centered_y();
    Element::row(vec![
        gap(17.0, 0.0),
        Element::row(vec![hairline(), label, hairline()])
            .gap(6.0)
            .flex(1.0),
        gap(32.0, 0.0),
    ])
    .size(None, Some(32.0))
}

/// The rule before the first message not read, with «NUEVO» at its end.
fn new_divider(look: &Look) -> Element {
    let tag = Element::panel(crate::centered(
        Element::text(t("NUEVO"))
            .font_family(FontFamily::SansBold)
            .font_size(10.0)
            .foreground(Color::rgb(255, 255, 255)),
    ))
    .size(Some(52.5), Some(14.0))
    .radius(3.0)
    .background(look.danger);
    Element::row(vec![
        gap(17.0, 0.0),
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.danger)
            .flex(1.0)
            .centered_y(),
        tag.centered_y(),
        gap(48.0, 0.0),
    ])
    .size(None, Some(20.0))
}

/// The top of a conversation, above its first message.
fn start(title: &str, subtitle: &str, edit_channel: Option<&str>, look: &Look) -> Element {
    let mut column = vec![
        Element::panel(crate::centered(
            Element::icon(Icon::Hash)
                .large_icon(40.0)
                .foreground(look.text),
        ))
        .size(Some(72.5), Some(72.5))
        .radius(36.25)
        .background(look.raised),
        Element::text(title.to_owned())
            .font_family(FontFamily::SansBold)
            .font_size(32.0)
            .foreground(look.text),
        crate::text(subtitle, 16.0, look.muted),
    ];
    if let Some(id) = edit_channel {
        let button = Element::clickable(
            id.to_owned(),
            Element::row(vec![
                icon(Icon::Edit, look.text).centered_y(),
                line(t("Editar canal"), 14.0, look.text)
                    .shrink_to_fit()
                    .centered_y(),
            ])
            .gap(6.0)
            .padding(8.0),
        )
        .size(None, Some(30.0))
        .radius(6.0)
        .background(look.raised)
        .hover_background(look.selected)
        .accessible(limn::Access::new(limn::Role::Button).name(t("Editar canal")));
        column.push(Element::row(vec![button, Element::spacer().flex(1.0)]));
    }
    Element::row(vec![
        gap(LEFT, 0.0),
        Element::column(column).gap(8.0).flex(1.0),
        gap(RIGHT, 0.0),
    ])
    .padding(16.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;
    use rune_model::{ChannelId, MessageId, Reaction, UserId};
    use std::sync::Arc;

    fn message(id: u64, author: u64, minute: i64) -> Arc<Message> {
        Arc::new(Message {
            id: MessageId(id),
            channel: ChannelId(1),
            author: UserId(author),
            content: String::new(),
            timestamp: Timestamp(1_790_000_000_000 + minute * 60_000),
            edited: None,
            kind: MessageKind::Default,
            reply_to: None,
            mentions: Vec::new(),
            reactions: Vec::new(),
            attachments: Vec::new(),
            stickers: Vec::new(),
            text_references: Vec::new(),
            delivery: rune_model::Delivery::Sent,
            pinned: false,
            embeds: Vec::new(),
            forwarded: None,
            poll: None,
            components: Default::default(),
            call: None,
            mention_roles: Vec::new(),
            mention_everyone: false,
        })
    }

    fn view<'a>(message: &'a Message, abilities: Abilities) -> MessageView<'a> {
        MessageView {
            message,
            author: Author {
                marks: Default::default(),
                image: None,
                name: "Ada".into(),
                ink: Look::DARK.text,
                accent: Look::DARK.accent,
                bot: false,
            },
            continued: false,
            day: None,
            first_new: false,
            start: None,
            reply: None,
            mentions_me: false,
            abilities,
            reacting: false,
            author_card: None,
            width: 700.0,
            now: message.timestamp,
            style: MessageStyle::default(),
        }
    }

    fn laid(element: Element) -> limn::Frame {
        frame(
            Element::column(vec![element, Element::spacer().flex(1.0)]),
            Size::new(700.0, 600.0),
        )
        .1
    }

    const ALL: Abilities = Abilities {
        react: true,
        vote: false,
        reply: true,
        edit: true,
        delete: true,
        pin: true,
        read_sync: true,
        thread: true,
        forward: false,
        copy_link: false,
        reaction_list: false,
        poll_voters: false,
        end_poll: false,
    };

    fn emoji_source(name: &str) -> rune_model::RemoteImage {
        rune_model::RemoteImage {
            url: format!("https://cdn.test/emojis/{name}.webp"),
            kind: Default::default(),
        }
    }

    fn jumbo_source(name: &str) -> rune_model::RemoteImage {
        rune_model::RemoteImage {
            url: format!("https://cdn.test/emojis/{name}.webp?size=96"),
            kind: Default::default(),
        }
    }

    fn emoji_picture(digest: char) -> limn::RasterImage {
        limn::RasterImage::new(
            digest.to_string().repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "emoji",
        )
        .unwrap()
    }

    /// A message of `content` naming the server emoji `wave` and `dance`.
    fn with_emoji(content: &str) -> Message {
        let reference = |name: &str, token: &str| rune_model::TextReference {
            source: token.into(),
            target: rune_model::TextTarget::Emoji {
                name: name.into(),
                image: Some(emoji_source(name)),
                jumbo: Some(jumbo_source(name)),
                id: None,
            },
        };
        Message {
            content: content.into(),
            text_references: vec![
                reference("wave", "<:wave:1>"),
                reference("dance", "<a:dance:2>"),
            ],
            ..(*message(1, 2, 0)).clone()
        }
    }

    fn pictures(frame: &limn::Frame) -> Vec<limn::Rect> {
        frame
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::RasterImage { rect, image, .. } if image.alt() == "emoji" => {
                    Some(*rect)
                }
                limn::DrawCommand::Surface { rect, id, .. } if id.starts_with("animation:") => {
                    Some(*rect)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_servers_emoji_is_its_picture_in_the_line_and_its_name_until_it_loads() {
        let said = with_emoji("hola <:wave:1> y <a:dance:2> chau");
        let loaded = std::collections::HashMap::from([
            (emoji_source("wave"), emoji_picture('a')),
            (emoji_source("dance"), emoji_picture('b')),
        ]);
        let playing = std::collections::HashSet::from(["b".repeat(64)]);
        let shown = laid(live_row(
            view(&said, ALL),
            Live {
                attachment_images: Some(&loaded),
                animated: Some(&playing),
                ..Live::default()
            },
            &Look::DARK,
        ));
        let drawn = pictures(&shown);
        // 1.375 times the 16 px of the text, one still and one playing.
        assert_eq!(drawn.len(), 2, "{drawn:?}");
        assert!(drawn
            .iter()
            .all(|rect| rect.width == 22.0 && rect.height == 22.0));
        assert_eq!(drawn[0].y, drawn[1].y, "in one line");
        assert!(shown.commands.iter().any(|command| matches!(
            command,
            limn::DrawCommand::Surface { id, .. } if *id == animation_surface(&"b".repeat(64))
        )));
        let source: String = shown
            .text_layouts()
            .iter()
            .map(|block| block.source.clone())
            .collect();
        assert!(source.contains("hola :wave: y :dance: chau"), "{source}");
        // Not loaded: its name, never its token.
        let waiting = laid(live_row(view(&said, ALL), Live::default(), &Look::DARK));
        assert!(pictures(&waiting).is_empty());
        let words = texts(&waiting);
        assert!(
            words.iter().any(|text| text.contains(":wave:")),
            "{words:?}"
        );
        assert!(!words.iter().any(|text| text.contains('<')), "{words:?}");
    }

    #[test]
    fn emoji_alone_are_larger_but_not_in_a_compact_line_nor_beside_words() {
        let loaded = std::collections::HashMap::from([
            (emoji_source("wave"), emoji_picture('a')),
            (emoji_source("dance"), emoji_picture('b')),
            (jumbo_source("wave"), emoji_picture('c')),
            (jumbo_source("dance"), emoji_picture('d')),
        ]);
        // Emoji alone draw their larger picture, also in a compact line.
        let drawn = |content: &str, compact: bool| {
            let said = with_emoji(content);
            let mut shown = view(&said, ALL);
            shown.style.compact = compact;
            let frame = laid(live_row(
                shown,
                Live {
                    attachment_images: Some(&loaded),
                    ..Live::default()
                },
                &Look::DARK,
            ));
            frame.commands.iter().find_map(|command| match command {
                limn::DrawCommand::RasterImage { image, .. } if image.alt() == "emoji" => {
                    image.sha256().chars().next()
                }
                _ => None,
            })
        };
        assert_eq!(drawn("<:wave:1>", false), Some('c'));
        assert_eq!(drawn("<:wave:1>", true), Some('c'));
        assert_eq!(drawn("ok <:wave:1>", false), Some('a'));
        let side = |content: &str, compact: bool| {
            let said = with_emoji(content);
            let mut shown = view(&said, ALL);
            shown.style.compact = compact;
            let frame = laid(live_row(
                shown,
                Live {
                    attachment_images: Some(&loaded),
                    ..Live::default()
                },
                &Look::DARK,
            ));
            pictures(&frame).first().map(|rect| rect.width)
        };
        // 48 px, as the official client draws a message of emoji alone.
        assert_eq!(
            side("<:wave:1> <a:dance:2>", false).map(f32::round),
            Some(48.0)
        );
        assert_eq!(side(" <:wave:1>\n👍🏽 ", false).map(f32::round), Some(48.0));
        assert_eq!(side("<:wave:1> <a:dance:2>", true), Some(22.0));
        assert_eq!(side("ok <:wave:1>", false), Some(22.0));
        assert_eq!(side("`<:wave:1>`", false), None, "code is not an emoji");
    }

    #[test]
    fn only_emoji_counts_unicode_sequences_once_and_stops_past_thirty() {
        let none: [rune_model::TextReference; 0] = [];
        assert!(text::only_emoji("👍 🇦🇷 👨‍👩‍👧 ❤️", &none));
        assert!(!text::only_emoji("👍 ok", &none));
        assert!(!text::only_emoji("   ", &none));
        assert!(!text::only_emoji("||👍||", &none));
        assert!(text::only_emoji(&"👍".repeat(30), &none));
        assert!(!text::only_emoji(&"👍".repeat(31), &none));
        // By the set's sequences (2026-10-18): a keycap and an arrow of the
        // set are emoji; a digit, `→` (not in it) and `©` are text.
        assert!(text::only_emoji("1\u{fe0f}\u{20e3} #\u{20e3}", &none));
        assert!(text::only_emoji("➡ ↔", &none));
        assert!(!text::only_emoji("1", &none));
        assert!(!text::only_emoji("→ ©", &none));
        assert!(!text::only_emoji("😀\u{fe0e}", &none), "VS15 asks for text");
    }

    #[test]
    fn unicode_emoji_alone_are_their_pictures_the_size_of_a_servers() {
        let drawn = |content: &str, compact: bool| {
            let said = Message {
                content: content.into(),
                ..(*message(1, 2, 0)).clone()
            };
            let mut shown = view(&said, ALL);
            shown.style.compact = compact;
            let frame = laid(live_row(shown, Live::default(), &Look::DARK));
            frame
                .commands
                .iter()
                .filter_map(|command| match command {
                    limn::DrawCommand::RasterImage { rect, image, .. } => {
                        Some((image.alt().to_owned(), rect.width.round()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            drawn("👍 1\u{fe0f}\u{20e3}", false),
            [
                ("👍".to_owned(), 48.0),
                ("1\u{fe0f}\u{20e3}".to_owned(), 48.0)
            ]
        );
        assert!(drawn("👍 ok", false).is_empty(), "beside words, its letter");
        assert_eq!(
            drawn("👍", true),
            [("👍".to_owned(), 22.0)],
            "a compact line's size, as a server's"
        );
    }

    #[test]
    fn each_emoji_of_a_message_opens_its_card() {
        let said = with_emoji("hola <:wave:1> 👍 y 1\u{fe0f}\u{20e3}");
        let frame = laid(live_row(view(&said, ALL), Live::default(), &Look::DARK));
        let ids = controls(&frame);
        for expected in [
            "msg:1:emoji:0:<:wave:1>",
            "msg:1:emoji:1:👍",
            "msg:1:emoji:2:1\u{fe0f}\u{20e3}",
        ] {
            assert!(ids.iter().any(|id| id == expected), "{expected} in {ids:?}");
        }
        // Not in a sample.
        let mut sample = view(&said, ALL);
        sample.style.sample = true;
        let shown = laid(live_row(sample, Live::default(), &Look::DARK));
        assert!(!controls(&shown).iter().any(|id| id.contains(":emoji:")));
    }

    #[test]
    fn a_reply_shows_the_emoji_of_the_line_it_answers() {
        let mut said = (*message(3, 2, 1)).clone();
        said.content = "sí".into();
        said.reply_to = Some(MessageId(1));
        let mut shown = view(&said, ALL);
        shown.reply = Some(Reply {
            author: Some(shown.author.clone()),
            snippet: "mira <:wave:1> esto".into(),
            emoji: vec![crate::identity::TextPicture {
                token: "<:wave:1>".into(),
                name: "wave".into(),
                image: Some(emoji_picture('a')),
                playing: false,
            }],
        });
        let frame = laid(live_row(shown, Live::default(), &Look::DARK));
        let drawn = pictures(&frame);
        // 1.25 times the reply's 14 px, as the official client draws it there.
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].width, 17.5);
        assert!(!texts(&frame).iter().any(|text| text.contains("<:wave:1>")));
    }

    #[test]
    fn a_reply_starts_where_the_name_does_and_its_spine_at_the_avatar() {
        for compact in [false, true] {
            let mut said = (*message(3, 2, 1)).clone();
            said.content = "sí".into();
            said.reply_to = Some(MessageId(1));
            let mut shown = view(&said, ALL);
            shown.style.compact = compact;
            shown.author.name = "Ada".into();
            shown.reply = Some(Reply {
                author: Some(Author {
                    name: "Grace".into(),
                    ..shown.author.clone()
                }),
                snippet: "que user?".into(),
                emoji: Vec::new(),
            });
            let frame = laid(live_row(shown, Live::default(), &Look::DARK));
            let x = |name: &str| {
                frame
                    .commands
                    .iter()
                    .find_map(|command| match command {
                        limn::DrawCommand::Text { position, text, .. } if text == name => {
                            Some(position.x)
                        }
                        _ => None,
                    })
                    .unwrap()
            };
            // The small mark, 16 and 4 of room, then the name it answers.
            assert_eq!(x("Grace") - 20.0, x("Ada"), "compact={compact}");
            let spine = frame
                .commands
                .iter()
                .find_map(|command| match command {
                    limn::DrawCommand::StrokeRect { rect, radius, .. } if *radius == 6.0 => {
                        Some(*rect)
                    }
                    _ => None,
                })
                .unwrap();
            let center = if compact {
                16.0 + (COMPACT_GUTTER - 16.0) / 2.0
            } else {
                LEFT + AVATAR / 2.0
            };
            assert_eq!(spine.x + 1.0, center, "compact={compact}");
        }
    }

    #[test]
    fn message_actions_show_only_where_they_apply() {
        let can = Abilities {
            forward: true,
            copy_link: true,
            reaction_list: true,
            poll_voters: true,
            end_poll: true,
            ..ALL
        };
        let said = |message: &Message| -> Vec<&'static str> {
            choices(message, can, false)
                .into_iter()
                .flatten()
                .map(|choice| choice.what)
                .collect()
        };
        let mut plain = (*message(1, 1, 0)).clone();
        plain.reactions.clear();
        let shown = said(&plain);
        assert!(shown.contains(&"forward") && shown.contains(&"copy-link"));
        assert!(!shown.contains(&"reactions") && !shown.contains(&"voters"));
        // Super reactions alone have nobody to list.
        let mut burst_only = plain.clone();
        burst_only.reactions.push(rune_model::Reaction {
            emoji: "🚀".into(),
            count: 0,
            me: false,
            burst: 2,
            me_burst: false,
            burst_colors: Vec::new(),
        });
        assert!(!said(&burst_only).contains(&"reactions"));
        plain.toggle_reaction("👍");
        assert!(said(&plain).contains(&"reactions"));
        let mut poll = plain.clone();
        poll.poll = Some(rune_model::Poll::default());
        let shown = said(&poll);
        assert!(!shown.contains(&"forward"), "a poll is not forwarded");
        assert!(shown.contains(&"voters") && shown.contains(&"end-poll"));
        let mut pending = plain;
        pending.delivery = Delivery::Sending;
        assert!(!said(&pending)
            .iter()
            .any(|what| matches!(*what, "forward" | "copy-link" | "reactions")));
    }

    #[test]
    fn invitation_card_keeps_the_link_and_requires_a_whole_adapter_reference() {
        use rune_model::{TextReference, TextTarget};
        let mut said = (*message(1, 1, 0)).clone();
        said.content = "https://fixture.invalid/invite/local".into();
        said.text_references = vec![TextReference {
            source: said.content.clone(),
            target: TextTarget::Invitation,
        }];
        let laid = laid(message_row(view(&said, ALL), &Look::DARK));
        let painted = texts(&laid).join(" ");
        assert!(painted.contains("Enlace de invitación"));
        assert!(
            painted.contains(&said.content),
            "the original URL stays visible"
        );
        assert!(controls(&laid).contains(&invitation_action(&said, 0, "review")));
        let denied = self::laid(live_row(
            view(&said, ALL),
            Live {
                invitation_denied: Some("Conecta una cuenta para consultar esta invitación"),
                ..Live::default()
            },
            &Look::DARK,
        ));
        assert!(texts(&denied).join(" ").contains("Conecta una cuenta"));
        assert!(!controls(&denied).contains(&invitation_action(&said, 0, "review")));
        let edited = self::laid(live_row(
            view(&said, ALL),
            Live {
                editing: Some(&said.content),
                ..Live::default()
            },
            &Look::DARK,
        ));
        assert!(!texts(&edited).join(" ").contains("Enlace de invitación"));
        let mut sample = view(&said, ALL);
        sample.style.sample = true;
        assert!(!texts(&self::laid(message_row(sample, &Look::DARK)))
            .join(" ")
            .contains("Enlace de invitación"));

        // A reference the text no longer has is not a card.
        said.content = "see https://fixture.invalid/invite/other".into();
        let laid = self::laid(message_row(view(&said, ALL), &Look::DARK));
        assert!(!texts(&laid).join(" ").contains("Enlace de invitación"));
        said.content = said.text_references[0].source.clone();
        said.text_references.clear();
        let laid = self::laid(message_row(view(&said, ALL), &Look::DARK));
        assert!(!texts(&laid).join(" ").contains("Enlace de invitación"));
    }

    #[test]
    fn every_invitation_amid_text_gets_its_own_card_and_words() {
        use rune_model::{TextReference, TextTarget};
        let mut said = (*message(1, 1, 0)).clone();
        let links: Vec<String> = (0..12)
            .map(|index| format!("https://fixture.invalid/invite/k{index}"))
            .collect();
        said.content = format!("Vengan a todos: {}", links.join(" y "));
        said.text_references = links
            .iter()
            .map(|link| TextReference {
                source: link.clone(),
                target: TextTarget::Invitation,
            })
            .collect();
        assert_eq!(invitation_links(&said).len(), INVITATION_CARDS);
        let laid = laid(message_row(view(&said, ALL), &Look::DARK));
        let reviews = controls(&laid)
            .into_iter()
            .filter(|id| id.starts_with(&format!("msg:{}:invitation:", said.id)))
            .count();
        assert_eq!(reviews, INVITATION_CARDS, "one card per invitation");

        let invited = |kind, member, joins_here| {
            InvitationCard::Server(InvitedServer {
                kind,
                name: String::new(),
                image: None,
                online: None,
                members: Some(4),
                member,
                joins_here,
                event: None,
            })
        };
        let cards = [
            invited(InvitedKind::Group, false, true),
            invited(InvitedKind::Friend, false, false),
            invited(InvitedKind::Guest, false, true),
            invited(InvitedKind::Friend, true, true),
        ];
        let shown = self::laid(live_row(
            view(&said, ALL),
            Live {
                invitations: &cards,
                ..Live::default()
            },
            &Look::DARK,
        ));
        let painted = texts(&shown).join(" ");
        for part in [
            "Te invitaron a unirte a un grupo",
            "Grupo sin nombre",
            "Te invitaron a ser amigos",
            "Aceptar amistad",
            "Te invitaron a entrar como invitado",
            "Como invitado",
            "Ya son amigos",
            "Enviar mensaje",
            // The cards without an answer still offer to review the link.
            "Revisar invitación",
        ] {
            assert!(painted.contains(part), "{part} in {painted}");
        }
        let ids = controls(&shown);
        assert!(ids.contains(&invitation_action(&said, 0, "join")));
        assert!(ids.contains(&invitation_action(&said, 1, "join")));
        assert!(ids.contains(&invitation_action(&said, 3, "open")));
        assert!(ids.contains(&invitation_action(&said, 4, "review")));
    }

    #[test]
    fn cards_forwards_and_records_are_drawn_without_links_in_samples() {
        use rune_model::{Embed, EmbedField, Forwarded};
        let mut said = (*message(1, 1, 0)).clone();
        said.embeds = vec![Embed {
            provider: Some("Sitio".into()),
            title: Some("Un título".into()),
            url: Some("https://example.test/a".into()),
            description: Some("Texto **fuerte**".into()),
            fields: vec![
                EmbedField {
                    name: "Campo".into(),
                    value: "uno".into(),
                    inline: true,
                },
                EmbedField {
                    name: "Otro".into(),
                    value: "dos".into(),
                    inline: true,
                },
            ],
            footer: Some("Pie".into()),
            color: Some(0x5865F2),
            ..Embed::default()
        }];
        said.forwarded = Some(Forwarded {
            content: "copiado".into(),
            timestamp: said.timestamp,
            ..Forwarded::default()
        });
        let laid = laid(message_row(view(&said, ALL), &Look::DARK));
        let painted = texts(&laid).join(" ");
        for part in [
            "Sitio",
            "Un título",
            "fuerte",
            "Campo",
            "dos",
            "Pie",
            "Reenviado",
            "copiado",
        ] {
            assert!(painted.contains(part), "{part} in {painted}");
        }
        let link = "markdown_link:embed:0:https://example.test/a".to_string();
        assert!(controls(&laid).contains(&link));
        let mut sample = view(&said, ALL);
        sample.style.sample = true;
        assert!(!controls(&self::laid(message_row(sample, &Look::DARK))).contains(&link));

        said.embeds.clear();
        said.forwarded = None;
        for (kind, expected) in [
            (MessageKind::Pinned, "Ada fijó un mensaje en este canal."),
            (
                MessageKind::Boost { tier: 2 },
                "¡El servidor alcanzó el nivel 2!",
            ),
            (MessageKind::Call, "Ada inició una llamada."),
            (MessageKind::Other(46), "todavía no se puede mostrar"),
        ] {
            said.kind = kind;
            // A record's names are runs of their own in its line.
            let painted = texts(&self::laid(message_row(view(&said, ALL), &Look::DARK))).concat();
            assert!(painted.contains(expected), "{kind:?}: {painted}");
        }
    }

    /// Message content (contenido-de-mensajes): the names of a record open
    /// their profiles, and a call's record says whether the account missed
    /// it and how long it lasted.
    #[test]
    fn records_name_people_as_links_and_calls_say_missed_and_how_long() {
        use rune_model::{MessageCall, UserId};
        let mut said = (*message(1, 1, 0)).clone();
        said.kind = MessageKind::RecipientAdd;
        said.mentions = vec![UserId(7)];
        let names = text::Names {
            display_names: std::collections::HashMap::from([(UserId(7), "Bruno".to_owned())]),
            ..text::Names::default()
        };
        let live = |me| Live {
            names: Some(&names),
            me,
            ..Live::default()
        };
        let laid = self::laid(live_row(view(&said, ALL), live(None), &Look::DARK));
        assert!(texts(&laid)
            .concat()
            .contains("Ada añadió a Bruno al grupo."));
        let shown = controls(&laid);
        assert!(
            shown.contains(&format!("member:{}", said.author)),
            "{shown:?}"
        );
        assert!(shown.contains(&"member:7".to_owned()), "{shown:?}");
        // A sample opens nothing.
        let mut sample = view(&said, ALL);
        sample.style.sample = true;
        assert!(!controls(&self::laid(message_row(sample, &Look::DARK)))
            .iter()
            .any(|id| id.starts_with("member:")));

        said.kind = MessageKind::Call;
        said.mentions.clear();
        let me = UserId(99);
        let line = |said: &Message, me| {
            texts(&self::laid(live_row(
                view(said, ALL),
                live(me),
                &Look::DARK,
            )))
            .concat()
        };
        // Unknown, or still going on: it was started.
        assert!(line(&said, Some(me)).contains("Ada inició una llamada."));
        said.call = Some(MessageCall {
            participants: vec![said.author],
            ended: None,
        });
        assert!(line(&said, Some(me)).contains("Ada inició una llamada."));
        said.call = Some(MessageCall {
            participants: vec![said.author, me],
            ended: Some(said.timestamp.plus_minutes(12)),
        });
        assert!(line(&said, Some(me)).contains("Ada inició una llamada que duró 12 minutos."));
        said.call = Some(MessageCall {
            participants: vec![said.author],
            ended: Some(said.timestamp.plus_minutes(1)),
        });
        assert!(
            line(&said, Some(me)).contains("Te perdiste una llamada de Ada que duró un minuto.")
        );
        // A name or a text never passes for another's place in the line.
        said.kind = MessageKind::ChannelRename;
        said.call = None;
        said.content = "nuevo \u{E001} nombre".into();
        assert!(line(&said, None).contains("cambió el nombre del canal: nuevo  nombre"));
    }

    /// Message content (contenido-de-mensajes): a forward's text resolves
    /// what it names, and an explicit card waits behind the filter's cover.
    #[test]
    fn forwards_resolve_their_names_and_explicit_cards_wait_for_a_click() {
        use rune_model::{Embed, EmbedImage, Forwarded, RemoteImage, TextReference, TextTarget};
        let mut said = (*message(1, 1, 0)).clone();
        said.forwarded = Some(Forwarded {
            content: "mira <@7>".into(),
            timestamp: said.timestamp,
            text_references: vec![TextReference {
                source: "<@7>".into(),
                target: TextTarget::User(UserId(7)),
            }],
            ..Forwarded::default()
        });
        let names = text::Names {
            display_names: std::collections::HashMap::from([(UserId(7), "Bruno".to_owned())]),
            ..text::Names::default()
        };
        let live = Live {
            names: Some(&names),
            ..Live::default()
        };
        let laid = self::laid(live_row(view(&said, ALL), live, &Look::DARK));
        assert!(texts(&laid).iter().any(|text| text == "@Bruno"));
        assert!(controls(&laid).contains(&"member:7".to_owned()));

        said.forwarded = None;
        let picture = EmbedImage {
            source: RemoteImage {
                url: "https://media.test/a.png".into(),
                kind: rune_model::ImageKind::AttachmentPreview,
            },
            dimensions: (400, 300),
        };
        said.embeds = vec![Embed {
            kind: rune_model::EmbedKind::Image,
            image: Some(picture.clone()),
            explicit: true,
            ..Embed::default()
        }];
        let reveal = format!("msg:1:spoiler:{}", card_spoiler(false, 0));
        let view_picture = card_picture("msg:1", false, 0, 0);
        let hidden = self::laid(live_row(
            view(&said, ALL),
            Live {
                obscure_explicit: true,
                ..Live::default()
            },
            &Look::DARK,
        ));
        assert!(texts(&hidden)
            .iter()
            .any(|text| text == "CONTENIDO EXPLÍCITO"));
        assert!(controls(&hidden).contains(&reveal));
        assert!(!controls(&hidden).contains(&view_picture));
        let shown = self::laid(live_row(view(&said, ALL), Live::default(), &Look::DARK));
        assert!(controls(&shown).contains(&view_picture));
        let revealed = [card_spoiler(false, 0)];
        let opened = self::laid(live_row(
            view(&said, ALL),
            Live {
                obscure_explicit: true,
                revealed: &revealed,
                ..Live::default()
            },
            &Look::DARK,
        ));
        assert!(controls(&opened).contains(&view_picture));
        // Its link behind a spoiler too: it says what the filter hides
        // before the one reveal that shows both.
        said.content = "||https://media.test/a.png||".into();
        said.embeds[0].url = Some("https://media.test/a.png".into());
        let spoiled = self::laid(live_row(
            view(&said, ALL),
            Live {
                obscure_explicit: true,
                ..Live::default()
            },
            &Look::DARK,
        ));
        assert!(texts(&spoiled)
            .iter()
            .any(|text| text == "CONTENIDO EXPLÍCITO"));
        assert!(controls(&spoiled).contains(&reveal));
        assert!(!controls(&spoiled).contains(&view_picture));
    }

    #[test]
    fn polls_offer_votes_only_where_the_account_may_vote_and_results_after() {
        use rune_model::{Poll, PollAnswer};
        let mut said = (*message(1, 1, 0)).clone();
        let answer = |id, text: &str, votes| PollAnswer {
            id,
            text: text.into(),
            emoji: None,
            votes,
            me: false,
        };
        said.poll = Some(Poll {
            question: "¿Cuándo?".into(),
            answers: vec![answer(1, "Hoy", 3), answer(2, "Mañana", 1)],
            expiry: Some(said.timestamp.plus_minutes(60)),
            results_known: true,
            ..Poll::default()
        });
        let can = Abilities { vote: true, ..ALL };
        let pick = |id| format!("msg:{}:poll:answer:{id}", said.id);
        let laid = laid(message_row(view(&said, can), &Look::DARK));
        let painted = texts(&laid).join(" ");
        assert!(controls(&laid).contains(&pick(1)) && controls(&laid).contains(&pick(2)));
        assert!(!painted.contains("75 %"), "results wait for the vote");
        assert!(!painted.contains("Votar"), "a single choice votes on click");
        // Without the ability: read-only results and the reason, no choices.
        let denied = self::laid(live_row(
            view(&said, ALL),
            Live {
                vote_denied: Some("Este proveedor no permite votar en encuestas"),
                ..Live::default()
            },
            &Look::DARK,
        ));
        let painted = texts(&denied).join(" ");
        assert!(painted.contains("75 %") && painted.contains("no permite votar"));
        assert!(!controls(&denied).contains(&pick(1)));
        // Multiselect picks first, then «Votar», only with something picked.
        said.poll.as_mut().unwrap().multiselect = true;
        let vote = format!("msg:{}:poll:vote", said.id);
        let none = self::laid(message_row(view(&said, can), &Look::DARK));
        assert!(texts(&none).join(" ").contains("Votar"));
        assert!(!controls(&none).contains(&vote), "nothing picked yet");
        let picked = self::laid(live_row(
            view(&said, can),
            Live {
                poll_choice: &[2],
                ..Live::default()
            },
            &Look::DARK,
        ));
        assert!(controls(&picked).contains(&vote));
        // Voted: results and «Quitar voto».
        said.poll.as_mut().unwrap().vote(&[2]);
        let voted = self::laid(message_row(view(&said, can), &Look::DARK));
        let painted = texts(&voted).join(" ");
        assert!(painted.contains("2 votos · 40 %"));
        assert!(controls(&voted).contains(&action(&said, "poll:remove")));
        assert!(!controls(&voted).contains(&pick(1)));
        // Ended: results only, nothing to change.
        said.poll.as_mut().unwrap().finalized = true;
        let ended = self::laid(message_row(view(&said, can), &Look::DARK));
        assert!(texts(&ended).join(" ").contains("La encuesta terminó"));
        let ids = controls(&ended);
        assert!(!ids.iter().any(|id| id.contains(":poll:")), "{ids:?}");
    }

    #[test]
    fn polls_media_cards_and_poll_results_are_drawn() {
        use rune_model::{
            Embed, EmbedField, EmbedImage, EmbedKind, ImageKind, Poll, PollAnswer, RemoteImage,
        };
        let mut said = (*message(1, 1, 0)).clone();
        let answer = |id, text: &str, votes, me| PollAnswer {
            id,
            text: text.into(),
            emoji: None,
            votes,
            me,
        };
        said.poll = Some(Poll {
            question: "¿Cuándo nos vemos?".into(),
            answers: vec![answer(1, "Hoy", 3, true), answer(2, "Mañana", 1, false)],
            expiry: Some(said.timestamp.plus_minutes(5 * 60)),
            results_known: true,
            ..Poll::default()
        });
        let painted = texts(&laid(message_row(view(&said, ALL), &Look::DARK))).join(" ");
        for part in [
            "¿Cuándo nos vemos?",
            "Hoy",
            "3 votos · 75 %",
            "1 voto · 25 %",
            "4 votos",
            "Quedan 5 h",
            "Elige una respuesta",
        ] {
            assert!(painted.contains(part), "{part} in {painted}");
        }
        // A narrow poll moves each count under its answer and keeps it.
        let mut narrow = view(&said, ALL);
        narrow.width = 380.0;
        let painted = texts(&laid(message_row(narrow, &Look::DARK))).join(" ");
        assert!(painted.contains("3 votos · 75 %") && painted.contains("Mañana"));
        let poll = said.poll.as_mut().unwrap();
        poll.results_known = false;
        poll.finalized = true;
        let painted = texts(&laid(message_row(view(&said, ALL), &Look::DARK))).join(" ");
        assert!(painted.contains("Resultados todavía no recibidos"));
        assert!(painted.contains("La encuesta terminó"));
        assert!(!painted.contains("0 votos"), "unknown counts are not zero");
        said.poll = None;

        let link = "https://example.test/cat.png";
        let picture = EmbedImage {
            source: RemoteImage {
                url: "https://images-ext-1.discordapp.net/external/a/https/example.test/cat.png"
                    .into(),
                kind: ImageKind::AttachmentPreview,
            },
            dimensions: (800, 600),
        };
        said.content = link.into();
        said.embeds = vec![Embed {
            kind: EmbedKind::Image,
            url: Some(link.into()),
            thumbnail: Some(picture.clone()),
            ..Embed::default()
        }];
        let laid = laid(message_row(view(&said, ALL), &Look::DARK));
        assert!(
            !texts(&laid).join(" ").contains(link),
            "the picture stands in for its link"
        );
        // A click opens the picture in the viewer, as an attachment's does.
        assert!(controls(&laid).contains(&card_picture(&format!("msg:{}", said.id), false, 0, 0)));
        let mut plain = view(&said, ALL);
        plain.style.image_previews = false;
        assert!(
            texts(&self::laid(message_row(plain, &Look::DARK)))
                .join(" ")
                .contains(link),
            "without previews the link reads as text"
        );
        said.content = format!("mira {link}");
        assert!(
            texts(&self::laid(message_row(view(&said, ALL), &Look::DARK)))
                .join(" ")
                .contains("mira")
        );

        said.content.clear();
        said.embeds = vec![Embed {
            kind: EmbedKind::Video,
            title: Some("Charla".into()),
            url: Some("https://video.test/v".into()),
            thumbnail: Some(picture),
            ..Embed::default()
        }];
        let laid = self::laid(message_row(view(&said, ALL), &Look::DARK));
        assert_eq!(
            controls(&laid)
                .iter()
                .filter(|id| *id == "markdown_link:embed:0:https://video.test/v")
                .count(),
            2,
            "title and poster open the page"
        );

        // A closed poll's record: its line, then the result and the way back
        // to the poll, as the reference client draws them.
        said.kind = MessageKind::PollResult;
        said.reply_to = Some(MessageId(1));
        let result = |fields: &[(&str, &str)]| Embed {
            kind: EmbedKind::PollResult,
            fields: fields
                .iter()
                .map(|(name, value)| EmbedField {
                    name: (*name).into(),
                    value: (*value).into(),
                    inline: false,
                })
                .collect(),
            ..Embed::default()
        };
        let question = ("poll_question_text", "¿Pizza?");
        for (fields, expected, absent) in [
            (
                vec![
                    question,
                    ("victor_answer_id", "1"),
                    ("victor_answer_text", "Sí"),
                    ("victor_answer_votes", "3"),
                    ("total_votes", "5"),
                    ("victor_answer_emoji_name", "🍕"),
                ],
                ["Sí", "Respuesta ganadora • 60 %", "🍕"],
                "empate",
            ),
            (
                vec![question, ("victor_answer_votes", "2"), ("total_votes", "4")],
                ["El resultado fue un empate", "50 %", "Ver encuesta"],
                "ganadora",
            ),
            (
                vec![question, ("total_votes", "0")],
                ["No hubo un ganador", "Ver encuesta", "¿Pizza?"],
                "%",
            ),
        ] {
            said.embeds = vec![result(&fields)];
            let laid = self::laid(message_row(view(&said, ALL), &Look::DARK));
            let painted = texts(&laid).join(" ");
            assert!(texts(&laid)
                .concat()
                .contains("La encuesta «¿Pizza?» de Ada terminó."));
            for part in expected {
                assert!(painted.contains(part), "{part} in {painted}");
            }
            assert!(!painted.contains(absent), "{absent} in {painted}");
            assert!(controls(&laid).contains(&action(&said, "poll-result")));
            assert_eq!(
                painted.matches("¿Pizza?").count(),
                1,
                "no second, generic card"
            );
        }
        said.reply_to = None;
        let laid = self::laid(message_row(view(&said, ALL), &Look::DARK));
        assert!(!controls(&laid).contains(&action(&said, "poll-result")));

        // A server emoji's picture stands for its name once it is loaded.
        said.embeds = vec![result(&[
            question,
            ("victor_answer_id", "1"),
            ("victor_answer_text", "Sí"),
            ("victor_answer_emoji_id", "22"),
            ("victor_answer_emoji_name", "pizza"),
        ])];
        let source = rune_model::RemoteImage {
            url: "https://cdn.example.test/emojis/22.webp".into(),
            kind: rune_model::ImageKind::Artwork,
        };
        said.embeds[0].thumbnail = Some(rune_model::EmbedImage {
            source: source.clone(),
            dimensions: (128, 128),
        });
        let painted = |images| {
            let live = Live {
                attachment_images: images,
                ..Live::default()
            };
            texts(&self::laid(live_row(view(&said, ALL), live, &Look::DARK))).join(" ")
        };
        assert!(painted(None).contains(":pizza:"));
        let raster = limn::RasterImage::new(
            "0".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            Arc::from("AAAA"),
            "Emoji",
        )
        .unwrap();
        let loaded = std::collections::HashMap::from([(source, raster)]);
        assert!(!painted(Some(&loaded)).contains(":pizza:"));
    }

    #[test]
    fn a_buttons_server_emoji_shows_its_picture_once_loaded() {
        use rune_model::components::Button;
        use rune_model::{ButtonStyle, Component, Components, ImageKind, RemoteImage};
        use std::collections::HashSet;
        let source = RemoteImage {
            url: "https://cdn.example.test/emojis/22.webp".into(),
            kind: ImageKind::Artwork,
        };
        let mut said = (*message(1, 1, 0)).clone();
        said.content.clear();
        said.components = Components {
            parts: vec![Component::Row(
                [Some("Fiesta"), None]
                    .map(|label| {
                        Component::Button(Button {
                            label: label.map(str::to_owned),
                            emoji: Some(":fiesta:".into()),
                            picture: Some(source.clone()),
                            style: ButtonStyle::Primary,
                            ..Button::default()
                        })
                    })
                    .into(),
            )],
            whole: true,
            ..Components::default()
        };
        let laid = |images| {
            let live = Live {
                attachment_images: images,
                ..Live::default()
            };
            self::laid(live_row(view(&said, ALL), live, &Look::DARK))
        };
        let pictures = |frame: &limn::Frame| {
            frame
                .commands
                .iter()
                .filter(|command| matches!(command, limn::DrawCommand::RasterImage { .. }))
                .count()
        };
        // While it loads, or if it fails, its name stands in.
        let waiting = laid(None);
        let painted = texts(&waiting).join(" ");
        assert!(painted.contains(":fiesta: Fiesta"), "{painted}");
        assert_eq!(pictures(&waiting), 0);
        let raster = limn::RasterImage::new(
            "0".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            Arc::from("AAAA"),
            "Emoji",
        )
        .unwrap();
        let loaded = std::collections::HashMap::from([(source.clone(), raster.clone())]);
        let shown = laid(Some(&loaded));
        let painted = texts(&shown).join(" ");
        assert!(
            painted.contains("Fiesta") && !painted.contains(":fiesta:"),
            "{painted}"
        );
        assert_eq!(pictures(&shown), 2, "with a label and alone");
        // One that moves plays on its surface while the host plays it
        // (limn-teclado-y-superficies); with motion reduced it is not played.
        let playing = HashSet::from([raster.sha256().to_owned()]);
        let moving = self::laid(live_row(
            view(&said, ALL),
            Live {
                attachment_images: Some(&loaded),
                animated: Some(&playing),
                ..Live::default()
            },
            &Look::DARK,
        ));
        let surfaces = moving
            .commands
            .iter()
            .filter(|command| matches!(command, limn::DrawCommand::Surface { id, .. } if *id == animation_surface(raster.sha256())))
            .count();
        assert_eq!((surfaces, pictures(&moving)), (2, 0));
    }

    #[test]
    fn a_menus_server_emoji_shows_in_its_option_and_moves_while_played() {
        use rune_model::components::{Choice, MenuOption, Select};
        use rune_model::{Component, Components, ImageKind, RemoteImage};
        use std::collections::HashSet;
        let source = RemoteImage {
            url: "https://cdn.example.test/emojis/22.webp".into(),
            kind: ImageKind::Artwork,
        };
        let mut said = (*message(1, 1, 0)).clone();
        said.content.clear();
        said.components = Components {
            parts: vec![Component::Row(vec![Component::Select(Select {
                custom_id: Some("color".into()),
                placeholder: Some("Elige".into()),
                options: vec![MenuOption {
                    label: "Fiesta".into(),
                    value: "f".into(),
                    emoji: Some(":fiesta:".into()),
                    picture: Some(source.clone()),
                    ..MenuOption::default()
                }],
                chosen: vec![Choice {
                    label: "Fiesta".into(),
                    emoji: Some(":fiesta:".into()),
                    picture: Some(source.clone()),
                }],
                ..Select::default()
            })])],
            whole: true,
            app: Some(UserId(9)),
            ..Components::default()
        };
        let raster = limn::RasterImage::new(
            "0".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            Arc::from("AAAA"),
            "Emoji",
        )
        .unwrap();
        let loaded = std::collections::HashMap::from([(source, raster.clone())]);
        let playing = HashSet::from([raster.sha256().to_owned()]);
        let open = OpenMenu {
            custom_id: "color".into(),
            ..OpenMenu::default()
        };
        let shown = |playing: Option<&HashSet<String>>, app: Option<AppParts>| {
            let frame = self::laid(live_row(
                view(&said, ALL),
                Live {
                    attachment_images: Some(&loaded),
                    animated: playing,
                    app,
                    ..Live::default()
                },
                &Look::DARK,
            ));
            let surfaces = frame
                .commands
                .iter()
                .filter(|command| matches!(command, limn::DrawCommand::Surface { id, .. } if *id == animation_surface(raster.sha256())))
                .count();
            let pictures = frame
                .commands
                .iter()
                .filter(|command| matches!(command, limn::DrawCommand::RasterImage { .. }))
                .count();
            (surfaces, pictures, texts(&frame).join(" "))
        };
        let opened = Some(AppParts {
            menu: Some(&open),
            ..AppParts::default()
        });
        // Where it cannot be used, the choice it shows moves where its
        // picture was.
        assert_eq!(shown(Some(&playing), None).0, 1);
        // Open, the option has the picture before its label, not `:name:`,
        // and moves too; not played, it stands still.
        let (surfaces, pictures, painted) = shown(Some(&playing), opened);
        assert_eq!((surfaces, pictures), (1, 0), "{painted}");
        assert!(!painted.contains(":fiesta: Fiesta"), "{painted}");
        assert_eq!(shown(None, opened).1, 1);
    }

    #[test]
    fn a_voice_card_joins_only_where_the_account_may() {
        use rune_model::{Embed, EmbedKind};
        use std::collections::HashMap;
        let mut said = (*message(1, 1, 0)).clone();
        let room = ChannelId(77);
        said.embeds = vec![Embed {
            kind: EmbedKind::VoiceChannel,
            title: Some("Sala común".into()),
            channel: Some(room),
            ..Embed::default()
        }];
        let join = action(&said, "voice:77");
        let with = |known: &HashMap<ChannelId, Option<String>>| {
            self::laid(live_row(
                view(&said, ALL),
                Live {
                    voice_channels: Some(known),
                    ..Live::default()
                },
                &Look::DARK,
            ))
        };
        let allowed = with(&HashMap::from([(room, None)]));
        assert!(texts(&allowed).join(" ").contains("Sala común"));
        assert!(controls(&allowed).contains(&join));
        let denied = with(&HashMap::from([(
            room,
            Some("Sin permiso para entrar".into()),
        )]));
        assert!(!controls(&denied).contains(&join));
        assert!(texts(&denied).join(" ").contains("Sin permiso para entrar"));
        let unknown = with(&HashMap::new());
        assert!(!controls(&unknown).contains(&join));
        assert!(texts(&unknown).join(" ").contains("no conoce ese canal"));
        said.embeds[0].channel = None;
        let nowhere = self::laid(message_row(view(&said, ALL), &Look::DARK));
        assert!(texts(&nowhere).join(" ").contains("a qué canal de voz"));
    }

    #[test]
    fn an_apps_explicit_pictures_wait_blurred_behind_the_filter() {
        use rune_model::components::Media;
        use rune_model::{Component, Components, EmbedImage, ImageKind, RemoteImage};
        let mut said = (*message(1, 1, 0)).clone();
        said.content.clear();
        let source = |name: &str| RemoteImage {
            url: format!("https://media.discordapp.net/attachments/1/2/{name}.png"),
            kind: ImageKind::AttachmentPreview,
        };
        let item = |name: &str, spoiler, explicit| Media {
            image: Some(EmbedImage {
                source: source(name),
                dimensions: (64, 64),
            }),
            description: None,
            spoiler,
            explicit,
        };
        // An explicit picture, an explicit spoiler and a spoiler: 0, 1, 2.
        said.components = Components {
            parts: vec![Component::Gallery(vec![
                item("a", false, true),
                item("b", true, true),
                item("c", true, false),
            ])],
            whole: true,
            ..Components::default()
        };
        let blurred = std::collections::HashMap::from([
            (source("a").obscured(), emoji_picture('a')),
            (source("b").obscured(), emoji_picture('b')),
        ]);
        let reveal = |n: usize| format!("msg:{}:component:{n}:reveal", said.id);
        let with = |obscure_explicit, revealed: &[usize]| {
            self::laid(live_row(
                view(&said, ALL),
                Live {
                    attachment_images: Some(&blurred),
                    obscure_explicit,
                    revealed,
                    ..Live::default()
                },
                &Look::DARK,
            ))
        };
        let hidden = with(true, &[]);
        let painted = texts(&hidden).join(" ");
        assert_eq!(
            painted.matches("CONTENIDO EXPLÍCITO").count(),
            2,
            "{painted}"
        );
        assert_eq!(painted.matches("SPOILER").count(), 1, "{painted}");
        assert_eq!(pictures(&hidden).len(), 1, "only a's blurred picture");
        let ids = controls(&hidden);
        assert!((0..3).all(|n| ids.contains(&reveal(n))), "{ids:?}");
        // The filter off, the explicit picture is no spoiler.
        let free = with(false, &[]);
        assert!(!controls(&free).contains(&reveal(0)));
        assert!(!texts(&free).join(" ").contains("CONTENIDO EXPLÍCITO"));
        assert_eq!(texts(&free).join(" ").matches("SPOILER").count(), 2);
        // Revealed, it is no longer covered.
        let shown = with(true, &[PARTS_SPOILER]);
        assert!(!controls(&shown).contains(&reveal(0)));
        assert_eq!(
            texts(&shown)
                .join(" ")
                .matches("CONTENIDO EXPLÍCITO")
                .count(),
            1
        );
    }

    #[test]
    fn an_apps_parts_are_read_only_except_links_and_spoilers_wait() {
        use rune_model::components::{Button, Media};
        use rune_model::{ButtonStyle, Component, Components, EmbedImage, ImageKind, RemoteImage};
        let mut said = (*message(1, 1, 0)).clone();
        said.content.clear();
        let button = |label: &str, style, url: Option<&str>| {
            Component::Button(Button {
                label: Some(label.into()),
                style,
                url: url.map(str::to_owned),
                ..Button::default()
            })
        };
        let picture = Media {
            image: Some(EmbedImage {
                source: RemoteImage {
                    url: "https://media.discordapp.net/attachments/1/2/a.png".into(),
                    kind: ImageKind::AttachmentPreview,
                },
                dimensions: (64, 64),
            }),
            description: Some("Una foto".into()),
            spoiler: true,
            explicit: false,
        };
        said.attachments = vec![rune_model::Attachment {
            id: rune_model::AttachmentId(9),
            filename: "notas.txt".into(),
            size_bytes: 12,
            content_type: Some("text/plain".into()),
            dimensions: None,
            preview: None,
            download: None,
            spoiler: false,
            voice: None,
            explicit: false,
            placeholder: None,
        }];
        said.components = Components {
            parts: vec![Component::Container {
                accent: Some(0x5865F2),
                spoiler: false,
                parts: vec![
                    Component::Text("**Hola** equipo".into()),
                    Component::Gallery(vec![picture.clone(), picture]),
                    Component::File {
                        name: "notas.txt".into(),
                        spoiler: false,
                    },
                    Component::Row(vec![
                        button("Confirmar", ButtonStyle::Primary, None),
                        button("Agenda", ButtonStyle::Link, Some("https://example.test/a")),
                    ]),
                    Component::Row(vec![Component::Select(rune_model::components::Select {
                        placeholder: Some("Elige un horario".into()),
                        ..Default::default()
                    })]),
                    // What its app left picked shows in place of the placeholder.
                    Component::Row(vec![Component::Select(rune_model::components::Select {
                        placeholder: Some("Nunca visto".into()),
                        chosen: ["Rojo", "Azul"]
                            .map(|label| rune_model::components::Choice {
                                label: label.into(),
                                emoji: Some(format!(":{label}:")),
                                picture: None,
                            })
                            .into(),
                        ..Default::default()
                    })]),
                    Component::Unknown(21),
                ],
            }],
            whole: true,
            ..Components::default()
        };
        let laid = laid(message_row(view(&said, ALL), &Look::DARK));
        let painted = texts(&laid).join(" ");
        for part in [
            "Hola",
            "Confirmar",
            "Agenda",
            "Elige un horario",
            ":Rojo: Rojo",
            ":Azul: Azul",
            "notas.txt",
            "SPOILER",
            NO_INTERACTIONS,
            "tipo 21",
        ] {
            assert!(painted.contains(part), "{part} in {painted}");
        }
        assert_eq!(
            painted.matches("notas.txt").count(),
            1,
            "the file shows once"
        );
        assert!(!painted.contains("Nunca visto"));
        let external = laid
            .commands
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    limn::DrawCommand::Icon {
                        icon: Icon::ExternalLink,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(external, 1, "only the link button says it leaves RUNE");
        let ids = controls(&laid);
        assert!(ids.contains(&"markdown_link:embed:0:https://example.test/a".to_string()));
        assert!(
            !ids.iter()
                .any(|id| id.contains(":component:") && !id.ends_with(":reveal")),
            "an app's own buttons do nothing: {ids:?}"
        );
        // Each spoiler reveals on its own.
        let reveal = |n: usize| format!("msg:{}:component:{n}:reveal", said.id);
        assert!(ids.contains(&reveal(0)) && ids.contains(&reveal(1)));
        let with = |revealed: &[usize]| {
            self::laid(live_row(
                view(&said, ALL),
                Live {
                    revealed,
                    ..Live::default()
                },
                &Look::DARK,
            ))
        };
        let one = with(&[PARTS_SPOILER + 1]);
        assert_eq!(texts(&one).join(" ").matches("SPOILER").count(), 1);
        assert!(controls(&one).contains(&reveal(0)) && !controls(&one).contains(&reveal(1)));
        let both = with(&[PARTS_SPOILER, PARTS_SPOILER + 1]);
        assert!(!texts(&both).join(" ").contains("SPOILER"));
        assert!(!controls(&both).contains(&reveal(0)));
        assert_eq!(
            texts(&with(&[0, 1])).join(" ").matches("SPOILER").count(),
            2,
            "the text's spoilers are others"
        );

        // A forward made of parts shows them, its files only where they say.
        let mut copy = said.clone();
        copy.forwarded = Some(rune_model::Forwarded {
            timestamp: said.timestamp,
            attachments: std::mem::take(&mut copy.attachments),
            components: std::mem::take(&mut copy.components),
            ..rune_model::Forwarded::default()
        });
        let laid = self::laid(message_row(view(&copy, ALL), &Look::DARK));
        let painted = texts(&laid).join(" ");
        for part in ["Reenviado", "Hola", "Confirmar", "SPOILER"] {
            assert!(painted.contains(part), "{part} in {painted}");
        }
        assert_eq!(painted.matches("notas.txt").count(), 1, "{painted}");
        let first = first_spoiler(true, None);
        assert!(controls(&laid).contains(&reveal(first)));
    }

    #[test]
    fn a_service_notice_is_a_card_with_its_page_and_unknown_kinds_say_so() {
        use rune_model::{Embed, EmbedField, EmbedKind};
        let mut said = (*message(1, 1, 0)).clone();
        let page = "https://support.discord.com/hc/articles/1";
        said.embeds = vec![Embed {
            kind: EmbedKind::SystemNotice,
            title: Some("Mensaje importante acerca de tu cuenta".into()),
            description: Some("Detectamos actividad inusual.".into()),
            url: Some(page.into()),
            timestamp: Some(said.timestamp),
            ..Embed::default()
        }];
        let laid = laid(message_row(view(&said, ALL), &Look::DARK));
        let painted = texts(&laid).join(" ");
        for part in [
            "Mensaje importante acerca de tu cuenta",
            "Detectamos actividad inusual.",
            "Más información",
        ] {
            assert!(painted.contains(part), "{part} in {painted}");
        }
        assert!(controls(&laid).contains(&format!("markdown_link:embed:0:{page}")));

        said.embeds[0].url = None;
        let laid = self::laid(message_row(view(&said, ALL), &Look::DARK));
        assert!(!texts(&laid).join(" ").contains("Más información"));

        said.embeds = vec![Embed {
            kind: EmbedKind::Other,
            fields: vec![EmbedField {
                name: "rule_name".into(),
                value: "Spam".into(),
                inline: false,
            }],
            ..Embed::default()
        }];
        let painted = texts(&self::laid(message_row(view(&said, ALL), &Look::DARK))).join(" ");
        assert!(painted.contains("rule_name") && painted.contains("Spam"));
        assert!(
            painted.contains("RUNE todavía no muestra todo"),
            "{painted}"
        );
    }

    #[test]
    fn groups_split_on_another_author_and_every_seven_minutes_from_their_start() {
        let messages = vec![
            message(1, 1, 0),
            message(2, 1, 3),
            message(3, 1, 6),
            message(4, 1, 8), // eight minutes after the group's first: a new group
            message(5, 2, 9),
            message(6, 1, 10),
        ];
        let starts: Vec<bool> = (0..messages.len())
            .map(|index| group_starts_at(&messages, index, 200))
            .collect();
        assert_eq!(starts, vec![true, false, false, true, true, true]);
    }

    #[test]
    fn a_long_run_costs_its_length_not_more() {
        let messages: Vec<Arc<Message>> = (0..5_000)
            .map(|index| message(index, 1, index as i64 / 10))
            .collect();
        let start = std::time::Instant::now();
        for index in 4_950..5_000 {
            group_starts_at(&messages, index, 200);
        }
        assert!(
            start.elapsed() < std::time::Duration::from_millis(500),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn the_text_starts_past_the_avatar_column_and_the_name_is_sixteen_pixels() {
        let mut said = (*message(1, 1, 0)).clone();
        said.content = "hola".into();
        let laid = laid(message_row(view(&said, ALL), &Look::DARK));
        let text = laid
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::Text {
                    text,
                    position,
                    font_size,
                    ..
                } if text == "hola" => Some((position.x, *font_size)),
                _ => None,
            })
            .expect("the text is painted");
        assert!((text.0 - GUTTER).abs() < 1.0, "{text:?}");
        assert!((text.1 - TEXT_SIZE).abs() < 0.01, "{text:?}");
        let name = laid
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::Text {
                    text, font_size, ..
                } if text == "Ada" => Some(*font_size),
                _ => None,
            })
            .unwrap();
        assert_eq!(name, 16.0);
    }

    #[test]
    fn the_bar_offers_quick_reactions_edit_on_own_and_reply_on_others_and_more() {
        let said = message(1, 1, 0);
        let own = controls(&laid(message_row(view(&said, ALL), &Look::DARK)));
        for id in [
            "msg:1:quick=👍",
            "msg:1:react",
            "msg:1:edit",
            "msg:1:actions:more",
        ] {
            assert!(own.contains(&id.to_owned()), "{id} in {own:?}");
        }
        assert!(!own.contains(&"msg:1:reply".to_owned()));
        let others = Abilities { edit: false, ..ALL };
        let others = controls(&laid(message_row(view(&said, others), &Look::DARK)));
        assert!(
            others.contains(&"msg:1:reply".to_owned())
                && !others.contains(&"msg:1:edit".to_owned())
        );
    }

    #[test]
    fn with_shift_held_the_bar_trades_its_quick_reactions_for_direct_actions() {
        let mut said = (*message(1, 1, 0)).clone();
        said.content = "hola".into();
        let held = Live {
            shift: true,
            ..Live::default()
        };
        let mut shown = view(&said, ALL);
        shown.style.copy_id = true;
        let bar = controls(&laid(live_row(shown, held, &Look::DARK)));
        for id in [
            "msg:1:react",
            "msg:1:edit",
            "msg:1:pin",
            "msg:1:unread",
            "msg:1:copy",
            "msg:1:copy-id",
            "msg:1:delete",
            "msg:1:actions:more",
        ] {
            assert!(bar.contains(&id.to_owned()), "{id} in {bar:?}");
        }
        assert!(
            !bar.iter().any(|id| id.starts_with("msg:1:quick=")),
            "{bar:?}"
        );
        // In Serein's order: pin, unread, link, text, ID, then delete.
        let place = |id: &str| bar.iter().position(|shown| shown == id).unwrap();
        assert!(place("msg:1:pin") < place("msg:1:unread"));
        assert!(place("msg:1:copy") < place("msg:1:copy-id"));
        assert!(place("msg:1:copy-id") < place("msg:1:delete"));
        // Only what the account may do: no pin nor delete without them.
        let limited = Abilities {
            pin: false,
            delete: false,
            ..ALL
        };
        let bar = controls(&laid(live_row(view(&said, limited), held, &Look::DARK)));
        assert!(
            !bar.contains(&"msg:1:pin".to_owned()) && !bar.contains(&"msg:1:delete".to_owned())
        );
        assert!(
            !bar.contains(&"msg:1:copy-id".to_owned()),
            "outside developer mode"
        );
        // Without Shift, the quick reactions and none of them.
        let bar = controls(&laid(live_row(
            view(&said, ALL),
            Live::default(),
            &Look::DARK,
        )));
        assert!(
            bar.contains(&"msg:1:quick=👍".to_owned()) && !bar.contains(&"msg:1:pin".to_owned())
        );
    }

    #[test]
    fn the_more_menu_has_quick_reactions_the_choices_and_the_id_in_developer_mode() {
        let mut said = (*message(1, 1, 0)).clone();
        said.content = "hola".into();
        let open = Live {
            more: true,
            ..Live::default()
        };
        let mut shown = view(&said, ALL);
        shown.style.copy_id = true;
        let laid = laid(live_row(shown, open, &Look::DARK));
        let painted = texts(&laid);
        for label in [
            "Agregar reacción",
            "Editar mensaje",
            "Responder",
            "Crear hilo",
            "Copiar texto",
            "Fijar mensaje",
            "Marcar como no leído",
            "Eliminar mensaje",
            "Copiar ID del mensaje",
        ] {
            assert!(
                painted.iter().any(|text| text == label),
                "{label} in {painted:?}"
            );
        }
        let ids = controls(&laid);
        assert_eq!(
            ids.iter()
                .filter(|id| id.starts_with("msg:1:quick="))
                .count(),
            3
        );
        assert!(
            ids.iter()
                .filter(|id| id.starts_with("msg:1:menu-quick="))
                .count()
                >= 4
        );
        let plain: usize = choices(&said, Abilities::default(), false)
            .iter()
            .map(Vec::len)
            .sum();
        assert_eq!(plain, 2, "copy text and mark unread");
    }

    #[test]
    fn a_heading_in_a_message_is_drawn_as_one_at_the_chosen_size() {
        let size_of = |content: &str, text: &str, text_size: f32| {
            let mut said = (*message(1, 1, 0)).clone();
            said.content = content.into();
            let mut shown = view(&said, ALL);
            shown.style.text_size = text_size;
            let laid = laid(message_row(shown, &Look::DARK));
            laid.commands
                .iter()
                .find_map(|command| match command {
                    limn::DrawCommand::Text {
                        text: drawn,
                        font_size,
                        ..
                    } if drawn == text => Some(*font_size),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{text} is drawn"))
        };
        let body = size_of("# Título\nlo demás", "lo demás", MARKDOWN_BODY);
        let heading = size_of("# Título\nlo demás", "Título", MARKDOWN_BODY);
        assert!(heading > body, "{heading} over {body}");
        assert!(size_of("## Sub", "Sub", MARKDOWN_BODY) < heading);
        // A larger text keeps the proportion, and the mark is not written.
        assert!((size_of("# Título", "Título", 2.0 * MARKDOWN_BODY) - 2.0 * heading).abs() < 0.01);
        let mut said = (*message(1, 1, 0)).clone();
        said.content = "# Título".into();
        let painted = texts(&laid(message_row(view(&said, ALL), &Look::DARK)));
        assert!(
            !painted.iter().any(|text| text.contains('#')),
            "{painted:?}"
        );
    }

    #[test]
    fn a_secondary_click_opens_the_whole_menu_where_it_was() {
        let mut said = (*message(1, 1, 0)).clone();
        said.content = "hola".into();
        let at = limn::Point::new(300.0, 40.0);
        let open = Live {
            context: Some(at),
            ..Live::default()
        };
        let shown = laid(live_row(view(&said, ALL), open, &Look::DARK));
        let ids = controls(&shown);
        assert!(
            ids.iter()
                .filter(|id| id.starts_with("msg:1:menu-quick="))
                .count()
                >= 4,
            "its quick reactions: {ids:?}"
        );
        let delete = shown
            .regions
            .iter()
            .find(|region| region.id == "msg:1:delete")
            .expect("deleting is in it")
            .rect;
        assert!(delete.x >= at.x && delete.y > at.y, "{delete:?}");
        // Closed, the row only asks for it.
        let closed = laid(message_row(view(&said, ALL), &Look::DARK));
        assert!(!controls(&closed).contains(&"msg:1:delete".to_owned()));
    }

    #[test]
    fn a_message_not_sent_yet_has_no_quick_reactions_nor_an_empty_menu() {
        let at = limn::Point::new(300.0, 40.0);
        let open = || Live {
            context: Some(at),
            ..Live::default()
        };
        let mut pending = (*message(1, 1, 0)).clone();
        pending.content = "hola".into();
        pending.delivery = rune_model::Delivery::Sending;
        let row = limn::Point::new(100.0, 10.0);
        let menu = |frame: &limn::Frame| {
            frame
                .regions
                .iter()
                .any(|region| region.id == "popover_surface:msg:1:context:dismiss")
        };
        let shown = laid(live_row(view(&pending, ALL), open(), &Look::DARK));
        assert!(menu(&shown));
        let ids = controls(&shown);
        assert!(
            !ids.iter().any(|id| id.starts_with("msg:1:menu-quick=")),
            "the provider does not know it yet: {ids:?}"
        );
        assert!(ids.contains(&"msg:1:copy".to_owned()), "{ids:?}");
        let closed = laid(live_row(view(&pending, ALL), Live::default(), &Look::DARK));
        assert_eq!(closed.context_menu_at(row), Some("msg:1:menu"));
        // Failed and without text, nothing is offered: no menu is asked for.
        pending.content.clear();
        pending.delivery = rune_model::Delivery::Failed {
            reason: "sin red".into(),
        };
        let closed = laid(live_row(view(&pending, ALL), Live::default(), &Look::DARK));
        assert_eq!(closed.context_menu_at(row), None);
        assert!(!menu(&laid(live_row(
            view(&pending, ALL),
            open(),
            &Look::DARK
        ))));
    }

    #[test]
    fn editing_puts_an_editor_and_how_to_leave_it_in_place_of_the_text() {
        let mut said = (*message(1, 1, 0)).clone();
        said.content = "antes".into();
        let laid = laid(live_row(
            view(&said, ALL),
            Live {
                editing: Some("después"),
                ..Live::default()
            },
            &Look::DARK,
        ));
        let ids = controls(&laid);
        assert!(ids.contains(&editor_id(MessageId(1))), "{ids:?}");
        assert!(ids.contains(&"msg:1:edit-cancel".to_owned()));
        assert!(ids.contains(&"msg:1:edit-save".to_owned()));
        assert!(!texts(&laid).iter().any(|text| text == "antes"));
    }

    #[test]
    fn the_hint_under_the_editor_wraps_inside_a_narrow_column() {
        let said = message(1, 1, 0);
        let narrow = laid(live_row(
            MessageView {
                width: 380.0,
                ..view(&said, ALL)
            },
            Live {
                editing: Some("algo"),
                ..Live::default()
            },
            &Look::DARK,
        ));
        let rect = |id: String| {
            narrow
                .regions
                .iter()
                .find(|region| region.id == id)
                .unwrap_or_else(|| panic!("{id}"))
                .rect
        };
        let field = rect(editor_id(MessageId(1)));
        let save = rect("msg:1:edit-save".into());
        assert!(
            save.x + save.width <= field.x + field.width + 0.5,
            "{save:?} past {field:?}"
        );
    }

    // AutoMod (automod).
    #[test]
    fn a_long_reason_wraps_whole_above_what_can_be_done() {
        let mut said = (*message(1, 1, 0)).clone();
        said.delivery = rune_model::Delivery::Failed {
            reason: "la moderación automática del servidor lo bloqueó: «No compartas invitaciones a otros servidores, ni enlaces acortados.»".into(),
        };
        let laid = laid(message_row(
            MessageView {
                width: 380.0,
                ..view(&said, ALL)
            },
            &Look::DARK,
        ));
        let painted = texts(&laid).join(" ");
        assert!(
            painted.contains("acortados.».") && painted.contains("No se envió"),
            "{painted}"
        );
        let retry = laid
            .regions
            .iter()
            .find(|region| region.id.ends_with(":retry"))
            .expect("Reintentar");
        assert!(retry.rect.x + retry.rect.width <= 700.0);
    }

    #[test]
    fn a_continuation_is_as_tall_as_its_line_and_the_bar_floats_over_it() {
        let said = |id: u64, text: &str| {
            let mut said = (*message(id, 1, 0)).clone();
            said.content = text.into();
            said
        };
        let (first, second) = (said(1, "uno"), said(2, "dos"));
        let row = |message| {
            message_row(
                MessageView {
                    continued: true,
                    ..view(message, ALL)
                },
                &Look::DARK,
            )
        };
        let laid = laid(Element::column(vec![row(&first), row(&second)]));
        let top = |wanted: &str| {
            laid.commands
                .iter()
                .find_map(|command| match command {
                    limn::DrawCommand::Text { text, position, .. } if text == wanted => {
                        Some(position.y)
                    }
                    _ => None,
                })
                .unwrap()
        };
        let step = top("dos") - top("uno");
        assert!(step < 30.0, "the bar (34 tall) set it: {step}");
        assert!(controls(&laid).contains(&"msg:2:actions:more".to_owned()));
    }

    #[test]
    fn reactions_say_who_reacted_and_offer_to_add_one() {
        let mut said = (*message(1, 1, 0)).clone();
        said.reactions = vec![
            Reaction {
                emoji: "❤️".into(),
                count: 3,
                me: true,
                ..Default::default()
            },
            Reaction {
                emoji: "🦀".into(),
                count: 1,
                me: false,
                ..Default::default()
            },
        ];
        let ids = controls(&laid(message_row(view(&said, ALL), &Look::DARK)));
        assert!(
            ids.contains(&"react:1:❤️".to_owned()) && ids.contains(&"msg:1:react-more".to_owned())
        );
        assert_eq!(
            reacted_with(&said.reactions[0]),
            "Tú y 2 personas más reaccionaron con :heart:"
        );
        assert_eq!(
            reacted_with(&said.reactions[1]),
            "1 persona reaccionó con :crab:"
        );
    }

    #[test]
    fn super_reactions_show_apart_and_are_never_sent() {
        let mut said = (*message(1, 1, 0)).clone();
        said.reactions = vec![
            Reaction {
                emoji: "🔥".into(),
                burst: 2,
                me_burst: true,
                burst_colors: vec![rune_model::Rgb(0xff8800)],
                ..Default::default()
            },
            Reaction {
                emoji: "❤️".into(),
                count: 1,
                burst: 1,
                ..Default::default()
            },
        ];
        let painted = |message: &Message| -> Vec<String> {
            laid(message_row(view(message, ALL), &Look::DARK))
                .commands
                .iter()
                .filter_map(|command| match command {
                    limn::DrawCommand::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect()
        };
        let ids = controls(&laid(message_row(view(&said, ALL), &Look::DARK)));
        assert!(
            !ids.iter().any(|id| id.starts_with("react:1:🔥")),
            "only super reactions: nothing to click"
        );
        assert!(ids.contains(&"react:1:❤️".to_owned()));
        // ❤️ shows twice, its super reactions apart; 🔥 only as super ones.
        let mut plain = said.clone();
        plain.reactions.remove(0);
        plain.reactions[0].burst = 0;
        let hearts = |texts: Vec<String>| texts.iter().filter(|text| *text == "❤️").count();
        assert_eq!(hearts(painted(&said)), hearts(painted(&plain)) + 1);
        assert!(
            painted(&said).contains(&"🔥".to_owned()) && painted(&said).contains(&"2".to_owned())
        );
        assert_eq!(
            super_reacted_with(&said.reactions[0]),
            "Tú y 1 persona más hicieron súper reacciones con :fire:"
        );
        assert_eq!(
            super_reacted_with(&said.reactions[1]),
            "1 persona hizo una súper reacción con :heart:"
        );
    }

    #[test]
    fn the_whole_date_names_the_weekday() {
        let date = full_date(Timestamp(1_790_000_000_000));
        assert!(date.contains(" de ") && date.contains(", "), "{date}");
    }
}
