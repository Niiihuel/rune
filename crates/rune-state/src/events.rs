//! Scheduled events (eventos-programados): each server's events as the
//! provider sent them, kept current by what it reports, who an event
//! interests once read, and what the account may do with them. Nothing is
//! shown ahead of an answer: a change counts once the provider says so.

use std::collections::HashMap;

use rune_i18n::{t, tr};
use rune_model::{Permissions, ScheduledEvent, ScheduledEventId, ServerId, UserId};
use rune_provider::Event;

use crate::{Applied, Permit, Store};

/// What the provider sent of servers' events this session.
#[derive(Debug, Default)]
pub(crate) struct EventsState {
    /// By server, once its list was read; in the order a list shows them.
    servers: HashMap<ServerId, Vec<ScheduledEvent>>,
    /// Who each event interests, once read.
    interested: HashMap<ScheduledEventId, Vec<UserId>>,
}

/// What the account may do with a server's events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventAbilities {
    /// See its events and say one interests the account.
    pub view: Permit,
    /// Plan a new one.
    pub create: Permit,
}

impl Store {
    /// `server`'s events, in the order a list shows them; `None` until
    /// the provider sent them this session.
    pub fn scheduled_events(&self, server: ServerId) -> Option<&[ScheduledEvent]> {
        self.events.servers.get(&server).map(Vec::as_slice)
    }

    /// The event `id` of `server`, as last reported.
    pub fn scheduled_event(
        &self,
        server: ServerId,
        id: ScheduledEventId,
    ) -> Option<&ScheduledEvent> {
        self.scheduled_events(server)?
            .iter()
            .find(|event| event.id == id)
    }

    /// The events of `server` still to come or happening: what its entry
    /// in the column counts.
    pub fn upcoming_events(&self, server: ServerId) -> usize {
        self.scheduled_events(server).map_or(0, |events| {
            events.iter().filter(|event| !event.status.ended()).count()
        })
    }

    /// Who `event` interests, once read.
    pub fn event_interested(&self, event: ScheduledEventId) -> Option<&[UserId]> {
        self.events.interested.get(&event).map(Vec::as_slice)
    }

    /// What the account may do with `server`'s events.
    pub fn event_abilities(&self, server: ServerId) -> EventAbilities {
        let capabilities = self
            .provider
            .as_ref()
            .map(|info| info.capabilities)
            .unwrap_or_default();
        let name = self
            .server(server)
            .map(|known| known.name.clone())
            .unwrap_or_default();
        let view = if !capabilities.scheduled_events {
            Permit::Denied(t("Este proveedor no muestra eventos").into())
        } else if self.me.and_then(|me| self.member(server, me)).is_none() {
            Permit::Denied(tr!("Todavía no eres miembro de {server}", server = name))
        } else {
            Permit::Allowed
        };
        let create = if !view.allowed() {
            view.clone()
        } else if !capabilities.manage_scheduled_events {
            Permit::Denied(t("Este proveedor no permite crear eventos").into())
        } else if !self
            .server_permissions(server)
            .intersects(Permissions::CREATE_EVENTS | Permissions::MANAGE_EVENTS)
        {
            Permit::Denied(tr!(
                "No tienes permiso para crear eventos en {server}",
                server = name
            ))
        } else if let Some(restriction) = self.restriction(server) {
            crate::profiles::restricted(restriction, &name)
        } else {
            Permit::Allowed
        };
        EventAbilities { view, create }
    }

    /// Whether the account may change, start, end or delete `event`:
    /// anyone's with «Gestionar eventos», its own with «Crear eventos»;
    /// never one that is over.
    pub fn may_change_event(&self, event: &ScheduledEvent) -> Permit {
        let create = self.event_abilities(event.server).create;
        if !create.allowed() {
            return create;
        }
        let permissions = self.server_permissions(event.server);
        if event.status.ended() {
            Permit::Denied(t("El evento ya terminó").into())
        } else if permissions.contains(Permissions::MANAGE_EVENTS)
            || (event.creator.is_some() && event.creator == self.me)
        {
            Permit::Allowed
        } else {
            Permit::Denied(
                t("Sólo quien lo creó o la moderación pueden cambiar este evento").into(),
            )
        }
    }

    /// Apply an event of the provider about scheduled events.
    pub(crate) fn apply_events(&mut self, event: Event) -> Applied {
        let (request, server, id) = match event {
            Event::ScheduledEvents {
                request,
                server,
                mut events,
            } => {
                rune_model::scheduled_events::in_order(&mut events);
                self.events.servers.insert(server, events);
                (Some(request), server, None)
            }
            Event::ScheduledEventChanged { request, mut event } => {
                let (server, id) = (event.server, event.id);
                // A list not read yet is read whole when shown.
                if let Some(events) = self.events.servers.get_mut(&server) {
                    // A change reported says nothing of who it interests:
                    // that stays as known.
                    if let Some(known) = events.iter().find(|known| known.id == event.id) {
                        event.interested = event.interested.or(known.interested);
                        event.account_interested = known.account_interested;
                    }
                    events.retain(|known| known.id != event.id);
                    events.push(event);
                    rune_model::scheduled_events::in_order(events);
                }
                (request, server, Some(id))
            }
            Event::ScheduledEventDeleted {
                request,
                server,
                event,
            } => {
                if let Some(events) = self.events.servers.get_mut(&server) {
                    events.retain(|known| known.id != event);
                }
                self.events.interested.remove(&event);
                (request, server, Some(event))
            }
            Event::EventInterest {
                request,
                server,
                event,
                user,
                interested,
            } => {
                self.interest_changed(server, event, user, interested);
                (request, server, Some(event))
            }
            Event::EventInterested {
                request,
                server,
                event,
                users,
            } => {
                let people = users.iter().map(|user| user.id).collect();
                for user in users {
                    self.users.entry(user.id).or_insert(user);
                }
                self.events.interested.insert(event, people);
                (Some(request), server, Some(event))
            }
            _ => return Applied::Nothing,
        };
        Applied::ScheduledEvents {
            request,
            server,
            event: id,
        }
    }

    /// `event` interests `user` now, or no longer. The account's own is
    /// counted once, whether its answer or the service's report comes
    /// first; someone else's, once more each time it is reported.
    fn interest_changed(
        &mut self,
        server: ServerId,
        event: ScheduledEventId,
        user: UserId,
        interested: bool,
    ) {
        let me = self.me == Some(user);
        let people = self.events.interested.get_mut(&event);
        let known = people
            .as_ref()
            .map(|people| people.contains(&user) == interested);
        let Some(found) = self
            .events
            .servers
            .get_mut(&server)
            .and_then(|events| events.iter_mut().find(|known| known.id == event))
        else {
            return;
        };
        let already = if me {
            found.account_interested == interested
        } else {
            known.unwrap_or(false)
        };
        if !already {
            found.interested = found.interested.map(|count| {
                if interested {
                    count + 1
                } else {
                    count.saturating_sub(1)
                }
            });
        }
        if me {
            found.account_interested = interested;
        }
        if let Some(people) = people {
            people.retain(|known| *known != user);
            if interested {
                people.push(user);
            }
        }
    }

    /// A new session reports every event again, when asked.
    pub(crate) fn events_ready(&mut self) {
        self.events = EventsState::default();
    }
}
