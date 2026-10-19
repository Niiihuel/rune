//! Simulated rich content near the end of #general: a closed poll and the
//! record of its result, a voice channel's card, RuneBot's message made only
//! of its parts and its message of menus (`mock::interactions`), an audio
//! file and a video; and (P4) an open poll of the account's own and a link
//! to another conversation with two moments in it, with super reactions; and
//! a line with server emoji in it and an answer of emoji alone (2026-10-17),
//! and a line in Discord's marks (2026-10-18); and invitation links of every
//! kind amid text (`mock::invitations`); and (contenido-de-mensajes) two
//! pictures, one waiting behind its ThumbHash and one marked explicit, a
//! forward that names people, a channel and an emoji, and in the
//! conversation with Ada the records of a call joined and of one missed.
//! They take fixed places counted from the end of the history and their own
//! random stream, so the rest of the history stays as it was.

use rune_i18n::t;
use rune_model::{
    Attachment, AttachmentId, ChannelId, ChannelKind, Embed, EmbedField, EmbedKind, Message,
    MessageId, MessageKind, Poll, PollAnswer, Reaction, Rgb,
};

use super::{message_id, mix, ChannelSim, World};

/// Mixed into this content's stream, apart from every other one of the world.
const SALT: u64 = 0x5348_4f57;

impl World {
    /// Turn `message`, at `index` of #general, into a showcase when its place
    /// is one of theirs.
    pub(super) fn showcase(
        &self,
        channel: ChannelId,
        sim: &ChannelSim,
        index: u64,
        message: &mut Message,
    ) {
        let plain = |message: &mut Message| {
            message.content.clear();
            message.mentions.clear();
            message.reply_to = None;
            message.attachments.clear();
            message.embeds.clear();
        };
        match sim.generated.checked_sub(index) {
            Some(10) => {
                plain(message);
                message.poll = Some(self.closed_poll(channel, index));
            }
            Some(8) => {
                let poll_index = index - 2;
                let poll = self.closed_poll(channel, poll_index);
                plain(message);
                message.kind = MessageKind::PollResult;
                message.author = self.generate(channel, sim, poll_index).author;
                message.reply_to = Some(message_id(channel, poll_index));
                message.reactions.clear();
                message.embeds = vec![result_card(&poll)];
            }
            Some(3) => {
                plain(message);
                message.content = format!("Estoy en la sala, ¿se suman? {}", t("(simulado)"));
                let server = self.channel(channel).and_then(|found| found.server);
                let voice = self
                    .channels
                    .iter()
                    .find(|found| found.server == server && found.kind == ChannelKind::Voice);
                message.embeds = vec![Embed {
                    kind: EmbedKind::VoiceChannel,
                    author: Some("Canal de voz".into()),
                    title: voice.map(|voice| voice.name.clone()),
                    description: Some(format!("Hay gente charlando ahora {}.", t("(simulado)"))),
                    channel: voice.map(|voice| voice.id),
                    ..Embed::default()
                }];
            }
            Some(2) => {
                plain(message);
                message.reactions.clear();
                self.app_message(message);
            }
            Some(9) => {
                plain(message);
                message.reactions.clear();
                self.menus_message(channel, message);
            }
            // An audio file and a video without a file to fetch: the host
            // plays samples generated for RUNE in their place.
            Some(7) => {
                plain(message);
                message.content = format!(
                    "La grabación del ensayo {}",
                    t("(simulada): un tono generado.")
                );
                message.attachments =
                    vec![played(message.id, "ensayo.mp3", 12_347, "audio/mpeg", None)];
            }
            Some(5) => {
                plain(message);
                message.content = format!(
                    "Así quedó la demo {}",
                    t("(simulada): un patrón de prueba.")
                );
                // The same, as WebM (VP9 and Opus).
                let mut webm = played(
                    message.id,
                    "demo.webm",
                    65_295,
                    "video/webm",
                    Some((320, 180)),
                );
                webm.id = AttachmentId(message.id.0 + 1);
                message.attachments = vec![
                    played(
                        message.id,
                        "demo.mp4",
                        61_134,
                        "video/mp4",
                        Some((320, 180)),
                    ),
                    webm,
                ];
            }
            // Message actions (P4): an open poll of the account's own, and
            // a link to another conversation with two moments in it. Its
            // own place: 4 and 6 are the generated poll's and its neighbor's.
            Some(11) => {
                plain(message);
                message.author = super::ME;
                message.reactions.clear();
                message.poll = Some(self.open_poll(channel, index));
            }
            // Server emoji in a line, and an answer that is only emoji.
            Some(13) => {
                plain(message);
                let [chispa, engranaje] = super::identity::server_emoji();
                message.content = format!(
                    "Quedó andando {} y sin trabas {} {}",
                    chispa.token,
                    engranaje.token,
                    t("(emoji simulados)")
                );
                message.text_references = super::identity::emoji_references(&message.content);
            }
            // Discord's marks (2026-10-18). Its own place: 14 and 15 are
            // the invitations'.
            Some(16) => {
                plain(message);
                message.content = format!(
                    "Quedó __subrayado__, ~~tachado~~, ***en negrita cursiva*** y \
                     **__las dos__** {}\n-# Y esto es subtexto, como una nota al pie.",
                    t("(simulado)")
                );
            }
            Some(12) => {
                plain(message);
                let [chispa, engranaje] = super::identity::server_emoji();
                message.content = format!("{} {}", chispa.token, engranaje.token);
                message.text_references = super::identity::emoji_references(&message.content);
                message.reply_to = Some(message_id(channel, index - 1));
            }
            Some(1) => {
                plain(message);
                let server = self.channel(channel).and_then(|found| found.server);
                let news = self
                    .channels
                    .iter()
                    .find(|found| found.server == server && found.name == "anuncios");
                let link = match (server, news) {
                    (Some(server), Some(news)) => format!(
                        "{}/{server}/{}/{}",
                        super::message_actions::LINKS,
                        news.id,
                        message_id(news.id, 3)
                    ),
                    _ => String::new(),
                };
                let at = self.history_end.0 / 1000 + 2 * 24 * 3600;
                message.content = format!(
                    "El anuncio de la juntada {}: {link} · empieza el <t:{at}:F> (<t:{at}:R>)",
                    t("(simulado)")
                );
                message.text_references = super::message_actions::references(&message.content);
                // Super reactions beside normal ones, and one alone (2026-10-18).
                message.reactions = vec![
                    Reaction {
                        emoji: "🎉".into(),
                        count: 4,
                        burst: 2,
                        burst_colors: vec![Rgb(0xf9a62b), Rgb(0xe8433a)],
                        ..Default::default()
                    },
                    Reaction {
                        emoji: "🚀".into(),
                        burst: 1,
                        burst_colors: vec![Rgb(0x5865f2)],
                        ..Default::default()
                    },
                ];
            }
            // Invitations amid text: a server, a guest's voice channel and
            // a group in one message, and a friendship in the next.
            Some(15) => {
                plain(message);
                let link = super::invitations::LINKS;
                message.content = format!(
                    "Para el sábado: el club {link}club y la radio {link}radio; \
                     y el grupo de los jueves, {link}jueves {}",
                    t("(simulado)")
                );
                message.text_references = super::invitations::references(&message.content);
            }
            // Scheduled events (eventos-programados): an invitation to the
            // club that names its photo walk, after what the line said (the
            // simulation's threads may quote it).
            Some(23) => {
                let link = format!(
                    "¿Vamos a la salida? {}club?event={} {}",
                    super::invitations::LINKS,
                    super::events::CLUB_OUTING,
                    t("(simulado)")
                );
                message.content = format!("{}\n{link}", message.content)
                    .trim_start()
                    .to_owned();
                message
                    .text_references
                    .extend(super::invitations::references(&link));
            }
            Some(14) => {
                plain(message);
                message.content = format!(
                    "¿Nos agregamos? {}amistad {}",
                    super::invitations::LINKS,
                    t("(simulado)")
                );
                message.text_references = super::invitations::references(&message.content);
            }
            // Message content (contenido-de-mensajes): two pictures, one
            // that never arrives (its ThumbHash stands in) and one the
            // service marked explicit; and a forward naming someone, a
            // channel and a server emoji.
            Some(17) => {
                plain(message);
                message.content = format!(
                    "Fotos de la costa {}",
                    t("(simuladas): una no llega nunca y la otra está marcada como explícita.")
                );
                message.attachments = vec![
                    pictured(
                        message.id,
                        0,
                        "costa.jpg",
                        (400, 260),
                        "cargando/costa",
                        false,
                    ),
                    pictured(message.id, 1, "playa.jpg", (300, 300), "fotos/playa", true),
                ];
            }
            Some(18) => {
                plain(message);
                let server = self.channel(channel).and_then(|found| found.server);
                let news = self
                    .channels
                    .iter()
                    .find(|found| found.server == server && found.name == "anuncios");
                let [chispa, _] = super::identity::server_emoji();
                let mut content = format!(
                    "{} <@10> {}",
                    chispa.token,
                    t("dejó todo listo (copia simulada)")
                );
                let mut references = super::identity::emoji_references(&content);
                references.push(rune_model::TextReference {
                    source: "<@10>".into(),
                    target: rune_model::TextTarget::User(rune_model::UserId(10)),
                });
                if let Some(news) = news {
                    let mention = format!("<#{}>", news.id);
                    content.push_str(&format!(": {mention}"));
                    references.push(rune_model::TextReference {
                        source: mention,
                        target: rune_model::TextTarget::Channel(news.id),
                    });
                }
                message.forwarded = Some(rune_model::Forwarded {
                    content,
                    timestamp: message.timestamp.plus_minutes(-90),
                    text_references: references,
                    ..rune_model::Forwarded::default()
                });
            }
            _ => {}
        }
    }

    /// The records of two calls near the end of the conversation with Ada
    /// (contenido-de-mensajes): one the account joined, twelve minutes
    /// long, and one it missed, of a minute.
    pub(super) fn calls(
        &self,
        channel: ChannelId,
        sim: &super::ChannelSim,
        index: u64,
        message: &mut Message,
    ) {
        let ada = rune_model::UserId(10);
        let direct = self
            .channel(channel)
            .is_some_and(|found| found.kind == ChannelKind::Direct && found.recipients == [ada]);
        let minutes = match sim.generated.checked_sub(index) {
            _ if !direct => return,
            Some(3) => 12,
            Some(6) => 1,
            _ => return,
        };
        let joined = minutes > 1;
        message.author = ada;
        message.kind = MessageKind::Call;
        message.content.clear();
        message.mentions.clear();
        message.reply_to = None;
        message.attachments.clear();
        message.embeds.clear();
        message.reactions.clear();
        message.poll = None;
        message.call = Some(rune_model::MessageCall {
            participants: if joined {
                vec![ada, super::ME]
            } else {
                vec![ada]
            },
            ended: Some(message.timestamp.plus_minutes(minutes)),
        });
    }

    /// A poll of the account's own, still open, with some votes.
    fn open_poll(&self, channel: ChannelId, index: u64) -> Poll {
        let mut votes = mix(&[self.seed, channel.0, index, SALT, 9]);
        let answer = |id: u32, text: &str, emoji: Option<&str>, votes: u32| PollAnswer {
            id,
            text: text.into(),
            emoji: emoji.map(str::to_owned),
            votes,
            me: false,
        };
        Poll {
            question: format!("¿Qué vemos el jueves? {}", t("(simulada)")),
            answers: vec![
                answer(1, "Async y Tokio", Some("⚡"), 3 + votes.below(4) as u32),
                answer(2, "Macros", Some("🧩"), 1 + votes.below(4) as u32),
                answer(3, "Interfaces con limn", Some("🎨"), votes.below(3) as u32),
            ],
            expiry: Some(self.history_end.plus_minutes(2 * 24 * 60)),
            multiselect: true,
            results_known: true,
            finalized: false,
        }
    }

    /// A poll that already closed, with its counts.
    fn closed_poll(&self, channel: ChannelId, index: u64) -> Poll {
        let mut votes = mix(&[self.seed, channel.0, index, SALT]);
        let answer = |id: u32, text: &str, emoji: Option<&str>, votes: u32| PollAnswer {
            id,
            text: text.into(),
            emoji: emoji.map(str::to_owned),
            votes,
            me: false,
        };
        Poll {
            question: format!("¿Dónde hacemos el encuentro? {}", t("(simulada)")),
            answers: vec![
                answer(
                    1,
                    "En la sala de voz",
                    Some("🎧"),
                    4 + votes.below(4) as u32,
                ),
                answer(2, "En persona", Some("☕"), votes.below(4) as u32),
            ],
            expiry: Some(self.history_end.plus_minutes(-60)),
            multiselect: false,
            results_known: true,
            finalized: true,
        }
    }
}

/// A file the simulation plays, by its kind; it has nothing to fetch.
fn played(
    message: MessageId,
    filename: &str,
    size_bytes: u64,
    content_type: &str,
    dimensions: Option<(u32, u32)>,
) -> Attachment {
    Attachment {
        id: AttachmentId(message.0),
        filename: filename.into(),
        size_bytes,
        content_type: Some(content_type.into()),
        dimensions,
        preview: None,
        download: None,
        spoiler: false,
        voice: None,
        placeholder: None,
        explicit: false,
    }
}

/// A ThumbHash of a drawn coast (sky, sun, sea and sand), as the format's
/// reference implementation encodes it (contenido-de-mensajes).
pub(super) const THUMBHASH: &str = "ascFVYqGd3iIiIePhzd2goZwbAjo";

/// A picture the simulation names on `simulacion.invalid/{path}`, with a
/// ThumbHash to stand in for it, and marked `explicit` or not; its id is
/// its message's plus `nth` (the message after it has no files).
fn pictured(
    message: MessageId,
    nth: u64,
    filename: &str,
    dimensions: (u32, u32),
    path: &str,
    explicit: bool,
) -> Attachment {
    Attachment {
        id: AttachmentId(message.0 + nth),
        filename: filename.into(),
        size_bytes: 210_000,
        content_type: Some("image/jpeg".into()),
        dimensions: Some(dimensions),
        preview: Some(rune_model::RemoteImage {
            url: format!("https://simulacion.invalid/{path}.jpg"),
            kind: rune_model::ImageKind::AttachmentPreview,
        }),
        download: None,
        spoiler: false,
        voice: None,
        placeholder: Some(rune_model::RemoteImage {
            url: THUMBHASH.into(),
            kind: rune_model::ImageKind::ThumbHash,
        }),
        explicit,
    }
}

/// The card of a result, with the fields the service writes for it.
fn result_card(poll: &Poll) -> Embed {
    let winner = poll
        .answers
        .iter()
        .max_by_key(|answer| answer.votes)
        .expect("a poll has answers");
    let field = |name: &str, value: String| EmbedField {
        name: name.into(),
        value,
        inline: false,
    };
    let mut fields = vec![
        field("poll_question_text", poll.question.clone()),
        field("victor_answer_votes", winner.votes.to_string()),
        field("total_votes", poll.votes().to_string()),
        field("victor_answer_id", winner.id.to_string()),
        field("victor_answer_text", winner.text.clone()),
    ];
    if let Some(emoji) = &winner.emoji {
        fields.push(field("victor_answer_emoji_name", emoji.clone()));
    }
    Embed {
        kind: EmbedKind::PollResult,
        fields,
        ..Embed::default()
    }
}
