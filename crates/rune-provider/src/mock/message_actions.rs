//! Message actions and voice moderation in the simulation (P4): forwards,
//! polls started and ended early, who reacted and who voted, conversations
//! opened from a link, and the moderation of voice. Each one checks what the
//! account may do the way a service would. Who reacted or voted is made up
//! from the counts, the same for the same message every time.

use rune_i18n::t;
use rune_model::{
    ChannelId, ChannelKind, Forwarded, MessageId, Permissions, Poll, PollAnswer, ServerId,
    TextReference, TextTarget, Timestamp, UserId, VoiceState,
};

use super::{mix, World, ME};
use crate::{Command, Event, Failure, NewPoll, VoiceModeration};

type Outcome = Result<Vec<Event>, String>;

/// Where the simulation's links start: the reserved `.invalid` domain, so
/// none of them opens anything outside RUNE.
pub(super) const LINKS: &str = "https://simulado.invalid/channels";

/// How many people a page of who reacted or voted holds.
const PAGE: usize = 100;

/// Mixed into who reacted and voted, apart from every other stream.
const SALT: u64 = 0x5045_4f50;

impl World {
    /// Carry out a message action or a voice moderation, and report what
    /// changed, or why not.
    pub(super) fn message_action(&mut self, command: Command, now: Timestamp) -> Vec<Event> {
        self.now = now;
        let failure = Failure::of(&command).unwrap_or(Failure::Other);
        let outcome = match command {
            Command::ForwardMessage {
                channel,
                message,
                to,
                nonce,
            } => self.forward(channel, message, to, nonce, now),
            Command::CreatePoll {
                channel,
                poll,
                nonce,
            } => self.create_poll(channel, poll, nonce, now),
            Command::EndPoll { channel, message } => self.end_poll(channel, message, now),
            Command::LoadReactions {
                request,
                channel,
                message,
                emoji,
                after,
            } => self
                .reacted(channel, message, &emoji)
                .map(|people| page(request, people, after)),
            Command::LoadPollVoters {
                request,
                channel,
                message,
                answer,
                after,
            } => self
                .voted(channel, message, answer)
                .map(|people| page(request, people, after)),
            Command::LoadChannel { request, channel } => self.load_channel(request, channel),
            Command::ModerateVoice {
                server,
                user,
                change,
            } => self.moderate_voice(server, user, change),
            _ => Err(t("la simulación no conoce esa acción").into()),
        };
        outcome.unwrap_or_else(|reason| {
            vec![Event::Failed {
                failure,
                reason,
                ambiguous: false,
            }]
        })
    }

    /// Whether the account may write in `channel` with `also`.
    fn may_write(
        &self,
        channel: ChannelId,
        also: Permissions,
        now: Timestamp,
    ) -> Result<(), String> {
        let found = self
            .channel(channel)
            .ok_or(t("la conversación no existe"))?;
        if !found.kind.has_messages() {
            return Err(t("ahí no se escriben mensajes").into());
        }
        if found.server.is_some() && !found.permissions.contains(Permissions::SEND | also) {
            return Err(t("no tienes permiso").into());
        }
        if self.restricted_in(channel, now).is_some() {
            return Err(t("estás aislado en este servidor (simulado)").into());
        }
        Ok(())
    }

    fn forward(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        to: ChannelId,
        nonce: u64,
        now: Timestamp,
    ) -> Outcome {
        let original = self.message(channel, id).ok_or(t("el mensaje no existe"))?;
        if original.poll.is_some() || !original.kind.is_said() {
            return Err(t("ese mensaje no se puede reenviar").into());
        }
        self.may_write(to, Permissions::NONE, now)?;
        let mut message = self
            .post_once(nonce, to, String::new(), None, now)
            .ok_or(t("la conversación no existe"))?;
        if message.forwarded.is_none() {
            message.forwarded = Some(Forwarded {
                content: original.content,
                timestamp: original.timestamp,
                edited: original.edited,
                attachments: original.attachments,
                embeds: original.embeds,
                components: original.components,
                text_references: original.text_references,
            });
            self.store(to, message.clone());
        }
        Ok(vec![Event::MessageCreated {
            message,
            nonce: Some(nonce),
        }])
    }

    fn create_poll(
        &mut self,
        channel: ChannelId,
        poll: NewPoll,
        nonce: u64,
        now: Timestamp,
    ) -> Outcome {
        if let Some(problem) = poll.problem() {
            return Err(problem.to_lowercase());
        }
        self.may_write(channel, Permissions::SEND_POLLS, now)?;
        let mut message = self
            .post_once(nonce, channel, String::new(), None, now)
            .ok_or(t("la conversación no existe"))?;
        if message.poll.is_none() {
            message.poll = Some(Poll {
                question: poll.question.trim().to_owned(),
                answers: poll
                    .answers
                    .into_iter()
                    .zip(1..)
                    .map(|(answer, id)| PollAnswer {
                        id,
                        text: answer.text.trim().to_owned(),
                        emoji: answer.emoji,
                        votes: 0,
                        me: false,
                    })
                    .collect(),
                expiry: Some(Timestamp(now.0 + i64::from(poll.hours) * 3_600_000)),
                multiselect: poll.multiselect,
                results_known: true,
                finalized: false,
            });
            self.store(channel, message.clone());
        }
        Ok(vec![Event::MessageCreated {
            message,
            nonce: Some(nonce),
        }])
    }

    fn end_poll(&mut self, channel: ChannelId, id: MessageId, now: Timestamp) -> Outcome {
        let mut message = self.message(channel, id).ok_or(t("el mensaje no existe"))?;
        if message.author != ME {
            return Err(t("solo quien la creó termina una encuesta").into());
        }
        let poll = message
            .poll
            .as_mut()
            .ok_or(t("ese mensaje no tiene encuesta"))?;
        if poll.ended(now) {
            return Err(t("la encuesta ya terminó").into());
        }
        poll.expiry = Some(now);
        poll.finalized = true;
        self.store(channel, message.clone());
        Ok(vec![Event::MessageUpdated(message)])
    }

    /// Everyone who reacted with `emoji`: the account when it did, and as
    /// many of the conversation's people as the count says, by id.
    fn reacted(
        &self,
        channel: ChannelId,
        id: MessageId,
        emoji: &str,
    ) -> Result<Vec<UserId>, String> {
        let message = self.message(channel, id).ok_or(t("el mensaje no existe"))?;
        let reaction = message
            .reactions
            .iter()
            .find(|reaction| reaction.emoji == emoji)
            .ok_or(t("nadie reaccionó con ese emoji"))?;
        let salt = emoji
            .bytes()
            .fold(0u64, |hash, byte| hash.rotate_left(5) ^ u64::from(byte));
        Ok(self.drawn(channel, id.0 ^ salt, reaction.count, reaction.me))
    }

    /// Everyone who voted for `answer`, like [`World::reacted`].
    fn voted(&self, channel: ChannelId, id: MessageId, answer: u32) -> Result<Vec<UserId>, String> {
        let message = self.message(channel, id).ok_or(t("el mensaje no existe"))?;
        let found = message
            .poll
            .as_ref()
            .and_then(|poll| poll.answers.iter().find(|known| known.id == answer))
            .ok_or(t("esa respuesta no existe"))?;
        Ok(self.drawn(channel, id.0 ^ u64::from(answer), found.votes, found.me))
    }

    /// `count` people of `channel`, the account among them when `me`, the
    /// same ones for the same `seed`, by ascending id.
    fn drawn(&self, channel: ChannelId, seed: u64, count: u32, me: bool) -> Vec<UserId> {
        let mut others: Vec<UserId> = self
            .participants(channel)
            .into_iter()
            .filter(|user| *user != ME)
            .collect();
        others.sort();
        let mut rng = mix(&[self.seed, channel.0, seed, SALT]);
        let wanted = (count as usize).saturating_sub(usize::from(me));
        let mut people: Vec<UserId> = Vec::new();
        while people.len() < wanted && !others.is_empty() {
            people.push(others.remove(rng.below(others.len() as u64) as usize));
        }
        if me {
            people.push(ME);
        }
        people.sort();
        people
    }

    /// A conversation a link names: a closed direct one comes back to the
    /// list, an archived thread is reported as it is.
    fn load_channel(&mut self, request: u64, id: ChannelId) -> Outcome {
        let found = match self.closed.iter().position(|closed| closed.id == id) {
            Some(index) => {
                let channel = self.closed.remove(index);
                self.channels.push(channel.clone());
                channel
            }
            None => self
                .channel(id)
                .cloned()
                .ok_or(t("no tienes acceso a esa conversación"))?,
        };
        if found.server.is_some() && !found.permissions.contains(Permissions::VIEW) {
            return Err(t("no tienes acceso a esa conversación").into());
        }
        Ok(vec![
            Event::ChannelChanged(found),
            Event::ChannelLoaded {
                request,
                channel: id,
            },
        ])
    }

    fn moderate_voice(
        &mut self,
        server: ServerId,
        user: UserId,
        change: VoiceModeration,
    ) -> Outcome {
        let needed = match change {
            VoiceModeration::Mute(_) => Permissions::MUTE_MEMBERS,
            VoiceModeration::Deafen(_) => Permissions::DEAFEN_MEMBERS,
            VoiceModeration::Move(_) | VoiceModeration::Disconnect => Permissions::MOVE_MEMBERS,
        };
        self.require(server, needed)?;
        self.require_over(server, user)?;
        let index = self
            .voice
            .iter()
            .position(|state| {
                state.user == user
                    && self.channel(state.channel).and_then(|c| c.server) == Some(server)
            })
            .ok_or(t("esa persona no está en un canal de voz del servidor"))?;
        if let VoiceModeration::Move(to) = change {
            let fits = self.channel(to).is_some_and(|found| {
                found.server == Some(server)
                    && found.kind == ChannelKind::Voice
                    && found
                        .permissions
                        .contains(Permissions::VIEW | Permissions::CONNECT)
            });
            if !fits || self.voice[index].channel == to {
                return Err(t("no se puede mover a ese canal").into());
            }
        }
        let state: &mut VoiceState = &mut self.voice[index];
        match change {
            VoiceModeration::Mute(on) => state.server_muted = on,
            VoiceModeration::Deafen(on) => state.server_deafened = on,
            VoiceModeration::Move(to) => state.channel = to,
            VoiceModeration::Disconnect => {
                let gone = self.voice.remove(index);
                return Ok(vec![Event::VoiceLeft {
                    user,
                    channel: gone.channel,
                }]);
            }
        }
        Ok(vec![Event::VoiceChanged(self.voice[index].clone())])
    }
}

/// The page of `people` after `after`, as the service gives them.
fn page(request: u64, people: Vec<UserId>, after: Option<UserId>) -> Vec<Event> {
    let rest: Vec<UserId> = people
        .into_iter()
        .filter(|user| after.is_none_or(|after| *user > after))
        .collect();
    let reached_end = rest.len() <= PAGE;
    vec![Event::PeopleLoaded {
        request,
        people: rest.into_iter().take(PAGE).collect(),
        reached_end,
    }]
}

/// The simulation's links in `content`, written bare, as references that
/// open in RUNE, and its moments written as `<t:seconds:style>`.
pub(super) fn references(content: &str) -> Vec<TextReference> {
    let mut found = Vec::new();
    for word in content.split_whitespace() {
        let word = word.trim_end_matches(['.', ',', ')', ';']);
        let Some(path) = word
            .strip_prefix(LINKS)
            .and_then(|rest| rest.strip_prefix('/'))
        else {
            continue;
        };
        let parts: Vec<&str> = path.split('/').collect();
        let number = |text: &str| text.parse::<u64>().ok().filter(|id| *id != 0);
        let target = match parts.as_slice() {
            [server, channel] | [server, channel, ""] => {
                number(channel).map(|channel| (server, channel, None))
            }
            [server, channel, message] => number(channel)
                .zip(number(message))
                .map(|(channel, message)| (server, channel, Some(message))),
            _ => None,
        };
        let Some((server, channel, message)) = target else {
            continue;
        };
        let server = match *server {
            "@me" => None,
            other => match number(other) {
                Some(server) => Some(ServerId(server)),
                None => continue,
            },
        };
        found.push(TextReference {
            source: word.to_owned(),
            target: TextTarget::Link {
                server,
                channel: ChannelId(channel),
                message: message.map(MessageId),
            },
        });
    }
    let mut rest = content;
    while let Some(start) = rest.find("<t:") {
        rest = &rest[start..];
        let Some(end) = rest.find('>') else { break };
        let body = &rest[3..end];
        let (seconds, style) = body.split_once(':').unwrap_or((body, "f"));
        let style = match style {
            "t" => Some(rune_model::TimeStyle::ShortTime),
            "T" => Some(rune_model::TimeStyle::LongTime),
            "d" => Some(rune_model::TimeStyle::ShortDate),
            "D" => Some(rune_model::TimeStyle::LongDate),
            "f" => Some(rune_model::TimeStyle::ShortDateTime),
            "F" => Some(rune_model::TimeStyle::LongDateTime),
            "R" => Some(rune_model::TimeStyle::Relative),
            _ => None,
        };
        if let (Ok(seconds), Some(style)) = (seconds.parse::<i64>(), style) {
            found.push(TextReference {
                source: rest[..=end].to_owned(),
                target: TextTarget::Time {
                    at: Timestamp(seconds.saturating_mul(1000)),
                    style,
                },
            });
        }
        rest = &rest[end + 1..];
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockConfig;
    use crate::NewPollAnswer;
    use rune_model::Message;

    fn world() -> World {
        World::build(&MockConfig::default())
    }

    fn named(world: &World, name: &str) -> ChannelId {
        world.channels.iter().find(|c| c.name == name).unwrap().id
    }

    fn created(events: &[Event]) -> &Message {
        match &events[0] {
            Event::MessageCreated { message, .. } => message,
            other => panic!("{other:?}"),
        }
    }

    fn failed(events: &[Event]) -> bool {
        matches!(events, [Event::Failed { .. }])
    }

    #[test]
    fn a_forward_copies_the_message_once_per_nonce() {
        let mut world = world();
        let general = named(&world, "general");
        let direct = world.first_direct();
        let total = world.sims[&general].generated;
        let original = world
            .message(general, super::super::message_id(general, total - 20))
            .unwrap();
        let command = Command::ForwardMessage {
            channel: general,
            message: original.id,
            to: direct,
            nonce: 77,
        };
        let first = world.message_action(command.clone(), Timestamp(5));
        let copy = created(&first);
        assert_eq!(copy.channel, direct);
        assert_eq!(copy.forwarded.as_ref().unwrap().content, original.content);
        let again = world.message_action(command, Timestamp(6));
        assert_eq!(created(&again).id, copy.id);
        // What its text names, the copy names too.
        let named = world
            .post(
                general,
                original.author,
                "a las <t:1790000000:t>".into(),
                None,
                Timestamp(7),
            )
            .unwrap();
        assert!(!named.text_references.is_empty());
        let forwarded = world.message_action(
            Command::ForwardMessage {
                channel: general,
                message: named.id,
                to: direct,
                nonce: 78,
            },
            Timestamp(8),
        );
        assert_eq!(
            created(&forwarded)
                .forwarded
                .as_ref()
                .unwrap()
                .text_references,
            named.text_references
        );
    }

    #[test]
    fn polls_start_checked_and_only_their_author_ends_them() {
        let mut world = world();
        let general = named(&world, "general");
        let answer = |text: &str| NewPollAnswer {
            text: text.into(),
            emoji: None,
        };
        let mut poll = NewPoll {
            question: "¿Cuándo?".into(),
            answers: vec![answer("Hoy")],
            hours: 24,
            multiselect: true,
        };
        let refused = world.message_action(
            Command::CreatePoll {
                channel: general,
                poll: poll.clone(),
                nonce: 1,
            },
            Timestamp(10),
        );
        assert!(failed(&refused));
        poll.answers.push(answer("Mañana"));
        let made = world.message_action(
            Command::CreatePoll {
                channel: general,
                poll,
                nonce: 2,
            },
            Timestamp(10),
        );
        let message = created(&made).clone();
        let started = message.poll.as_ref().unwrap();
        assert_eq!(started.answers.len(), 2);
        assert_eq!(started.expiry, Some(Timestamp(10 + 24 * 3_600_000)));
        assert!(started.multiselect && !started.ended(Timestamp(11)));
        let ended = world.message_action(
            Command::EndPoll {
                channel: general,
                message: message.id,
            },
            Timestamp(20),
        );
        let [Event::MessageUpdated(updated)] = &ended[..] else {
            panic!("{ended:?}")
        };
        assert!(updated.poll.as_ref().unwrap().ended(Timestamp(20)));
        // Someone else's showcase poll is not the account's to end.
        let total = world.sims[&general].generated;
        let theirs = super::super::message_id(general, total - 10);
        assert!(failed(&world.message_action(
            Command::EndPoll {
                channel: general,
                message: theirs,
            },
            Timestamp(20),
        )));
        // Its voters are as many as its count says, the same every time.
        let voters = |world: &mut World| {
            world.message_action(
                Command::LoadPollVoters {
                    request: 3,
                    channel: general,
                    message: theirs,
                    answer: 1,
                    after: None,
                },
                Timestamp(20),
            )
        };
        let first = voters(&mut world);
        let [Event::PeopleLoaded {
            people,
            reached_end: true,
            ..
        }] = &first[..]
        else {
            panic!("{first:?}")
        };
        let votes = world
            .message(general, theirs)
            .unwrap()
            .poll
            .unwrap()
            .answers[0]
            .votes;
        assert_eq!(people.len(), votes as usize);
        assert_eq!(voters(&mut world), first);
    }

    #[test]
    fn who_reacted_matches_the_count_and_includes_the_account() {
        let mut world = world();
        let general = named(&world, "general");
        let total = world.sims[&general].generated;
        let message = (1..200)
            .filter_map(|back| {
                world.message(general, super::super::message_id(general, total - back))
            })
            .find(|message| !message.reactions.is_empty())
            .expect("a message with reactions");
        let reaction = message.reactions[0].clone();
        let mut mine = message.clone();
        if !reaction.me {
            mine.toggle_reaction(&reaction.emoji);
            world.store(general, mine.clone());
        }
        let loaded = world.message_action(
            Command::LoadReactions {
                request: 9,
                channel: general,
                message: message.id,
                emoji: reaction.emoji.clone(),
                after: None,
            },
            Timestamp(1),
        );
        let [Event::PeopleLoaded {
            request: 9, people, ..
        }] = &loaded[..]
        else {
            panic!("{loaded:?}")
        };
        let count = mine.reactions[0].count;
        assert_eq!(people.len(), count as usize);
        assert!(people.contains(&ME));
        assert!(people.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_closed_conversation_comes_back_from_its_link() {
        let mut world = world();
        let direct = world.first_direct();
        let index = world.channels.iter().position(|c| c.id == direct).unwrap();
        let closed = world.channels.remove(index);
        world.closed.push(closed);
        let loaded = world.message_action(
            Command::LoadChannel {
                request: 4,
                channel: direct,
            },
            Timestamp(1),
        );
        assert!(matches!(
            &loaded[..],
            [Event::ChannelChanged(found), Event::ChannelLoaded { request: 4, channel }]
                if found.id == direct && *channel == direct
        ));
        assert!(world.channel(direct).is_some());
        assert!(failed(&world.message_action(
            Command::LoadChannel {
                request: 5,
                channel: ChannelId(424242),
            },
            Timestamp(1),
        )));
    }

    #[test]
    fn voice_moderation_needs_the_permission_and_the_rank() {
        let mut world = world();
        let owned = world
            .servers
            .iter()
            .find(|server| server.owner == Some(ME))
            .unwrap()
            .id;
        let room = world
            .channels
            .iter()
            .find(|c| c.server == Some(owned) && c.kind == ChannelKind::Voice)
            .unwrap()
            .clone();
        // A second room to move to.
        let mut second = room.clone();
        second.id = ChannelId(room.id.0 + 999_999);
        world.channels.push(second.clone());
        let rooms = [room.id, second.id];
        let someone = world
            .members
            .iter()
            .find(|member| member.server == owned && member.user != ME)
            .unwrap()
            .user;
        world.voice.retain(|state| state.user != someone);
        world.voice.push(VoiceState {
            user: someone,
            channel: rooms[0],
            muted: false,
            deafened: false,
            speaking: false,
            streaming: false,
            server_muted: false,
            server_deafened: false,
            camera: false,
            suppressed: false,
            speak_request: None,
        });
        let moderate = |world: &mut World, change| {
            world.message_action(
                Command::ModerateVoice {
                    server: owned,
                    user: someone,
                    change,
                },
                Timestamp(1),
            )
        };
        let muted = moderate(&mut world, VoiceModeration::Mute(true));
        assert!(matches!(&muted[..], [Event::VoiceChanged(state)] if state.server_muted));
        let deaf = moderate(&mut world, VoiceModeration::Deafen(true));
        assert!(matches!(&deaf[..], [Event::VoiceChanged(state)] if state.server_deafened));
        assert!(failed(&moderate(
            &mut world,
            VoiceModeration::Move(rooms[0])
        )));
        let moved = moderate(&mut world, VoiceModeration::Move(rooms[1]));
        assert!(matches!(&moved[..], [Event::VoiceChanged(state)] if state.channel == rooms[1]));
        let gone = moderate(&mut world, VoiceModeration::Disconnect);
        assert!(matches!(&gone[..], [Event::VoiceLeft { channel, .. }] if *channel == rooms[1]));
        assert!(failed(&moderate(&mut world, VoiceModeration::Disconnect)));
        // Not over the owner of a server the account does not own.
        let other = world
            .servers
            .iter()
            .find(|server| server.owner != Some(ME))
            .unwrap();
        let (server, owner) = (other.id, other.owner.unwrap());
        assert!(failed(&world.message_action(
            Command::ModerateVoice {
                server,
                user: owner,
                change: VoiceModeration::Mute(true),
            },
            Timestamp(1),
        )));
    }

    #[test]
    fn links_and_moments_become_references() {
        let found = references(&format!(
            "Mira {LINKS}/7/70/700, o {LINKS}/@me/9. Es <t:1700000000:R> y <t:5>"
        ));
        assert_eq!(
            found[0].target,
            TextTarget::Link {
                server: Some(ServerId(7)),
                channel: ChannelId(70),
                message: Some(MessageId(700)),
            }
        );
        assert_eq!(found[0].source, format!("{LINKS}/7/70/700"));
        assert_eq!(
            found[1].target,
            TextTarget::Link {
                server: None,
                channel: ChannelId(9),
                message: None,
            }
        );
        assert_eq!(
            found[2].target,
            TextTarget::Time {
                at: Timestamp(1_700_000_000_000),
                style: rune_model::TimeStyle::Relative,
            }
        );
        assert_eq!(found[3].source, "<t:5>");
        assert!(references("https://simulado.invalid/channels/x/1 <t:no:R>").is_empty());
    }

    #[test]
    fn pages_follow_the_last_id() {
        let people: Vec<UserId> = (1..=150).map(UserId).collect();
        let [Event::PeopleLoaded {
            people: first,
            reached_end,
            ..
        }] = &page(1, people.clone(), None)[..]
        else {
            panic!()
        };
        assert_eq!((first.len(), *reached_end), (100, false));
        let [Event::PeopleLoaded {
            people: second,
            reached_end,
            ..
        }] = &page(2, people, first.last().copied())[..]
        else {
            panic!()
        };
        assert_eq!((second.len(), *reached_end), (50, true));
        assert_eq!(second[0], UserId(101));
    }
}
