//! Scheduled events in the simulation (eventos-programados). The owned
//! server plans a meeting in its voice room and an outing elsewhere; the
//! first server has a talk happening now on its stage and a workshop next
//! week; the club of the invitation «club» plans a photo walk its link
//! names (`club?event=…`). Everything is fixed against the world's clock;
//! writes check what the account may do as a service would and answer at
//! once, as the service's dispatch would.

use std::collections::HashMap;

use rune_i18n::t;
use rune_model::{
    ChannelKind, EventDraft, EventPlace, EventStatus, Permissions, ScheduledEvent,
    ScheduledEventId, ServerId, Timestamp, UserId,
};

use super::{World, ME};
use crate::{Command, Event, Failure, RequestId};

/// The photo walk the club's invitation names.
pub(super) const CLUB_OUTING: ScheduledEventId = ScheduledEventId(8_000_000_201);
/// The owned server and the first one.
const OWNED: ServerId = ServerId(2);
const FIRST: ServerId = ServerId(1);
const HOUR: i64 = 60 * 60 * 1000;
#[cfg(test)]
const DAY: i64 = 24 * HOUR;

/// What the simulation keeps of every server's events.
#[derive(Debug, Default)]
pub(super) struct EventsSim {
    events: Vec<ScheduledEvent>,
    /// Who each event interests, the account among them when it does.
    interested: HashMap<ScheduledEventId, Vec<UserId>>,
}

impl World {
    /// The events every simulation starts with, at hours from now.
    pub(super) fn add_events(&mut self) {
        let hour = Timestamp((self.now.0 / HOUR + 1) * HOUR);
        let at = |hours: i64| Timestamp(hour.0 + hours * HOUR);
        let room = |world: &World, server: ServerId, kind: ChannelKind| {
            world
                .channels
                .iter()
                .find(|found| found.server == Some(server) && found.kind == kind)
                .map(|found| found.id)
        };
        let person = |index: u64| UserId(10 + index);
        let mut planned = Vec::new();
        if let Some(meeting) = room(self, OWNED, ChannelKind::Voice) {
            planned.push((
                event(
                    8_000_000_101,
                    OWNED,
                    "Revisión del diseño de limn",
                    Some(
                        "Repasamos los componentes nuevos y lo que falta para la próxima versión.",
                    ),
                    at(2 * 24 + 6),
                    Some(at(2 * 24 + 7)),
                    EventPlace::Voice(meeting),
                    ME,
                ),
                vec![ME, person(0), person(4), person(16)],
            ));
        }
        planned.push((
            event(
                8_000_000_102,
                OWNED,
                "Asado de fin de año",
                None,
                at(9 * 24 + 8),
                Some(at(9 * 24 + 12)),
                EventPlace::External("Parque Centenario, Buenos Aires".into()),
                person(16),
            ),
            vec![person(16), person(1), person(11)],
        ));
        if let Some(stage) = room(self, FIRST, ChannelKind::Stage) {
            planned.push((
                ScheduledEvent {
                    status: EventStatus::Active,
                    ..event(
                        8_000_000_103,
                        FIRST,
                        "Charla: async en Rust",
                        Some("Cómo pensar los futuros sin perderse en los lifetimes."),
                        at(-1),
                        None,
                        EventPlace::Stage(stage),
                        person(0),
                    )
                },
                (0..12).map(person).collect(),
            ));
        }
        if let Some(common) = room(self, FIRST, ChannelKind::Voice) {
            planned.push((
                event(
                    8_000_000_104,
                    FIRST,
                    "Taller de macros",
                    None,
                    at(6 * 24 + 4),
                    Some(at(6 * 24 + 6)),
                    EventPlace::Voice(common),
                    person(7),
                ),
                vec![person(7), person(3), person(9)],
            ));
        }
        planned.push((
            event(
                CLUB_OUTING.0,
                super::invitations::SERVER,
                "Salida fotográfica",
                Some("Caminata con cámara por la costa; llevá agua."),
                at(3 * 24 + 2),
                Some(at(3 * 24 + 5)),
                EventPlace::External("Reserva Ecológica Costanera Sur".into()),
                person(13),
            ),
            vec![person(13), person(5)],
        ));
        for (planned, people) in planned {
            self.events.interested.insert(planned.id, people);
            self.events.events.push(planned);
        }
    }

    /// `event` as the account sees it: how many it interests, and whether
    /// the account is one of them.
    fn shown_event(&self, event: &ScheduledEvent) -> ScheduledEvent {
        let people = self.events.interested.get(&event.id);
        ScheduledEvent {
            interested: Some(people.map_or(0, |people| people.len() as u64)),
            account_interested: people.is_some_and(|people| people.contains(&ME)),
            ..event.clone()
        }
    }

    /// The event `id` of `server`, for an invitation that names it.
    pub(super) fn invitation_event(
        &self,
        server: ServerId,
        id: ScheduledEventId,
    ) -> Option<ScheduledEvent> {
        self.events
            .events
            .iter()
            .find(|known| known.id == id && known.server == server)
            .map(|known| self.shown_event(known))
    }

    /// Carry out a command about events and report what changed, or why not.
    pub(super) fn scheduled_events(&mut self, command: Command) -> Vec<Event> {
        let failure = Failure::of(&command).unwrap_or(Failure::Other);
        let outcome = match command {
            Command::LoadScheduledEvents { request, server } => self.mine(server).map(|_| {
                vec![Event::ScheduledEvents {
                    request,
                    server,
                    events: self
                        .events
                        .events
                        .iter()
                        .filter(|known| known.server == server)
                        .map(|known| self.shown_event(known))
                        .collect(),
                }]
            }),
            Command::LoadEventInterested {
                request,
                server,
                event,
            } => self.event_of(server, event).map(|_| {
                let people = self.events.interested.get(&event).cloned();
                vec![Event::EventInterested {
                    request,
                    server,
                    event,
                    users: people
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|user| self.users.iter().find(|known| known.id == user))
                        .cloned()
                        .collect(),
                }]
            }),
            Command::SetEventInterest {
                request,
                server,
                event,
                interested,
            } => self.set_interest(request, server, event, interested),
            Command::CreateScheduledEvent {
                request,
                server,
                draft,
            } => self.create_event(request, server, draft),
            Command::EditScheduledEvent {
                request,
                server,
                event,
                draft,
            } => self.edit_event(request, server, event, Some(draft), None),
            Command::SetScheduledEventStatus {
                request,
                server,
                event,
                status,
            } => self.edit_event(request, server, event, None, Some(status)),
            Command::DeleteScheduledEvent {
                request,
                server,
                event,
            } => self.event_may_change(server, event).map(|_| {
                self.events.events.retain(|known| known.id != event);
                self.events.interested.remove(&event);
                vec![Event::ScheduledEventDeleted {
                    request: Some(request),
                    server,
                    event,
                }]
            }),
            _ => Err(t("la simulación no hace eso").into()),
        };
        outcome.unwrap_or_else(|reason| {
            vec![Event::Failed {
                failure,
                reason,
                ambiguous: false,
            }]
        })
    }

    /// The event `event` of `server`, which the account is in.
    fn event_of(
        &self,
        server: ServerId,
        event: ScheduledEventId,
    ) -> Result<&ScheduledEvent, String> {
        self.mine(server)?;
        self.events
            .events
            .iter()
            .find(|known| known.id == event && known.server == server)
            .ok_or_else(|| t("el evento ya no existe").to_owned())
    }

    /// Whether the account may change `event`: anyone's with «Gestionar
    /// eventos», its own with «Crear eventos».
    fn event_may_change(&self, server: ServerId, event: ScheduledEventId) -> Result<(), String> {
        let creator = self.event_of(server, event)?.creator;
        let mine = self.mine(server)?;
        if mine.contains(Permissions::MANAGE_EVENTS)
            || (mine.contains(Permissions::CREATE_EVENTS) && creator == Some(ME))
        {
            Ok(())
        } else {
            Err(t("no tenés permiso").into())
        }
    }

    /// Whether `draft` may go to `server`: a place that is one of its
    /// stages or voice channels, or written, and times that make sense.
    fn check_draft(
        &self,
        server: ServerId,
        draft: &EventDraft,
        starts: bool,
    ) -> Result<(), String> {
        if let Some(problem) = draft.problem(self.now, starts) {
            return Err(problem.into());
        }
        let wanted = match draft.place {
            EventPlace::Stage(_) => Some(ChannelKind::Stage),
            EventPlace::Voice(_) => Some(ChannelKind::Voice),
            EventPlace::External(_) => None,
        };
        if let (Some(kind), Some(channel)) = (wanted, draft.place.channel()) {
            let found = self.channel(channel);
            if !found.is_some_and(|found| found.server == Some(server) && found.kind == kind) {
                return Err(t("ese canal ya no está en el servidor").into());
            }
        }
        Ok(())
    }

    fn create_event(
        &mut self,
        request: RequestId,
        server: ServerId,
        draft: EventDraft,
    ) -> Result<Vec<Event>, String> {
        if !self
            .mine(server)?
            .intersects(Permissions::CREATE_EVENTS | Permissions::MANAGE_EVENTS)
        {
            return Err(t("no tenés permiso").into());
        }
        self.check_draft(server, &draft, true)?;
        let id = ScheduledEventId(self.fresh_id());
        let created = event(
            id.0,
            server,
            draft.name.trim(),
            None,
            draft.start,
            draft.end,
            draft.place,
            ME,
        );
        let created = ScheduledEvent {
            description: draft.description,
            ..created
        };
        // Who plans it is interested, as the service counts them.
        self.events.interested.insert(id, vec![ME]);
        self.events.events.push(created.clone());
        Ok(vec![Event::ScheduledEventChanged {
            request: Some(request),
            event: self.shown_event(&created),
        }])
    }

    fn edit_event(
        &mut self,
        request: RequestId,
        server: ServerId,
        event: ScheduledEventId,
        draft: Option<EventDraft>,
        status: Option<EventStatus>,
    ) -> Result<Vec<Event>, String> {
        self.event_may_change(server, event)?;
        let current = self.event_of(server, event)?.clone();
        if current.status.ended() {
            return Err(t("el evento ya terminó").into());
        }
        if let Some(draft) = &draft {
            self.check_draft(server, draft, current.status == EventStatus::Scheduled)?;
        }
        let status = match (current.status, status) {
            (_, None) => current.status,
            (EventStatus::Scheduled, Some(EventStatus::Active))
            | (EventStatus::Active, Some(EventStatus::Completed)) => {
                status.unwrap_or(current.status)
            }
            _ => return Err(t("el evento no puede pasar a ese estado").into()),
        };
        let mut changed = current;
        if let Some(draft) = draft {
            changed.name = draft.name.trim().to_owned();
            changed.description = draft.description;
            changed.start = draft.start;
            changed.end = draft.end;
            changed.place = draft.place;
        }
        changed.status = status;
        if let Some(kept) = self
            .events
            .events
            .iter_mut()
            .find(|known| known.id == event)
        {
            *kept = changed.clone();
        }
        Ok(vec![Event::ScheduledEventChanged {
            request: Some(request),
            event: self.shown_event(&changed),
        }])
    }

    fn set_interest(
        &mut self,
        request: RequestId,
        server: ServerId,
        event: ScheduledEventId,
        interested: bool,
    ) -> Result<Vec<Event>, String> {
        if self.event_of(server, event)?.status.ended() {
            return Err(t("el evento ya terminó").into());
        }
        let people = self.events.interested.entry(event).or_default();
        people.retain(|user| *user != ME);
        if interested {
            people.push(ME);
        }
        Ok(vec![Event::EventInterest {
            request: Some(request),
            server,
            event,
            user: ME,
            interested,
        }])
    }
}

/// A planned event, nobody's interest counted yet.
#[allow(clippy::too_many_arguments)]
fn event(
    id: u64,
    server: ServerId,
    name: &str,
    description: Option<&str>,
    start: Timestamp,
    end: Option<Timestamp>,
    place: EventPlace,
    creator: UserId,
) -> ScheduledEvent {
    ScheduledEvent {
        id: ScheduledEventId(id),
        server,
        name: name.into(),
        description: description.map(str::to_owned),
        start,
        end,
        place,
        status: EventStatus::Scheduled,
        image: None,
        creator: Some(creator),
        interested: None,
        account_interested: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockConfig, MockProvider};

    fn events(provider: &MockProvider, server: ServerId) -> Vec<ScheduledEvent> {
        match &provider.manage(Command::LoadScheduledEvents { request: 1, server })[..] {
            [Event::ScheduledEvents { events, .. }] => events.clone(),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn the_owned_server_plans_changes_and_deletes_its_events() {
        let provider = MockProvider::new(MockConfig::default());
        let now = provider.world.lock().unwrap().now;
        let listed = events(&provider, OWNED);
        assert_eq!(listed.len(), 2);
        let meeting = &listed[0];
        assert!(meeting.account_interested && meeting.interested == Some(4));
        assert!(matches!(meeting.place, EventPlace::Voice(_)));

        let draft = EventDraft {
            name: " Hackatón ".into(),
            description: Some("Un día entero".into()),
            start: Timestamp(now.0 + DAY),
            end: Some(Timestamp(now.0 + DAY + HOUR)),
            place: EventPlace::External("Sede".into()),
        };
        let created = match &provider.manage(Command::CreateScheduledEvent {
            request: 2,
            server: OWNED,
            draft: draft.clone(),
        })[..]
        {
            [Event::ScheduledEventChanged {
                request: Some(2),
                event,
            }] => event.clone(),
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(created.name, "Hackatón");
        assert_eq!(created.creator, Some(ME));
        assert_eq!(events(&provider, OWNED).len(), 3);

        // A start in the past is refused, and nothing changes.
        let late = provider.manage(Command::EditScheduledEvent {
            request: 3,
            server: OWNED,
            event: created.id,
            draft: EventDraft {
                start: Timestamp(now.0 - HOUR),
                ..draft.clone()
            },
        });
        assert!(matches!(
            &late[..],
            [Event::Failed {
                failure: Failure::ScheduledEvent { request: 3, .. },
                ..
            }]
        ));
        let started = provider.manage(Command::SetScheduledEventStatus {
            request: 4,
            server: OWNED,
            event: created.id,
            status: EventStatus::Active,
        });
        assert!(matches!(
            &started[..],
            [Event::ScheduledEventChanged { event, .. }] if event.status == EventStatus::Active
        ));
        let deleted = provider.manage(Command::DeleteScheduledEvent {
            request: 5,
            server: OWNED,
            event: created.id,
        });
        assert!(matches!(
            &deleted[..],
            [Event::ScheduledEventDeleted {
                request: Some(5),
                ..
            }]
        ));
        assert_eq!(events(&provider, OWNED).len(), 2);
    }

    #[test]
    fn interest_is_said_once_and_others_events_are_not_the_accounts_to_change() {
        let provider = MockProvider::new(MockConfig::default());
        let talk = events(&provider, FIRST)
            .into_iter()
            .find(|event| event.status == EventStatus::Active)
            .expect("a talk on the stage");
        assert!(!talk.account_interested);
        let said = provider.manage(Command::SetEventInterest {
            request: 6,
            server: FIRST,
            event: talk.id,
            interested: true,
        });
        assert!(matches!(
            &said[..],
            [Event::EventInterest {
                user: ME,
                interested: true,
                ..
            }]
        ));
        let again = events(&provider, FIRST);
        let talk = again.iter().find(|event| event.id == talk.id).unwrap();
        assert!(talk.account_interested && talk.interested == Some(13));
        let people = provider.manage(Command::LoadEventInterested {
            request: 7,
            server: FIRST,
            event: talk.id,
        });
        assert!(matches!(
            &people[..],
            [Event::EventInterested { users, .. }] if users.len() == 13
        ));
        // Someone else's event of a server the account does not manage.
        let refused = provider.manage(Command::DeleteScheduledEvent {
            request: 8,
            server: FIRST,
            event: talk.id,
        });
        assert!(matches!(&refused[..], [Event::Failed { .. }]));
        // The club's walk is not the account's to read before it joins.
        let outside = provider.manage(Command::LoadScheduledEvents {
            request: 9,
            server: super::super::invitations::SERVER,
        });
        assert!(matches!(&outside[..], [Event::Failed { .. }]));
    }

    #[test]
    fn the_clubs_link_names_its_walk() {
        use crate::Provider;
        let provider = MockProvider::new(MockConfig::default());
        let link = super::super::invitations::LINKS;
        let preview = |tail: &str| {
            let target = provider.invitation_target(&format!("{link}{tail}"))?;
            provider.world.lock().unwrap().invitation(&target).ok()
        };
        let walk = preview(&format!("club?event={CLUB_OUTING}")).expect("the club's link");
        assert_eq!(walk.event.map(|event| event.id), Some(CLUB_OUTING));
        assert!(preview("club").expect("a plain link").event.is_none());
        // Another server's event, or one that is not there, is not shown.
        assert!(preview("club?event=8000000101").unwrap().event.is_none());
        assert!(preview("club?event=uno").is_none());
    }
}
