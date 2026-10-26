//! Friends, requests, blocks and conversations outside servers in the
//! simulation. The starting relationships are fixed, not drawn from the
//! world's random stream, so adding them changes nothing else the seed makes.
//! Every write is checked the way a service would and confirmed at once.

use std::collections::HashMap;

use rune_i18n::t;
use rune_model::{
    Channel, ChannelId, ChannelKind, ImageKind, MessageKind, Permissions, Relationship,
    RelationshipKind, RemoteImage, Timestamp, UserId,
};

use super::{ChannelSim, World, ME};
use crate::{Command, Created, Event, GroupIcon, RequestId};

type Outcome = Result<Vec<Event>, String>;

/// Who the account starts knowing, by their place in `PEOPLE`, and since
/// how many days before the history ends.
const STARTING: [(usize, RelationshipKind, i64); 11] = [
    (0, RelationshipKind::Friend, 900),
    (1, RelationshipKind::Friend, 640),
    (2, RelationshipKind::Friend, 420),
    (4, RelationshipKind::Friend, 300),
    (7, RelationshipKind::Friend, 210),
    (11, RelationshipKind::Friend, 120),
    (21, RelationshipKind::Friend, 45),
    (9, RelationshipKind::IncomingRequest, 2),
    (12, RelationshipKind::IncomingRequest, 1),
    (19, RelationshipKind::OutgoingRequest, 3),
    (24, RelationshipKind::Blocked, 30),
];

/// Requests the service filed as likely spam, the same way.
const SPAM: [(usize, i64); 1] = [(16, 4)];

/// A group has the account and up to this many others.
const GROUP_OTHERS: usize = 9;

/// The relationships the simulation starts with.
pub(super) fn starting(history_end: Timestamp) -> Vec<Relationship> {
    let spam = SPAM
        .iter()
        .map(|(index, days)| (*index, RelationshipKind::IncomingRequest, *days, true));
    STARTING
        .iter()
        .map(|(index, kind, days)| (*index, *kind, *days, false))
        .chain(spam)
        .map(|(index, kind, days, spam)| Relationship {
            user: UserId(10 + index as u64),
            kind,
            since: Some(history_end.plus_minutes(-days * 24 * 60)),
            nickname: None,
            spam,
        })
        .collect()
}

impl World {
    /// Carry out a relationship or conversation command, if it is one.
    pub(super) fn social(&mut self, command: Command) -> Option<Outcome> {
        Some(match command {
            Command::SendFriendRequest { username } => self.request_friend(&username),
            Command::AcceptFriendRequest { user } => match self.kind_with(user) {
                Some(RelationshipKind::IncomingRequest) => {
                    Ok(self.relate(user, RelationshipKind::Friend))
                }
                _ => Err(t("no hay una solicitud de esa persona").into()),
            },
            Command::RemoveRelationship { user } => {
                let before = self.relationships.len();
                self.relationships.retain(|known| known.user != user);
                if self.relationships.len() == before {
                    Err(t("no hay ninguna relación con esa persona").into())
                } else {
                    Ok(vec![Event::RelationshipRemoved { user }])
                }
            }
            Command::BlockUser { user } => {
                if user == ME || !self.users.iter().any(|known| known.id == user) {
                    Err(t("esa persona no existe").into())
                } else {
                    Ok(self.relate(user, RelationshipKind::Blocked))
                }
            }
            Command::OpenDirect { request, user } => self.open_direct(request, user),
            Command::RenameGroup { channel, name } => {
                let name = name.trim().to_owned();
                if name.chars().count() > 100 {
                    return Some(Err(t("el nombre puede tener hasta 100 caracteres").into()));
                }
                let said = name.clone();
                self.change_group(channel, |group| {
                    group.name = name;
                    Ok(())
                })
                .map(|events| {
                    self.recorded(events, channel, MessageKind::ChannelRename, None, said)
                })
            }
            Command::AddGroupMember { channel, user } => {
                let friend = self.kind_with(user) == Some(RelationshipKind::Friend);
                self.change_group(channel, |group| {
                    if group.recipients.contains(&user) {
                        Err(t("ya está en el grupo").into())
                    } else if !friend {
                        Err(t("solo puedes añadir a tus amigos").into())
                    } else if group.recipients.len() >= GROUP_OTHERS {
                        Err(t("un grupo tiene hasta 10 personas").into())
                    } else {
                        group.recipients.push(user);
                        Ok(())
                    }
                })
                .map(|events| {
                    self.recorded(
                        events,
                        channel,
                        MessageKind::RecipientAdd,
                        Some(user),
                        String::new(),
                    )
                })
            }
            Command::RemoveGroupMember { channel, user } => self
                .change_group(channel, |group| {
                    if group.owner != Some(ME) {
                        Err(t("solo quien es dueño del grupo puede quitar personas").into())
                    } else if !group.recipients.contains(&user) {
                        Err(t("no está en el grupo").into())
                    } else {
                        group.recipients.retain(|known| *known != user);
                        Ok(())
                    }
                })
                .map(|events| {
                    self.recorded(
                        events,
                        channel,
                        MessageKind::RecipientRemove,
                        Some(user),
                        String::new(),
                    )
                }),
            // No line in the history: the service has no message for it, so
            // the crown moves and that is all.
            Command::SetGroupOwner { channel, user } => self.change_group(channel, |group| {
                if group.owner != Some(ME) {
                    Err(t("solo quien es dueño del grupo puede pasar la propiedad").into())
                } else if user == ME || !group.recipients.contains(&user) {
                    Err(t("esa persona no está en el grupo").into())
                } else {
                    group.owner = Some(user);
                    Ok(())
                }
            }),
            Command::SetGroupIcon { channel, icon } => {
                let version = self.fresh_id();
                self.change_group(channel, |group| {
                    group.icon = icon
                        .as_ref()
                        .map(|icon| simulated_icon(channel, version, icon));
                    Ok(())
                })
                .map(|events| {
                    self.recorded(
                        events,
                        channel,
                        MessageKind::ChannelIcon,
                        None,
                        String::new(),
                    )
                })
            }
            Command::CreateGroup { request, users } => self.create_group(request, users),
            Command::CloseDirect { channel } => {
                let Some(index) = self
                    .channels
                    .iter()
                    .position(|found| found.id == channel && found.kind == ChannelKind::Direct)
                else {
                    return Some(Err(t("solo se cierran conversaciones directas").into()));
                };
                let closed = self.channels.remove(index);
                self.closed.push(closed);
                Ok(vec![Event::ChannelRemoved { channel }])
            }
            Command::SetFriendNickname { user, nickname } => {
                let nickname = nickname
                    .map(|name| name.trim().to_owned())
                    .filter(|name| !name.is_empty());
                match self
                    .relationships
                    .iter_mut()
                    .find(|known| known.user == user)
                {
                    Some(known) if known.kind == RelationshipKind::Friend => {
                        if nickname
                            .as_ref()
                            .is_some_and(|name| name.chars().count() > Relationship::NICKNAME_MAX)
                        {
                            Err(t("el apodo puede tener hasta 32 caracteres").into())
                        } else {
                            known.nickname = nickname;
                            Ok(vec![Event::RelationshipChanged(known.clone())])
                        }
                    }
                    _ => Err(t("solo tus amigos pueden tener un apodo").into()),
                }
            }
            Command::LeaveGroup { channel } => {
                if self.group(channel).is_none() {
                    return Some(Err(t("el grupo no existe").into()));
                }
                self.channels.retain(|known| known.id != channel);
                self.sims.remove(&channel);
                Ok(vec![Event::ChannelRemoved { channel }])
            }
            _ => return None,
        })
    }

    pub(super) fn kind_with(&self, user: UserId) -> Option<RelationshipKind> {
        self.relationships
            .iter()
            .find(|known| known.user == user)
            .map(|known| known.kind)
    }

    /// Set the relationship with `user` to `kind`, replacing any other.
    fn relate(&mut self, user: UserId, kind: RelationshipKind) -> Vec<Event> {
        self.relationships.retain(|known| known.user != user);
        let relationship = Relationship {
            user,
            kind,
            since: Some(self.now),
            nickname: None,
            spam: false,
        };
        self.relationships.push(relationship.clone());
        vec![Event::RelationshipChanged(relationship)]
    }

    fn request_friend(&mut self, username: &str) -> Outcome {
        let handle = username.trim().trim_start_matches('@').to_lowercase();
        let user = self
            .users
            .iter()
            .find(|user| user.name == handle && !handle.is_empty())
            .map(|user| user.id)
            .ok_or(t("no encontramos a nadie con ese nombre de usuario"))?;
        if user == ME {
            return Err(t("no puedes enviarte una solicitud").into());
        }
        match self.kind_with(user) {
            Some(RelationshipKind::Friend) => Err(t("ya son amigos").into()),
            Some(RelationshipKind::OutgoingRequest) => {
                Err(t("ya le enviaste una solicitud").into())
            }
            Some(RelationshipKind::Blocked) => {
                Err(t("bloqueaste a esa persona: desbloquéala primero").into())
            }
            // They had asked first: asking back accepts.
            Some(RelationshipKind::IncomingRequest) => {
                Ok(self.relate(user, RelationshipKind::Friend))
            }
            None => Ok(self.relate(user, RelationshipKind::OutgoingRequest)),
        }
    }

    fn open_direct(&mut self, request: RequestId, user: UserId) -> Outcome {
        // A conversation taken off the list comes back as it was.
        if let Some(index) = self
            .closed
            .iter()
            .position(|channel| channel.recipients == [user])
        {
            let channel = self.closed.remove(index);
            self.channels.push(channel);
        }
        if let Some(found) = self
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Direct && channel.recipients == [user])
        {
            return Ok(vec![
                Event::ChannelChanged(found.clone()),
                Event::Created {
                    request,
                    created: Created::Channel(found.id),
                },
            ]);
        }
        if user == ME || !self.users.iter().any(|known| known.id == user) {
            return Err(t("esa persona no existe").into());
        }
        if self.kind_with(user) == Some(RelationshipKind::Blocked) {
            return Err(t("bloqueaste a esa persona").into());
        }
        Ok(self.start_conversation(request, ChannelKind::Direct, vec![user]))
    }

    fn create_group(&mut self, request: RequestId, users: Vec<UserId>) -> Outcome {
        let mut people: Vec<UserId> = Vec::new();
        for user in users {
            if !people.contains(&user) {
                people.push(user);
            }
        }
        if people.len() < 2 {
            return Err(t("elige al menos a dos amigos").into());
        }
        if people.len() > GROUP_OTHERS {
            return Err(t("un grupo tiene hasta 10 personas").into());
        }
        if people
            .iter()
            .any(|user| self.kind_with(*user) != Some(RelationshipKind::Friend))
        {
            return Err(t("solo puedes añadir a tus amigos").into());
        }
        Ok(self.start_conversation(request, ChannelKind::Group, people))
    }

    /// A new conversation outside servers with `recipients`, the account
    /// owning it when it is a group.
    fn start_conversation(
        &mut self,
        request: RequestId,
        kind: ChannelKind,
        recipients: Vec<UserId>,
    ) -> Vec<Event> {
        let id = ChannelId(self.fresh_id());
        let channel = Channel {
            id,
            server: None,
            kind,
            name: String::new(),
            topic: None,
            topic_pictures: Vec::new(),
            parent: None,
            position: 0,
            recipients,
            thread: None,
            tags: Vec::new(),
            forum: Default::default(),
            owner: (kind == ChannelKind::Group).then_some(ME),
            permissions: Permissions::MEMBER | Permissions::READ_HISTORY,
            overwrites: Vec::new(),
            slowmode_secs: 0,
            nsfw: false,
            icon: None,
        };
        self.channels.push(channel.clone());
        self.sims.insert(
            id,
            ChannelSim {
                generated: 0,
                sitting_gap: 600,
                extra: Vec::new(),
                overlay: HashMap::new(),
            },
        );
        vec![
            Event::ChannelChanged(channel),
            Event::Created {
                request,
                created: Created::Channel(id),
            },
        ]
    }

    /// `events`, followed by the line the group's history keeps of the
    /// change: who it was about, or the new name.
    fn recorded(
        &mut self,
        mut events: Vec<Event>,
        channel: ChannelId,
        kind: MessageKind,
        about: Option<UserId>,
        content: String,
    ) -> Vec<Event> {
        if let Some(mut message) = self.post(channel, ME, content, None, self.now) {
            message.kind = kind;
            message.mentions = about.into_iter().collect();
            if let Some(kept) = self
                .sims
                .get_mut(&channel)
                .and_then(|sim| sim.extra.last_mut())
            {
                *kept = message.clone();
            }
            events.push(Event::MessageCreated {
                message,
                nonce: None,
            });
        }
        events
    }

    /// A closed direct conversation, back on the list because its person
    /// writes again: its ID, who writes, and the event that lists it.
    pub(super) fn reopen_closed(&mut self, pick: u64) -> Option<(ChannelId, UserId, Event)> {
        if self.closed.is_empty() {
            return None;
        }
        let channel = self.closed.remove(pick as usize % self.closed.len());
        let (id, author) = (channel.id, *channel.recipients.first()?);
        self.channels.push(channel.clone());
        Some((id, author, Event::ChannelChanged(channel)))
    }

    fn group(&self, channel: ChannelId) -> Option<&Channel> {
        self.channel(channel)
            .filter(|found| found.kind == ChannelKind::Group)
    }

    fn change_group(
        &mut self,
        channel: ChannelId,
        change: impl FnOnce(&mut Channel) -> Result<(), String>,
    ) -> Outcome {
        let group = self
            .channels
            .iter_mut()
            .find(|found| found.id == channel && found.kind == ChannelKind::Group)
            .ok_or(t("el grupo no existe"))?;
        let mut changed = group.clone();
        change(&mut changed)?;
        *group = changed.clone();
        Ok(vec![Event::ChannelChanged(changed)])
    }
}

/// The simulation keeps no pictures: a group's icon is a name that never
/// loads, so the interface keeps its usual mark and says why.
fn simulated_icon(channel: ChannelId, version: u64, icon: &GroupIcon) -> RemoteImage {
    RemoteImage {
        url: format!(
            "https://simulacion.invalid/grupos/{channel}/{version}-{}.png",
            icon.png().len()
        ),
        kind: ImageKind::Artwork,
    }
}

#[cfg(test)]
mod tests {
    use rune_model::RelationshipKind;

    use crate::mock::{MockConfig, MockProvider};
    use crate::{Command, Created, Event, Failure};

    fn provider() -> MockProvider {
        MockProvider::new(MockConfig {
            seed: 11,
            history_end: rune_model::Timestamp(1_790_000_000_000),
            clock: || rune_model::Timestamp(1_790_000_100_000),
            latency: std::time::Duration::ZERO,
            ambient: false,
            replies: false,
        })
    }

    #[test]
    fn the_simulation_starts_with_friends_requests_both_ways_and_a_block() {
        let relationships = provider().snapshot().relationships.unwrap();
        for kind in [
            RelationshipKind::Friend,
            RelationshipKind::IncomingRequest,
            RelationshipKind::OutgoingRequest,
            RelationshipKind::Blocked,
        ] {
            assert!(relationships.iter().any(|known| known.kind == kind));
        }
    }

    #[test]
    fn opening_a_direct_conversation_twice_names_the_same_channel() {
        let provider = provider();
        // Zoe has no conversation with the account yet.
        let zoe = rune_model::UserId(35);
        let created = |events: Vec<Event>| {
            events.iter().find_map(|event| match event {
                Event::Created {
                    created: Created::Channel(channel),
                    ..
                } => Some(*channel),
                _ => None,
            })
        };
        let first = created(provider.manage(Command::OpenDirect {
            request: 1,
            user: zoe,
        }))
        .unwrap();
        let again = created(provider.manage(Command::OpenDirect {
            request: 2,
            user: zoe,
        }))
        .unwrap();
        assert_eq!(first, again);
    }

    #[test]
    fn a_request_by_handle_waits_and_one_they_sent_is_accepted() {
        let provider = provider();
        let sent = provider.manage(Command::SendFriendRequest {
            username: "@Zoe ".into(),
        });
        assert!(matches!(&sent[..], [Event::RelationshipChanged(r)]
            if r.kind == RelationshipKind::OutgoingRequest));
        let refused = provider.manage(Command::SendFriendRequest {
            username: "nadie".into(),
        });
        assert!(matches!(
            &refused[..],
            [Event::Failed {
                failure: Failure::Relationship { user: None },
                ..
            }]
        ));
        // Julián asked first.
        let accepted = provider.manage(Command::AcceptFriendRequest {
            user: rune_model::UserId(19),
        });
        assert!(matches!(&accepted[..], [Event::RelationshipChanged(r)]
            if r.kind == RelationshipKind::Friend));
    }

    fn created(events: &[Event]) -> Option<rune_model::ChannelId> {
        events.iter().find_map(|event| match event {
            Event::Created {
                created: Created::Channel(channel),
                ..
            } => Some(*channel),
            _ => None,
        })
    }

    fn system(events: &[Event]) -> Option<(rune_model::MessageKind, Vec<UserId>, String)> {
        events.iter().find_map(|event| match event {
            Event::MessageCreated { message, .. } => Some((
                message.kind,
                message.mentions.clone(),
                message.content.clone(),
            )),
            _ => None,
        })
    }

    use rune_model::{MessageKind, UserId};

    #[test]
    fn a_group_is_made_with_two_friends_or_more_and_records_its_changes() {
        let provider = provider();
        // Ada and Bruno are friends; Julián only asked.
        let (ada, bruno, julian) = (UserId(10), UserId(11), UserId(19));
        let refused = provider.manage(Command::CreateGroup {
            request: 1,
            users: vec![ada, julian],
        });
        assert!(matches!(
            &refused[..],
            [Event::Failed {
                failure: Failure::NewGroup { request: 1 },
                ..
            }]
        ));
        let lonely = provider.manage(Command::CreateGroup {
            request: 2,
            users: vec![ada, ada],
        });
        assert!(matches!(&lonely[..], [Event::Failed { .. }]));
        let made = provider.manage(Command::CreateGroup {
            request: 3,
            users: vec![ada, bruno],
        });
        let group = created(&made).unwrap();
        assert!(matches!(&made[0], Event::ChannelChanged(channel)
            if channel.id == group && channel.owner == Some(crate::mock::ME)
                && channel.recipients == [ada, bruno]));
        let renamed = provider.manage(Command::RenameGroup {
            channel: group,
            name: " Asado ".into(),
        });
        assert_eq!(
            system(&renamed),
            Some((MessageKind::ChannelRename, Vec::new(), "Asado".into()))
        );
        let carla = UserId(12);
        provider.manage(Command::AcceptFriendRequest { user: carla });
        let added = provider.manage(Command::AddGroupMember {
            channel: group,
            user: carla,
        });
        assert_eq!(
            system(&added),
            Some((MessageKind::RecipientAdd, vec![carla], String::new()))
        );
        let removed = provider.manage(Command::RemoveGroupMember {
            channel: group,
            user: carla,
        });
        assert_eq!(system(&removed).unwrap().0, MessageKind::RecipientRemove);
        let png = bytes::Bytes::from_static(
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x40\0\0\0\x40\x08\x06\0\0\0",
        );
        let icon = provider.manage(Command::SetGroupIcon {
            channel: group,
            icon: crate::GroupIcon::new(png),
        });
        assert!(matches!(&icon[0], Event::ChannelChanged(channel) if channel.icon.is_some()));
        assert_eq!(system(&icon).unwrap().0, MessageKind::ChannelIcon);
        // The history keeps those lines too.
        let (history, _) = provider.history(group, None, 50);
        assert_eq!(history.len(), 4);
        // Handed over: the crown moves, and the old owner can no longer.
        let handed = provider.manage(Command::SetGroupOwner {
            channel: group,
            user: ada,
        });
        assert!(matches!(&handed[..], [Event::ChannelChanged(channel)]
            if channel.owner == Some(ada)));
        let again = provider.manage(Command::SetGroupOwner {
            channel: group,
            user: bruno,
        });
        assert!(matches!(&again[..], [Event::Failed { .. }]));
    }

    #[test]
    fn a_closed_direct_conversation_comes_back_as_it_was() {
        let provider = provider();
        let first = provider
            .snapshot()
            .channels
            .into_iter()
            .find(|channel| channel.kind == rune_model::ChannelKind::Direct);
        let channel = first.unwrap();
        let closed = provider.manage(Command::CloseDirect {
            channel: channel.id,
        });
        assert_eq!(
            closed,
            vec![Event::ChannelRemoved {
                channel: channel.id
            }]
        );
        assert!(!provider
            .snapshot()
            .channels
            .iter()
            .any(|known| known.id == channel.id));
        // Opening it again names the same conversation, history and all.
        let opened = provider.manage(Command::OpenDirect {
            request: 4,
            user: channel.recipients[0],
        });
        assert_eq!(created(&opened), Some(channel.id));
        // A group is left, not closed.
        let group = provider
            .snapshot()
            .channels
            .into_iter()
            .find(|known| known.kind == rune_model::ChannelKind::Group)
            .unwrap();
        assert!(matches!(
            &provider.manage(Command::CloseDirect { channel: group.id })[..],
            [Event::Failed {
                failure: Failure::Close { .. },
                ..
            }]
        ));
        // Written in after being closed, it is listed before the message.
        provider.manage(Command::CloseDirect {
            channel: channel.id,
        });
        let mut world = provider.world.lock().unwrap();
        let (back, author, listed) = world.reopen_closed(0).unwrap();
        assert_eq!((back, author), (channel.id, channel.recipients[0]));
        assert!(matches!(listed, Event::ChannelChanged(found) if found.id == channel.id));
        assert!(world.reopen_closed(0).is_none());
    }

    #[test]
    fn friends_get_a_nickname_and_one_request_waits_as_spam() {
        let provider = provider();
        let named = provider.manage(Command::SetFriendNickname {
            user: UserId(10),
            nickname: Some("  Adita ".into()),
        });
        assert!(matches!(&named[..], [Event::RelationshipChanged(r)]
            if r.nickname.as_deref() == Some("Adita")));
        let cleared = provider.manage(Command::SetFriendNickname {
            user: UserId(10),
            nickname: Some(" ".into()),
        });
        assert!(matches!(&cleared[..], [Event::RelationshipChanged(r)] if r.nickname.is_none()));
        let refused = provider.manage(Command::SetFriendNickname {
            user: UserId(19),
            nickname: Some("x".into()),
        });
        assert!(matches!(
            &refused[..],
            [Event::Failed {
                failure: Failure::Relationship { user: Some(_) },
                ..
            }]
        ));
        let spam: Vec<_> = provider
            .snapshot()
            .relationships
            .unwrap()
            .into_iter()
            .filter(|known| known.spam)
            .collect();
        assert_eq!(spam.len(), 1);
        assert_eq!(spam[0].kind, RelationshipKind::IncomingRequest);
        assert!(!spam[0].awaits_answer());
    }
}
