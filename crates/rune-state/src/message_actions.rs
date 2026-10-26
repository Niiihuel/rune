//! What message actions and voice moderation ask of the state (P4): the
//! link to a message, who may end a poll, where a message may be
//! forwarded, and what the account may do to someone's voice.

use rune_i18n::t;
use rune_model::{Channel, ChannelId, ChannelKind, MessageId, ServerId, Timestamp, UserId};

use crate::{Permit, Store};

/// What the account may do to someone in voice in a server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceModerationAbilities {
    pub server: ServerId,
    /// Where they are.
    pub channel: ChannelId,
    pub mute: Permit,
    pub deafen: Permit,
    /// Move them, to one of `rooms`.
    pub move_to: Permit,
    pub disconnect: Permit,
    /// The server's other voice channels they may be moved to, in order.
    pub rooms: Vec<ChannelId>,
}

impl Store {
    /// The address that opens `message` of `channel` elsewhere, when the
    /// provider has one.
    pub fn message_link(&self, channel: ChannelId, message: MessageId) -> Option<String> {
        let base = self.capabilities().channel_links?;
        let found = self.channel(channel)?;
        let scope = found
            .server
            .map_or_else(|| "@me".to_owned(), |server| server.to_string());
        Some(format!("{base}/{scope}/{channel}/{message}"))
    }

    /// Whether the account may end the poll on `message` of `channel` at
    /// `now`, and why not: only who started it, while it is open.
    pub fn end_poll(&self, channel: ChannelId, message: MessageId, now: Timestamp) -> Permit {
        let found = self
            .timeline(channel)
            .and_then(|timeline| timeline.get(message));
        let poll = found.and_then(|found| found.poll.as_ref());
        if !self.capabilities().create_polls {
            Permit::Denied(t("Este proveedor no permite terminar encuestas").into())
        } else if found.is_none_or(|found| Some(found.author) != self.me()) {
            Permit::Denied(t("Solo quien creó la encuesta la termina").into())
        } else if poll.is_none_or(|poll| poll.ended(now)) {
            Permit::Denied(t("La encuesta ya terminó").into())
        } else if let Some(denied) = self.offline() {
            denied
        } else {
            Permit::Allowed
        }
    }

    /// The conversations the account may write in whose name, server or
    /// people hold `query`, ignoring case; the most recently active first,
    /// at most `limit`.
    pub fn forward_targets(&self, query: &str, limit: usize) -> Vec<ChannelId> {
        let query = query.trim().to_lowercase();
        let mut found: Vec<(Option<MessageId>, ChannelId)> = self
            .channels
            .values()
            .filter(|channel| channel.kind.has_messages() && Store::visible(channel))
            .filter(|channel| {
                self.abilities(channel.id)
                    .is_some_and(|abilities| abilities.write.allowed())
            })
            .filter(|channel| query.is_empty() || self.names(channel).contains(&query))
            .map(|channel| (self.read(channel.id).last_message, channel.id))
            .collect();
        found.sort_by(|a, b| b.cmp(a));
        found
            .into_iter()
            .take(limit)
            .map(|(_, channel)| channel)
            .collect()
    }

    /// What a conversation is searched by: its title, its server's name
    /// and its people's names, in lower case.
    fn names(&self, channel: &Channel) -> String {
        let mut names = self.channel_title(channel.id);
        if let Some(server) = channel.server.and_then(|server| self.server(server)) {
            names.push(' ');
            names.push_str(&server.name);
        }
        for user in channel
            .recipients
            .iter()
            .filter_map(|user| self.user(*user))
        {
            names.push(' ');
            names.push_str(&user.name);
            names.push(' ');
            names.push_str(user.shown_name());
        }
        names.to_lowercase()
    }

    /// What the account may do to `user`'s voice in `server`, while they
    /// are in one of its voice channels; `None` otherwise or for the
    /// account itself. As in Discord, voice moderation has no role
    /// hierarchy: the permission alone decides, and the connection or the
    /// lack of another room are the only reasons it is denied.
    pub fn voice_moderation(
        &self,
        server: ServerId,
        user: UserId,
    ) -> Option<VoiceModerationAbilities> {
        if self.me() == Some(user) {
            return None;
        }
        let channel = self
            .voice_in_server(server)
            .into_iter()
            .find(|state| state.user == user)?
            .channel;
        let mut abilities = self.server_abilities(server)?;
        let unavailable = self.offline();
        for permit in [
            &mut abilities.mute_members,
            &mut abilities.deafen_members,
            &mut abilities.move_members,
        ] {
            if let (true, Some(denied)) = (permit.allowed(), &unavailable) {
                *permit = denied.clone();
            }
        }
        let rooms: Vec<ChannelId> = self
            .channel_tree(server)
            .into_iter()
            .flat_map(|(_, channels)| channels)
            .filter(|room| room.id != channel && room.kind == ChannelKind::Voice)
            .filter(|room| {
                self.abilities(room.id)
                    .is_some_and(|abilities| abilities.join_voice.allowed())
            })
            .map(|room| room.id)
            .collect();
        let move_to = match &abilities.move_members {
            Permit::Allowed if rooms.is_empty() => {
                Permit::Denied(t("No hay otro canal de voz al que puedas moverlo").into())
            }
            permit => permit.clone(),
        };
        Some(VoiceModerationAbilities {
            server,
            channel,
            mute: abilities.mute_members,
            deafen: abilities.deafen_members,
            disconnect: abilities.move_members,
            move_to,
            rooms,
        })
    }

    /// The reply sent with `nonce` does not notify who it answers, so a
    /// retry of it does not either.
    pub fn quiet_reply(&mut self, nonce: u64) {
        self.quiet_replies.insert(nonce);
    }

    /// Where `user` is in voice, and how, when they are.
    pub fn voice_state_of(&self, user: UserId) -> Option<&rune_model::VoiceState> {
        self.voice.iter().find(|state| state.user == user)
    }

    /// Who is in voice in `server`.
    fn voice_in_server(&self, server: ServerId) -> Vec<&rune_model::VoiceState> {
        self.voice
            .iter()
            .filter(|state| self.channel(state.channel).and_then(|c| c.server) == Some(server))
            .collect()
    }
}
