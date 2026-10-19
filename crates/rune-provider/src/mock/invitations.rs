//! Invitations in the simulation: a fixed link of each kind the interface
//! knows (a server, a guest's way into a voice channel, a group and a
//! friendship), read without any service. Accepting one answers at once
//! with what a service would then report: the server, the guest's one voice
//! channel, the group or the friendship, and the acknowledgement.

use std::collections::HashMap;

use rune_i18n::t;
use rune_model::{
    Channel, ChannelId, ChannelKind, Member, Permissions, Relationship, RelationshipKind, Role,
    ServerId, TextReference, TextTarget, UserId,
};

use super::{ChannelSim, World, ME};
use crate::{
    Event, InvitationDestination, InvitationError, InvitationJoinError, InvitationPreview,
    InvitationTarget, RequestId,
};

/// Where the simulation's invitation links start; their code follows.
pub(super) const LINKS: &str = "https://simulado.invalid/invite/";

/// The server, the guest's server and its voice channel, and the group the
/// links lead to: fixed, far from the ids the world makes.
pub(super) const SERVER: ServerId = ServerId(8_000_000_001);
const RADIO: ServerId = ServerId(8_000_000_002);
const RADIO_VOICE: ChannelId = ChannelId(8_000_000_003);
const GROUP: ChannelId = ChannelId(8_000_000_004);
/// The darkroom: a server whose rules a moderator reviews (2026-10-18).
pub(super) const DARKROOM: ServerId = ServerId(8_000_000_005);
/// Who offers friendship: someone the account starts not knowing.
const FRIEND: UserId = UserId(10 + 5);
/// A server whose invitation is always refused, as a service refuses an
/// account banned from it: the simulation's refusal (2026-10-19).
const CLOSED: ServerId = ServerId(8_000_000_006);

/// Each link's code and where it leads; an event it names
/// (`{code}?event={id}`, eventos-programados) leads there too.
fn destination(key: &str) -> Option<InvitationDestination> {
    let code = key.split('?').next().unwrap_or(key);
    Some(match code {
        "club" => InvitationDestination::Server(SERVER),
        "revelado" => InvitationDestination::Server(DARKROOM),
        "radio" => InvitationDestination::Guest {
            server: RADIO,
            channel: RADIO_VOICE,
        },
        "jueves" => InvitationDestination::Group(GROUP),
        "amistad" => InvitationDestination::Friend(FRIEND),
        "cerrado" => InvitationDestination::Server(CLOSED),
        _ => return None,
    })
}

/// A link of the simulation's own, whatever its code: an unknown code is
/// an invitation that no longer works.
pub(super) fn target(url: &str) -> Option<InvitationTarget> {
    let key = url.strip_prefix(LINKS)?;
    // Scheduled events (eventos-programados): `?event={id}` names one.
    let (code, event) = match key.split_once("?event=") {
        Some((code, event)) => (code, Some(event)),
        None => (key, None),
    };
    let plain = |text: &str, max: usize, digits: bool| {
        !text.is_empty()
            && text.len() <= max
            && text.bytes().all(|b| {
                if digits {
                    b.is_ascii_digit()
                } else {
                    b.is_ascii_alphanumeric()
                }
            })
    };
    (plain(code, 32, false) && event.is_none_or(|event| plain(event, 20, true))).then(|| {
        InvitationTarget {
            key: key.into(),
            url: url.into(),
        }
    })
}

/// The event an invitation's key names, if it names one.
fn named_event(key: &str) -> Option<rune_model::ScheduledEventId> {
    let (_, event) = key.split_once("?event=")?;
    event.parse().ok().map(rune_model::ScheduledEventId)
}

/// The invitation links in `content`, as the adapter of a service marks them.
pub(super) fn references(content: &str) -> Vec<TextReference> {
    let mut found: Vec<TextReference> = Vec::new();
    for word in content.split_whitespace() {
        let word = word.trim_end_matches(['.', ',', ')', ';', ':']);
        if target(word).is_some() && !found.iter().any(|known| known.source == word) {
            found.push(TextReference {
                source: word.into(),
                target: TextTarget::Invitation,
            });
        }
    }
    found
}

/// A channel of a server the account joins by invitation.
fn place(server: ServerId, id: ChannelId, kind: ChannelKind, name: &str) -> Channel {
    Channel {
        id,
        server: Some(server),
        kind,
        name: name.into(),
        topic: None,
        topic_pictures: Vec::new(),
        parent: None,
        position: 0,
        recipients: Vec::new(),
        owner: None,
        permissions: Permissions::NONE,
        overwrites: Vec::new(),
        slowmode_secs: 0,
        nsfw: false,
        icon: None,
        thread: None,
        tags: Vec::new(),
        forum: Default::default(),
    }
}

impl World {
    /// What the simulated invitation `target` shows.
    pub(super) fn invitation(
        &self,
        target: &InvitationTarget,
    ) -> Result<InvitationPreview, InvitationError> {
        let destination = destination(&target.key).ok_or(InvitationError::Invalid)?;
        let simulated = t("(simulado)");
        let preview = |name: String, members: Option<u64>, online: Option<u64>| InvitationPreview {
            destination,
            name,
            description: None,
            image: None,
            members,
            online,
            expires_at: None,
            // Scheduled events (eventos-programados).
            event: destination
                .server()
                .zip(named_event(&target.key))
                .and_then(|(server, event)| self.invitation_event(server, event)),
        };
        Ok(match destination {
            InvitationDestination::Server(CLOSED) => {
                preview(format!("Sala cerrada {simulated}"), Some(12), Some(1))
            }
            InvitationDestination::Server(DARKROOM) => InvitationPreview {
                description: Some(t("Un taller que revisa cada solicitud.").into()),
                ..preview(format!("Cuarto oscuro {simulated}"), Some(36), Some(4))
            },
            InvitationDestination::Server(_) => InvitationPreview {
                description: Some(t("Un club abierto para probar invitaciones.").into()),
                ..preview(format!("Club abierto {simulated}"), Some(128), Some(17))
            },
            InvitationDestination::Guest { .. } => {
                preview(format!("Radio RUNE {simulated}"), Some(40), Some(6))
            }
            InvitationDestination::Group(_) => {
                preview(format!("Los jueves {simulated}"), Some(4), None)
            }
            InvitationDestination::Friend(user) => preview(
                self.users
                    .iter()
                    .find(|known| known.id == user)
                    .map_or_else(String::new, |known| known.shown_name().to_owned()),
                None,
                None,
            ),
        })
    }

    /// Accept the invitation leading to `destination`, as a service would:
    /// what it joins first, then the acknowledgement.
    pub(super) fn accept_invitation(
        &mut self,
        request: RequestId,
        target: &InvitationTarget,
        destination: InvitationDestination,
    ) -> Vec<Event> {
        let answer = |result| Event::InvitationAcceptance {
            request,
            destination,
            result,
        };
        if self.invitation(target).map(|preview| preview.destination) != Ok(destination) {
            return vec![answer(Err(InvitationJoinError::Invalid))];
        }
        let joined = match destination {
            InvitationDestination::Server(server) | InvitationDestination::Guest { server, .. } => {
                self.servers.iter().any(|known| known.id == server)
            }
            InvitationDestination::Group(channel) => self.channel(channel).is_some(),
            InvitationDestination::Friend(user) => self
                .relationships
                .iter()
                .any(|known| known.user == user && known.kind == RelationshipKind::Friend),
        };
        if joined {
            return vec![answer(Err(InvitationJoinError::AlreadyJoined))];
        }
        if destination == InvitationDestination::Server(CLOSED) {
            return vec![answer(Err(InvitationJoinError::Banned))];
        }
        let mut events = match destination {
            InvitationDestination::Server(server) => {
                let (text, voice) = (ChannelId(self.fresh_id()), ChannelId(self.fresh_id()));
                let darkroom = server == DARKROOM;
                let mut joined = self.join_server(
                    server,
                    if darkroom {
                        "Cuarto oscuro"
                    } else {
                        "Club abierto"
                    },
                    vec![
                        place(server, text, ChannelKind::Text, "general"),
                        place(server, voice, ChannelKind::Voice, "General"),
                    ],
                );
                // Rules and onboarding (2026-10-18): the club asks both;
                // the darkroom, rules its moderators review.
                joined.extend(if darkroom {
                    self.screen_joined(server)
                } else {
                    self.gate_joined(server)
                });
                joined
            }
            // A guest sees only the voice channel it was let into, and is
            // never asked the onboarding the radio has (2026-10-18).
            InvitationDestination::Guest { server, channel } => {
                let mut joined = self.join_server(
                    server,
                    "Radio RUNE",
                    vec![place(server, channel, ChannelKind::Voice, "Al aire")],
                );
                joined.extend(self.guest_joined(server));
                joined
            }
            InvitationDestination::Group(id) => {
                let others: Vec<UserId> = self
                    .users
                    .iter()
                    .map(|user| user.id)
                    .filter(|user| *user != ME)
                    .take(3)
                    .collect();
                let group = Channel {
                    server: None,
                    kind: ChannelKind::Group,
                    recipients: others.clone(),
                    owner: others.first().copied(),
                    permissions: Permissions::MEMBER | Permissions::READ_HISTORY,
                    ..place(SERVER, id, ChannelKind::Group, "Los jueves")
                };
                self.channels.push(group.clone());
                self.sims.insert(id, quiet());
                vec![Event::ChannelChanged(group)]
            }
            InvitationDestination::Friend(user) => {
                self.relationships.retain(|known| known.user != user);
                let friendship = Relationship {
                    user,
                    kind: RelationshipKind::Friend,
                    since: Some(self.now),
                    nickname: None,
                    spam: false,
                };
                self.relationships.push(friendship.clone());
                vec![Event::RelationshipChanged(friendship)]
            }
        };
        events.push(answer(Ok(())));
        events
    }

    /// The account in `server`, which has these `channels` and nothing
    /// else, with what it may do in each.
    fn join_server(&mut self, id: ServerId, name: &str, channels: Vec<Channel>) -> Vec<Event> {
        let server = rune_model::Server {
            stickers: Vec::new(),
            emojis: Vec::new(),
            icon: None,
            id,
            name: format!("{name} {}", t("(simulado)")),
            accent: rune_model::Rgb(0x2E_A0_43),
            roles: vec![Role::everyone(id, Permissions::EVERYONE)],
            owner: None,
            description: None,
            options: Default::default(),
        };
        self.servers.push(server.clone());
        let member = Member {
            user: ME,
            server: id,
            nick: None,
            roles: Vec::new(),
            ..Default::default()
        };
        self.members.push(member.clone());
        for channel in channels {
            if channel.kind.has_messages() {
                self.sims.insert(channel.id, quiet());
            }
            self.channels.push(channel);
        }
        self.refresh_permissions(id);
        let mut events = vec![Event::ServerChanged(server), Event::MemberChanged(member)];
        events.extend(
            self.channels
                .iter()
                .filter(|channel| channel.server == Some(id))
                .cloned()
                .map(Event::ChannelChanged),
        );
        events
    }
}

/// The link the simulation always refuses and where it leads, for a
/// capture of a refused acceptance.
pub fn refused_invitation() -> (InvitationTarget, InvitationDestination) {
    let target = target(&format!("{LINKS}cerrado")).expect("a simulated link");
    (target, InvitationDestination::Server(CLOSED))
}

/// A conversation that starts empty.
pub(super) fn quiet() -> ChannelSim {
    ChannelSim {
        generated: 0,
        sitting_gap: 600,
        extra: Vec::new(),
        overlay: HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockConfig, MockProvider};
    use crate::{Command, Provider};

    fn accept(provider: &MockProvider, code: &str) -> (InvitationDestination, Vec<Event>) {
        let target = provider
            .invitation_target(&format!("{LINKS}{code}"))
            .unwrap();
        let destination = provider.world.lock().unwrap().invitation(&target).unwrap();
        let events = provider.manage(Command::AcceptInvitation {
            request: 9,
            target,
            destination: destination.destination,
            account: ME,
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(60),
        });
        (destination.destination, events)
    }

    #[test]
    fn each_kind_of_link_shows_its_destination_and_joins_it_once() {
        let provider = MockProvider::new(MockConfig::default());
        assert!(provider
            .info()
            .capabilities
            .accepts(InvitationDestination::Friend(FRIEND)));
        assert!(provider
            .invitation_target("https://simulado.invalid/x/taller")
            .is_none());
        let gone = provider
            .invitation_target(&format!("{LINKS}nadie"))
            .unwrap();
        assert_eq!(
            provider.world.lock().unwrap().invitation(&gone),
            Err(InvitationError::Invalid)
        );

        let (server, events) = accept(&provider, "club");
        assert!(events
            .iter()
            .any(|event| matches!(event, Event::ServerChanged(found) if Some(found.id) == server.server())));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::ChannelChanged(channel) if channel.kind == ChannelKind::Text
                && channel.permissions.contains(Permissions::VIEW)
        )));
        assert!(matches!(
            events.last(),
            Some(Event::InvitationAcceptance { result: Ok(()), destination, .. }) if *destination == server
        ));
        // Accepting again is refused: the account is in it already.
        let (_, again) = accept(&provider, "club");
        assert!(matches!(
            &again[..],
            [Event::InvitationAcceptance {
                result: Err(InvitationJoinError::AlreadyJoined),
                ..
            }]
        ));
        // The closed room always refuses, and joins nothing.
        let (closed, refused) = accept(&provider, "cerrado");
        assert_eq!(refused_invitation().1, closed);
        assert!(matches!(
            &refused[..],
            [Event::InvitationAcceptance {
                result: Err(InvitationJoinError::Banned),
                ..
            }]
        ));

        let (guest, events) = accept(&provider, "radio");
        let channels: Vec<&Channel> = events
            .iter()
            .filter_map(|event| match event {
                Event::ChannelChanged(channel) => Some(channel),
                _ => None,
            })
            .collect();
        assert_eq!(channels.len(), 1, "a guest sees only its voice channel");
        assert_eq!(
            guest,
            InvitationDestination::Guest {
                server: RADIO,
                channel: channels[0].id
            }
        );

        let (_, events) = accept(&provider, "jueves");
        assert!(events.iter().any(|event| matches!(
            event,
            Event::ChannelChanged(group) if group.id == GROUP && group.kind == ChannelKind::Group
        )));

        let (_, events) = accept(&provider, "amistad");
        assert!(events.iter().any(|event| matches!(
            event,
            Event::RelationshipChanged(friend) if friend.user == FRIEND
                && friend.kind == RelationshipKind::Friend
        )));
    }

    #[test]
    fn links_in_text_are_marked_once_each() {
        let found = references(&format!(
            "Vení {LINKS}club, o al grupo {LINKS}jueves. {LINKS}club https://otro.invalid/invite/x"
        ));
        let sources: Vec<&str> = found.iter().map(|found| found.source.as_str()).collect();
        assert_eq!(
            sources,
            vec![
                format!("{LINKS}club").as_str(),
                format!("{LINKS}jueves").as_str()
            ]
        );
    }
}
