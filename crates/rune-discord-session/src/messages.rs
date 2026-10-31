//! Session-local validation and a bounded cache for partial Gateway updates.

use rune_i18n::es;
use std::collections::{HashMap, VecDeque};

use rune_model::{ChannelId, Message, MessageId, Permissions, UserId};
use rune_provider::{Command, Event, Snapshot};
use serde_json::Value;

use crate::{tallies::Tallies, wire, LoginError};

const CACHE_LIMIT: usize = 512;

#[derive(Default)]
pub(crate) struct Messages {
    entries: HashMap<(ChannelId, MessageId), Message>,
    order: VecDeque<(ChannelId, MessageId)>,
    tallies: Tallies,
    /// What stickers the account may send beyond the channel's server (P5).
    reach: crate::identity::Reach,
}

impl Messages {
    pub fn observe(&mut self, event: &Event) {
        self.reach.observe(event);
        match event {
            Event::MessageCreated { message, .. } | Event::MessageUpdated(message) => {
                self.keep(message)
            }
            Event::MessagePatched(patch) => {
                if let Some(message) = self.entries.get_mut(&(patch.channel, patch.id)) {
                    patch.apply(message);
                }
            }
            Event::HistoryLoaded { messages, .. }
            | Event::ContextLoaded { messages, .. }
            | Event::PinsLoaded { messages, .. } => {
                for message in messages {
                    self.keep(message);
                }
            }
            Event::MessageDeleted { channel, message } => {
                self.entries.remove(&(*channel, *message));
                self.order.retain(|key| *key != (*channel, *message));
            }
            Event::ChannelRemoved { channel } => {
                self.entries.retain(|(id, _), _| id != channel);
                self.order.retain(|(id, _)| id != channel);
            }
            // Kept current so a toggle knows whether the account has reacted.
            Event::ReactionChanged {
                channel,
                message,
                emoji,
                me,
                added,
            } => {
                if let Some(held) = self.entries.get_mut(&(*channel, *message)) {
                    held.count_reaction(emoji, *me, *added);
                }
            }
            Event::ReactionsCleared {
                channel,
                message,
                emoji,
            } => {
                if let Some(held) = self.entries.get_mut(&(*channel, *message)) {
                    held.clear_reactions(emoji.as_deref());
                }
            }
            Event::BurstReactionChanged {
                channel,
                message,
                emoji,
                me,
                added,
                colors,
            } => {
                if let Some(held) = self.entries.get_mut(&(*channel, *message)) {
                    held.count_burst(emoji, *me, *added, colors);
                }
            }
            _ => {}
        }
    }

    fn keep(&mut self, message: &Message) {
        let key = (message.channel, message.id);
        if self
            .entries
            .get(&key)
            .is_some_and(|held| held.author != message.author || held.edited > message.edited)
        {
            return;
        }
        self.order.retain(|old| *old != key);
        self.order.push_back(key);
        self.entries.insert(key, message.clone());
        while self.order.len() > CACHE_LIMIT {
            if let Some(old) = self.order.pop_front() {
                self.entries.remove(&old);
            }
        }
    }

    /// A message this session loaded.
    pub fn get(&self, channel: ChannelId, message: MessageId) -> Option<&Message> {
        self.entries.get(&(channel, message))
    }

    pub fn allowed(&self, snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
        let (channel, permission) = match command {
            // Commands and interactions (P3).
            command if crate::interactions::is_app_command(command) => {
                return crate::interactions::allowed(snapshot, command)
            }
            // A profile is read in a server the account is still in, or outside any.
            Command::LoadProfile { server, .. } => {
                return match server {
                    Some(server) if !snapshot.servers.iter().any(|known| known.id == *server) => {
                        Err(LoginError::new(es("Ya no estás en ese servidor.")))
                    }
                    _ => Ok(()),
                };
            }
            Command::LoadHistory { channel, .. } => (*channel, Permissions::READ_HISTORY),
            Command::LoadContext {
                channel, anchor, ..
            } => {
                if anchor.0 == 0 {
                    return Err(wire::invalid());
                }
                (*channel, Permissions::READ_HISTORY)
            }
            Command::SendMessage {
                channel, content, ..
            } => {
                validate_content(content)?;
                (*channel, Permissions::SEND)
            }
            Command::SendSticker {
                channel, sticker, ..
            } => {
                let server = snapshot
                    .channels
                    .iter()
                    .find(|found| found.id == *channel)
                    .and_then(|found| found.server)
                    .and_then(|id| snapshot.servers.iter().find(|server| server.id == id));
                let available = |server: &rune_model::Server| {
                    server
                        .stickers
                        .iter()
                        .any(|known| known.id == *sticker && known.available)
                };
                // A standard pack's anywhere; another server's only as the
                // plan allows (P5).
                let elsewhere = self.reach.pack_stickers.contains(sticker)
                    || (self.reach.stickers_anywhere && snapshot.servers.iter().any(available));
                if !server.is_some_and(available) && !elsewhere {
                    return Err(LoginError::new(es(
                        "Este sticker no está disponible en el servidor.",
                    )));
                }
                (*channel, Permissions::SEND)
            }
            Command::EditMessage {
                channel,
                message,
                content,
            } => {
                validate_content(content)?;
                if !self.own(snapshot, *channel, *message) {
                    return Err(LoginError::new(es(
                        "Solo puedes editar un mensaje propio cargado en RUNE.",
                    )));
                }
                (*channel, Permissions::VIEW)
            }
            Command::DeleteMessage { channel, message } => (
                *channel,
                if self.own(snapshot, *channel, *message) {
                    Permissions::VIEW
                } else {
                    Permissions::MANAGE_MESSAGES
                },
            ),
            Command::VotePoll {
                channel,
                message,
                answers,
            } => {
                // The poll must be one this session loaded: its answers and
                // its end are checked here, never guessed.
                let Some(poll) = self
                    .entries
                    .get(&(*channel, *message))
                    .and_then(|message| message.poll.as_ref())
                else {
                    return Err(LoginError::new(es(
                        "Carga la encuesta otra vez antes de votar.",
                    )));
                };
                if poll.ended(wall_now()) {
                    return Err(LoginError::new(es("La encuesta ya terminó.")));
                }
                if !poll.valid_choice(answers) {
                    return Err(LoginError::new(es(
                        "Esas respuestas no valen para la encuesta.",
                    )));
                }
                (*channel, Permissions::READ_HISTORY)
            }
            Command::ToggleReaction { channel, emoji, .. } => {
                validate_emoji(emoji)?;
                // Discord also asks for history; RUNE asks for the reaction
                // bit even to take one back, as the interface does.
                (*channel, Permissions::READ_HISTORY | Permissions::REACT)
            }
            Command::StartTyping { channel } => (*channel, Permissions::SEND),
            Command::PinMessage { channel, .. } | Command::UnpinMessage { channel, .. } => {
                // In a direct conversation anyone in it pins; in a server,
                // whoever has Discord's pin bit (51) and the history, as the
                // official client asks (docs/DISCORD.md).
                let known = snapshot.channels.iter().find(|known| known.id == *channel);
                (
                    *channel,
                    match known {
                        Some(known) if known.server.is_none() => Permissions::NONE,
                        _ => Permissions::PIN_MESSAGES | Permissions::READ_HISTORY,
                    },
                )
            }
            Command::LoadPins { channel, .. } => (*channel, Permissions::READ_HISTORY),
            // Files may go without text; the text, if any, is bounded as a message's.
            Command::SendFiles { scope, content, .. } => {
                if content.encode_utf16().count() > 2000 {
                    return Err(LoginError::new(es(
                        "El mensaje debe contener entre 1 y 2000 caracteres.",
                    )));
                }
                (scope.channel, Permissions::SEND | Permissions::ATTACH)
            }
            Command::SendFriendRequest { .. }
            | Command::AcceptFriendRequest { .. }
            | Command::RemoveRelationship { .. }
            | Command::BlockUser { .. }
            | Command::IgnoreUser { .. }
            | Command::UnignoreUser { .. }
            | Command::SetFriendNickname { .. }
            | Command::OpenDirect { .. }
            | Command::CreateGroup { .. }
            | Command::RenameGroup { .. }
            | Command::SetGroupIcon { .. }
            | Command::AddGroupMember { .. }
            | Command::RemoveGroupMember { .. }
            | Command::LeaveGroup { .. }
            | Command::SetGroupOwner { .. }
            | Command::CloseDirect { .. } => return crate::social::allowed(snapshot, command),
            command if crate::bootstrap::is_thread_command(command) => {
                return crate::bootstrap::thread_allowed(snapshot, command)
            }
            Command::RaiseHand { .. } | Command::SetSpeaker { .. } => {
                return crate::stage::allowed(snapshot, command)
            }
            Command::EditProfile(_) | Command::EditServerProfile { .. } => {
                return crate::profiles::edit_allowed(snapshot, command)
            }
            command if crate::admin::is_admin(command) => {
                return crate::admin::allowed(snapshot, command)
            }
            command if crate::account::is_account_command(command) => {
                return crate::account::allowed(snapshot, command)
            }
            // Reads, search and remote members (P2).
            command if crate::reads::is_read(command) => {
                return crate::reads::allowed(snapshot, command)
            }
            // Message actions (P4): a poll is ended only by who started it,
            // once this session loaded it.
            Command::EndPoll { channel, message } if !self.own(snapshot, *channel, *message) => {
                return Err(LoginError::new(es(
                    "Carga la encuesta otra vez: solo quien la creó la termina.",
                )))
            }
            command if crate::message_actions::is_action(command) => {
                return crate::message_actions::allowed(snapshot, command)
            }
            // Desktop (P6): about no conversation; checked as they arrive.
            Command::SetLocalActivity(_) | Command::LoadDetectableGames => return Ok(()),
            // Streams (transmisiones): checked as they arrive, against the
            // channel and the account's own stream; nobody waits on them.
            Command::LoadStreamPreview { .. } | Command::UploadStreamPreview { .. } => {
                return Ok(())
            }
            // Private calls.
            Command::RingCall { .. } | Command::StopRinging { .. } | Command::CheckCall { .. } => {
                return crate::voice::private::allowed(snapshot, command)
            }
            // Packs and GIFs (P5): reads of the account, checked when admitted.
            command if crate::identity::is_identity(command) => return Ok(()),
            // Rules and onboarding (2026-10-18).
            command if crate::onboarding::is_onboarding(command) => {
                return crate::onboarding::allowed(snapshot, command)
            }
            // Scheduled events (eventos-programados).
            command if crate::events::is_events(command) => {
                return crate::events::allowed(snapshot, command)
            }
            // AutoMod (automod).
            command if crate::automod::is_automod(command) => {
                return crate::automod::allowed(snapshot, command)
            }
            _ => {
                return Err(LoginError::new(es(
                    "Esta acción todavía no está disponible.",
                )))
            }
        };
        // A reply reopens an archived thread, as Discord does, unless it is
        // locked to all but moderation; nothing else is done in an archived one.
        let reads = matches!(
            command,
            Command::LoadHistory { .. } | Command::LoadContext { .. } | Command::LoadPins { .. }
        );
        let own_delete =
            matches!(command, Command::DeleteMessage { .. }) && permission == Permissions::VIEW;
        if !reads && !own_delete {
            unrestricted(snapshot, channel)?;
        }
        let thread = snapshot
            .channels
            .iter()
            .find(|known| known.id == channel && known.is_thread());
        if let Some(thread) = thread.filter(|_| !reads) {
            let info = thread.thread.clone().unwrap_or_default();
            let reply = matches!(
                command,
                Command::SendMessage { .. } | Command::SendSticker { .. }
            );
            let writes = reply || matches!(command, Command::StartTyping { .. });
            if writes && info.locked && !thread.permissions.contains(Permissions::MANAGE_THREADS) {
                return Err(LoginError::new(es("Este hilo está bloqueado.")));
            }
            if info.archived && !reply {
                return Err(LoginError::new(es("Este hilo está archivado.")));
            }
        }
        // In a thread, writing takes its own bit, not the channel's.
        let permission = if thread.is_some() && permission.contains(Permissions::SEND) {
            permission.without(Permissions::SEND) | Permissions::SEND_IN_THREADS
        } else {
            permission
        };
        let visible = snapshot
            .channels
            .iter()
            .find(|candidate| candidate.id == channel)
            .is_some_and(|channel| {
                holds_chat(channel.kind)
                    && channel.permissions.contains(Permissions::VIEW | permission)
            });
        if !visible {
            return Err(LoginError::new(es(
                "No tienes permiso para esta acción en el canal.",
            )));
        }
        Ok(())
    }

    fn own(&self, snapshot: &Snapshot, channel: ChannelId, message: MessageId) -> bool {
        snapshot
            .me
            .as_ref()
            .zip(self.entries.get(&(channel, message)))
            .is_some_and(|(me, message)| message.author == me.id)
    }

    /// Cache misses cannot establish authorship. Recover only the missing proof,
    /// while a moderator already authorized to delete needs no additional read.
    pub fn needs_preflight(
        &self,
        snapshot: &Snapshot,
        command: &Command,
    ) -> Result<bool, LoginError> {
        if let Some(key) = write_target(command) {
            if !(self.entries.contains_key(&key)
                || matches!(command, Command::DeleteMessage { .. })
                    && self.allowed(snapshot, command).is_ok())
            {
                let account = snapshot.me.as_ref().ok_or_else(wire::invalid)?.id;
                Self::readable(snapshot, command, account)?;
                return Ok(true);
            }
        }
        self.allowed(snapshot, command)?;
        Ok(false)
    }

    /// Whether a confirmed change of the account's own reaction is new. A
    /// write's answer and its Gateway echo share one record, so whichever
    /// arrives second is dropped and the change counts once.
    pub fn first_confirmation(&mut self, account: UserId, event: &Event) -> bool {
        match event {
            Event::ReactionChanged {
                channel,
                message,
                emoji,
                me: true,
                added,
            } => self
                .tallies
                .own_reaction(*channel, *message, account, emoji, *added),
            _ => true,
        }
    }

    pub fn author(&self, command: &Command) -> Option<UserId> {
        self.entries
            .get(&write_target(command)?)
            .map(|message| message.author)
    }

    /// This read gate intentionally does not require the proof GET will recover.
    pub fn readable(
        snapshot: &Snapshot,
        command: &Command,
        account: UserId,
    ) -> Result<(), LoginError> {
        let (channel, message) = write_target(command).ok_or_else(wire::invalid)?;
        match command {
            Command::EditMessage { content, .. } => validate_content(content)?,
            Command::ToggleReaction { emoji, .. } => validate_emoji(emoji)?,
            _ => {}
        }
        if message.0 == 0 || snapshot.me.as_ref().is_none_or(|me| me.id != account) {
            return Err(wire::invalid());
        }
        channel_allowed(
            snapshot,
            channel,
            Permissions::VIEW | Permissions::READ_HISTORY,
        )
    }

    /// A running write keeps its small identity proof independently of the LRU.
    pub fn allowed_with_author(
        snapshot: &Snapshot,
        command: &Command,
        account: UserId,
        author: Option<UserId>,
    ) -> Result<(), LoginError> {
        let (channel, _) = write_target(command).ok_or_else(wire::invalid)?;
        if snapshot.me.as_ref().is_none_or(|me| me.id != account) {
            return Err(wire::invalid());
        }
        let permission = match command {
            Command::EditMessage { content, .. } => {
                validate_content(content)?;
                if author != Some(account) {
                    return Err(LoginError::new(es("Solo puedes editar un mensaje propio.")));
                }
                Permissions::VIEW
            }
            Command::DeleteMessage { .. } if author == Some(account) => Permissions::VIEW,
            Command::DeleteMessage { .. } => Permissions::VIEW | Permissions::MANAGE_MESSAGES,
            Command::ToggleReaction { emoji, .. } => {
                validate_emoji(emoji)?;
                Permissions::VIEW | Permissions::READ_HISTORY | Permissions::REACT
            }
            _ => return Err(wire::invalid()),
        };
        if !(matches!(command, Command::DeleteMessage { .. }) && author == Some(account)) {
            unrestricted(snapshot, channel)?;
        }
        channel_allowed(snapshot, channel, permission)
    }

    pub fn dispatch(
        &mut self,
        snapshot: &Snapshot,
        kind: &str,
        data: &Value,
        nonce_prefix: u32,
    ) -> Result<Option<Vec<Event>>, LoginError> {
        let tally = kind.starts_with("MESSAGE_REACTION_") || kind.starts_with("MESSAGE_POLL_VOTE_");
        if !tally
            && !matches!(
                kind,
                "MESSAGE_CREATE" | "MESSAGE_UPDATE" | "MESSAGE_DELETE" | "MESSAGE_DELETE_BULK"
            )
        {
            return Ok(None);
        }
        let channel = match wire::id(&data["channel_id"]) {
            Ok(id) => ChannelId(id),
            // A malformed reaction or vote is dropped, not a lost connection.
            Err(_) if tally => return Ok(Some(Vec::new())),
            Err(error) => return Err(error),
        };
        if !snapshot.channels.iter().any(|known| {
            known.id == channel
                && known.permissions.contains(Permissions::VIEW)
                && holds_chat(known.kind)
        }) {
            return Ok(Some(Vec::new()));
        }
        if tally {
            return Ok(self.tallies.dispatch(snapshot, kind, data, channel));
        }
        let events = match kind {
            "MESSAGE_CREATE" => {
                let (message, users) = wire::message(data)?;
                let mut events: Vec<_> = users.into_iter().map(Event::UserChanged).collect();
                events.push(Event::MessageCreated {
                    nonce: snapshot
                        .me
                        .as_ref()
                        .filter(|me| me.id == message.author)
                        .and_then(|_| read_nonce(data.get("nonce"), nonce_prefix)),
                    message,
                });
                events
            }
            "MESSAGE_UPDATE" => {
                let (patch, users) = wire::message_patch(data)?;
                let mut events: Vec<_> = users.into_iter().map(Event::UserChanged).collect();
                events.push(Event::MessagePatched(patch));
                events
            }
            "MESSAGE_DELETE" => vec![Event::MessageDeleted {
                channel,
                message: MessageId(wire::id(&data["id"])?),
            }],
            "MESSAGE_DELETE_BULK" => data["ids"]
                .as_array()
                .filter(|ids| ids.len() <= 100)
                .ok_or_else(wire::invalid)?
                .iter()
                .map(|id| {
                    Ok(Event::MessageDeleted {
                        channel,
                        message: MessageId(wire::id(id)?),
                    })
                })
                .collect::<Result<_, LoginError>>()?,
            _ => unreachable!(),
        };
        Ok(Some(events))
    }
}

/// Whether a channel has a chat: a voice channel and a stage have one too,
/// through the same routes and Gateway events as a text channel's.
pub(crate) fn holds_chat(kind: rune_model::ChannelKind) -> bool {
    kind.has_messages() || kind.is_voice()
}

pub(crate) fn wall_now() -> rune_model::Timestamp {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    rune_model::Timestamp(elapsed.as_millis() as i64)
}

pub(crate) fn write_target(command: &Command) -> Option<(ChannelId, MessageId)> {
    match command {
        Command::EditMessage {
            channel, message, ..
        }
        | Command::DeleteMessage { channel, message }
        // A reaction's message is recovered to validate it; the command says which way it goes.
        | Command::ToggleReaction {
            channel, message, ..
        } => Some((*channel, *message)),
        _ => None,
    }
}

/// A timed-out, screening or quarantined membership may read and take back
/// its own messages, nothing else: refused here before anything is sent.
pub(crate) fn unrestricted(snapshot: &Snapshot, channel: ChannelId) -> Result<(), LoginError> {
    let server = snapshot
        .channels
        .iter()
        .find(|known| known.id == channel)
        .and_then(|known| known.server);
    let restricted = server
        .zip(snapshot.me.as_ref())
        .and_then(|(server, me)| {
            snapshot
                .members
                .iter()
                .find(|m| m.server == server && m.user == me.id)
        })
        .and_then(|member| member.restrictions.at(wall_now()));
    match restricted {
        Some(_) => Err(LoginError::new(es(
            "Tu cuenta está restringida en este servidor: puedes leer, pero no participar.",
        ))),
        None => Ok(()),
    }
}

fn channel_allowed(
    snapshot: &Snapshot,
    channel: ChannelId,
    permission: Permissions,
) -> Result<(), LoginError> {
    snapshot
        .channels
        .iter()
        .any(|known| {
            known.id == channel && holds_chat(known.kind) && known.permissions.contains(permission)
        })
        .then_some(())
        .ok_or_else(|| LoginError::new(es("No tienes permiso para esta acción en el canal.")))
}

/// Only Unicode reactions are modeled; a custom one (`name:id`) or text
/// that cannot be one never reaches a request path.
pub(crate) fn validate_emoji(emoji: &str) -> Result<(), LoginError> {
    if emoji.is_empty()
        || emoji.len() > 64
        || emoji.is_ascii()
        || emoji
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == ':')
    {
        return Err(LoginError::new(es("Esa reacción no está disponible.")));
    }
    Ok(())
}

pub(crate) fn validate_content(content: &str) -> Result<(), LoginError> {
    if content.trim().is_empty() || content.len() > 8000 || content.encode_utf16().count() > 2000 {
        return Err(LoginError::new(es(
            "El mensaje debe contener entre 1 y 2000 caracteres.",
        )));
    }
    Ok(())
}

pub(crate) fn send_nonce(prefix: u32, nonce: u64) -> String {
    // Discord documents a maximum of 25 characters. The namespace avoids
    // treating messages sent from another device as this process's pending send.
    format!("r{prefix:08x}{nonce:016x}")
}

fn read_nonce(value: Option<&Value>, prefix: u32) -> Option<u64> {
    let nonce = value?.as_str()?;
    if nonce.len() != 25 || !nonce.starts_with(&format!("r{prefix:08x}")) {
        return None;
    }
    u64::from_str_radix(nonce.get(9..)?, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{Channel, ChannelKind, Delivery, MessageKind, Rgb, Timestamp, User, UserId};
    use serde_json::json;

    fn account() -> Snapshot {
        Snapshot {
            me: Some(User {
                identity: Default::default(),
                avatar: None,
                banner: Default::default(),
                id: UserId(42),
                name: "Owner".into(),
                display_name: None,
                accent: Rgb(0),
                bot: false,
                pronouns: None,
                about: None,
                default_avatar: None,
            }),
            channels: vec![Channel {
                id: ChannelId(20),
                server: None,
                kind: ChannelKind::Direct,
                name: "Peer".into(),
                topic: None,
                topic_pictures: Vec::new(),
                parent: None,
                position: 0,
                recipients: vec![UserId(43)],
                owner: None,
                permissions: Permissions::VIEW | Permissions::READ_HISTORY,
                overwrites: vec![],
                slowmode_secs: 0,
                nsfw: false,
                icon: None,
                thread: None,
                tags: Vec::new(),
                forum: Default::default(),
            }],
            ..Default::default()
        }
    }

    // Streams (transmisiones): until 2026-10-25 both fell to «todavía no
    // está disponible» here, so no stream picture was ever asked or sent.
    #[test]
    fn stream_pictures_get_past_the_generic_refusal() {
        let cache = Messages::default();
        for command in [
            Command::LoadStreamPreview {
                server: None,
                channel: ChannelId(20),
                owner: UserId(43),
            },
            Command::UploadStreamPreview {
                server: None,
                channel: ChannelId(20),
                owner: UserId(42),
                jpeg: vec![0xFF, 0xD8, 0xFF],
            },
        ] {
            assert_eq!(cache.needs_preflight(&account(), &command), Ok(false));
        }
    }

    #[test]
    fn a_restricted_membership_reads_and_deletes_its_own_but_writes_nothing() {
        let mut snapshot = account();
        let server = rune_model::ServerId(7);
        snapshot.channels[0].server = Some(server);
        snapshot.channels[0].kind = ChannelKind::Text;
        snapshot.channels[0].permissions = Permissions::ALL;
        let mut member = rune_model::Member {
            user: UserId(42),
            server,
            ..Default::default()
        };
        member.restrictions.timeout_until = Some(rune_model::Timestamp(i64::MAX));
        snapshot.members.push(member);
        let cache = Messages::default();
        let send = Command::SendMessage {
            channel: ChannelId(20),
            content: "hola".into(),
            reply_to: None,
            nonce: 1,
            mention_reply: false,
        };
        let read = Command::LoadHistory {
            request: 1,
            channel: ChannelId(20),
            before: None,
            limit: 50,
        };
        assert!(cache.allowed(&snapshot, &send).is_err());
        assert!(cache.allowed(&snapshot, &read).is_ok());
        let delete = Command::DeleteMessage {
            channel: ChannelId(20),
            message: MessageId(5),
        };
        let edit = Command::EditMessage {
            channel: ChannelId(20),
            message: MessageId(5),
            content: "otra".into(),
        };
        let own = Some(UserId(42));
        assert!(Messages::allowed_with_author(&snapshot, &delete, UserId(42), own).is_ok());
        assert!(Messages::allowed_with_author(&snapshot, &edit, UserId(42), own).is_err());
        assert!(
            Messages::allowed_with_author(&snapshot, &delete, UserId(42), Some(UserId(43)))
                .is_err(),
            "moderation is taking part"
        );
        // A timeout past its end restricts nothing.
        snapshot.members[0].restrictions.timeout_until = Some(rune_model::Timestamp(0));
        assert!(cache.allowed(&snapshot, &send).is_ok());
    }

    #[test]
    fn readonly_channels_and_uncached_authorship_cannot_authorize_writes() {
        let mut cache = Messages::default();
        let snapshot = account();
        assert!(cache
            .allowed(
                &snapshot,
                &Command::LoadHistory {
                    request: 1,
                    channel: ChannelId(20),
                    before: None,
                    limit: 50,
                }
            )
            .is_ok());
        assert!(cache
            .allowed(
                &snapshot,
                &Command::SendMessage {
                    channel: ChannelId(20),
                    content: "hello".into(),
                    reply_to: None,
                    nonce: 1,
                    mention_reply: false
                }
            )
            .is_err());
        let edit = Command::EditMessage {
            channel: ChannelId(20),
            message: MessageId(100),
            content: "edited".into(),
        };
        assert!(cache.allowed(&snapshot, &edit).is_err());
        cache.keep(&Message {
            id: MessageId(100),
            channel: ChannelId(20),
            author: UserId(42),
            content: "hello".into(),
            timestamp: Timestamp(0),
            edited: None,
            kind: MessageKind::Default,
            reply_to: None,
            mentions: vec![],
            reactions: vec![],
            attachments: vec![],
            stickers: Vec::new(),
            text_references: Vec::new(),
            delivery: Delivery::Sent,
            pinned: false,
            embeds: Vec::new(),
            forwarded: None,
            poll: None,
            components: Default::default(),
            call: None,
            mention_roles: Vec::new(),
            mention_everyone: false,
        });
        assert!(cache.allowed(&snapshot, &edit).is_ok());
    }

    #[test]
    fn a_gateway_patch_survives_more_than_512_adapter_cache_evictions() {
        let mut cache = Messages::default();
        let (mut retained, _) = wire::message(&json!({"id":"100","channel_id":"20",
            "author":{"id":"42","username":"Owner"},"content":"still visible",
            "timestamp":"2026-10-14T12:34:56Z"}))
        .unwrap();
        cache.keep(&retained);
        for id in 1000..=1512 {
            cache.keep(&Message {
                id: MessageId(id),
                channel: ChannelId(21),
                ..retained.clone()
            });
        }
        assert_eq!(cache.entries.len(), CACHE_LIMIT);
        assert!(!cache.entries.contains_key(&(retained.channel, retained.id)));
        let updates = cache
            .dispatch(
                &account(),
                "MESSAGE_UPDATE",
                &json!({
                    "id":"100", "channel_id":"20", "content":"changed elsewhere"
                }),
                0,
            )
            .unwrap()
            .unwrap();
        let [Event::MessagePatched(patch)] = updates.as_slice() else {
            panic!("an evicted message still emits its typed patch");
        };
        assert!(patch.apply(&mut retained));
        assert_eq!(retained.content, "changed elsewhere");
        assert_eq!(retained.author, UserId(42));
        cache.observe(&updates[0]);
        assert_eq!(cache.entries.len(), CACHE_LIMIT);
        assert!(
            !cache.entries.contains_key(&(retained.channel, retained.id)),
            "a patch never reconstructs an evicted baseline"
        );
    }

    #[test]
    fn cached_patches_preserve_absence_and_reject_author_or_version_rollback() {
        let mut cache = Messages::default();
        let (mut original, _) = wire::message(&json!({"id":"100","channel_id":"20",
            "author":{"id":"42","username":"Owner"},"content":"before",
            "timestamp":"2026-10-14T12:34:56Z"}))
        .unwrap();
        original.edited = Some(Timestamp(30));
        cache.keep(&original);
        for data in [
            json!({"content":"stale","edited_timestamp":null}),
            json!({"author":{"id":"43"},"content":"not mine"}),
        ] {
            let mut data = data;
            data["id"] = json!("100");
            data["channel_id"] = json!("20");
            for event in cache
                .dispatch(&account(), "MESSAGE_UPDATE", &data, 0)
                .unwrap()
                .unwrap()
            {
                cache.observe(&event);
            }
            assert_eq!(cache.entries[&(ChannelId(20), MessageId(100))], original);
        }
        let updates = cache
            .dispatch(
                &account(),
                "MESSAGE_UPDATE",
                &json!({"id":"100","channel_id":"20","pinned":true}),
                0,
            )
            .unwrap()
            .unwrap();
        for event in updates {
            cache.observe(&event);
        }
        let current = &cache.entries[&(ChannelId(20), MessageId(100))];
        assert!(current.pinned);
        assert_eq!(current.content, original.content);
        assert_eq!(current.edited, original.edited);
    }

    #[test]
    fn a_thread_takes_replies_with_its_own_bit_and_a_reply_reopens_it() {
        let cache = Messages::default();
        let mut snapshot = account();
        snapshot.channels[0].kind = ChannelKind::Thread;
        snapshot.channels[0].thread = Some(rune_model::Thread::default());
        snapshot.channels[0].permissions = Permissions::VIEW
            | Permissions::READ_HISTORY
            | Permissions::REACT
            | Permissions::SEND_IN_THREADS;
        let read = Command::LoadHistory {
            request: 1,
            channel: ChannelId(20),
            before: None,
            limit: 50,
        };
        assert!(cache.allowed(&snapshot, &read).is_ok());
        let send = Command::SendMessage {
            channel: ChannelId(20),
            content: "hola".into(),
            reply_to: None,
            nonce: 1,
            mention_reply: false,
        };
        let typing = Command::StartTyping {
            channel: ChannelId(20),
        };
        let react = Command::ToggleReaction {
            channel: ChannelId(20),
            message: MessageId(100),
            emoji: "👍".into(),
            add: true,
        };
        // Without SEND: the thread's own bit is what counts.
        assert!(cache.allowed(&snapshot, &send).is_ok());
        assert!(cache.allowed(&snapshot, &typing).is_ok());
        fn thread(snapshot: &mut Snapshot) -> &mut rune_model::Thread {
            snapshot.channels[0].thread.as_mut().unwrap()
        }
        // Archived: a reply reopens it; typing and reacting wait for that.
        thread(&mut snapshot).archived = true;
        assert!(cache.allowed(&snapshot, &send).is_ok());
        assert!(cache.allowed(&snapshot, &typing).is_err());
        assert!(cache.allowed(&snapshot, &react).is_err());
        // Locked: only moderation replies.
        thread(&mut snapshot).locked = true;
        assert!(cache.allowed(&snapshot, &send).is_err());
        snapshot.channels[0].permissions =
            snapshot.channels[0].permissions | Permissions::MANAGE_THREADS;
        assert!(cache.allowed(&snapshot, &send).is_ok());
        snapshot.channels[0].permissions = Permissions::VIEW | Permissions::READ_HISTORY;
        thread(&mut snapshot).locked = false;
        assert!(cache.allowed(&snapshot, &send).is_err());
    }

    #[test]
    fn in_a_server_the_pin_bit_and_the_history_pin() {
        let cache = Messages::default();
        let mut snapshot = account();
        snapshot.channels[0].server = Some(rune_model::ServerId(7));
        snapshot.channels[0].kind = ChannelKind::Text;
        let pin = Command::PinMessage {
            channel: ChannelId(20),
            message: MessageId(100),
        };
        let unpin = Command::UnpinMessage {
            channel: ChannelId(20),
            message: MessageId(100),
        };
        assert!(cache.allowed(&snapshot, &pin).is_err());
        snapshot.channels[0].permissions =
            Permissions::VIEW | Permissions::READ_HISTORY | Permissions::PIN_MESSAGES;
        assert!(cache.allowed(&snapshot, &pin).is_ok());
        assert!(cache.allowed(&snapshot, &unpin).is_ok());
        // Managing messages alone, or the pin bit without the history, does
        // not: the official client offers neither.
        for lacking in [
            Permissions::VIEW | Permissions::READ_HISTORY | Permissions::MANAGE_MESSAGES,
            Permissions::VIEW | Permissions::PIN_MESSAGES,
        ] {
            snapshot.channels[0].permissions = lacking;
            assert!(cache.allowed(&snapshot, &pin).is_err(), "{lacking:?}");
            assert!(cache.allowed(&snapshot, &unpin).is_err(), "{lacking:?}");
        }
        // Discord's bit 51 is the pin bit.
        let bits = crate::bootstrap::permission_bits(&json!((1_u64 << 51).to_string())).unwrap();
        assert_eq!(bits, Permissions::PIN_MESSAGES);
    }

    #[test]
    fn stickers_require_current_server_catalog_availability_and_send_permission() {
        use rune_model::{Server, ServerId, Sticker, StickerId};
        let cache = Messages::default();
        let mut snapshot = account();
        snapshot.channels[0].server = Some(ServerId(10));
        snapshot.channels[0].kind = ChannelKind::Text;
        let sticker = Sticker {
            id: StickerId(501),
            name: "Wave".into(),
            image: None,
            available: true,
        };
        snapshot.servers.push(Server {
            id: ServerId(10),
            name: "Test".into(),
            accent: Rgb(0),
            icon: None,
            roles: vec![],
            emojis: vec![],
            stickers: vec![sticker],
            owner: None,
            description: None,
            options: Default::default(),
        });
        let command = Command::SendSticker {
            channel: ChannelId(20),
            sticker: StickerId(501),
            nonce: 1,
        };
        assert!(cache.allowed(&snapshot, &command).is_err());
        snapshot.channels[0].permissions = snapshot.channels[0].permissions | Permissions::SEND;
        assert!(cache.allowed(&snapshot, &command).is_ok());
        snapshot.servers[0].stickers[0].available = false;
        assert!(cache.allowed(&snapshot, &command).is_err());
        snapshot.servers[0].stickers[0].available = true;
        snapshot.channels[0].server = None;
        assert!(cache.allowed(&snapshot, &command).is_err());
        snapshot.channels[0].server = Some(ServerId(10));
        snapshot.servers[0].stickers.clear();
        assert!(cache.allowed(&snapshot, &command).is_err());
    }

    #[test]
    fn context_messages_supply_the_same_bounded_action_baseline_as_history() {
        let mut cache = Messages::default();
        let (message, _) = wire::message(&json!({"id":"100","channel_id":"20",
            "author":{"id":"42","username":"Owner"},"content":"around target",
            "timestamp":"2026-10-14T12:34:56Z"}))
        .unwrap();
        cache.observe(&Event::ContextLoaded {
            request: 5,
            channel: message.channel,
            anchor: message.id,
            messages: vec![message.clone()],
        });
        assert!(cache
            .allowed(
                &account(),
                &Command::EditMessage {
                    channel: message.channel,
                    message: message.id,
                    content: "edit from context".into()
                }
            )
            .is_ok());
    }

    #[test]
    fn only_this_session_nonce_can_confirm_its_pending_message() {
        let nonce = json!(send_nonce(123, u64::MAX));
        assert_eq!(read_nonce(Some(&nonce), 123), Some(u64::MAX));
        assert_eq!(read_nonce(Some(&nonce), 124), None);
        assert_eq!(read_nonce(Some(&json!(123)), 123), None);
    }
}
