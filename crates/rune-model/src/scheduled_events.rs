//! A server's scheduled events (2026-10-19, branch `eventos-programados`):
//! something planned for a time, on one of its stages or voice channels or
//! somewhere else, which members may say interests them. The limits are
//! the official client's (docs/REA.md, «eventos programados»); none was
//! confirmed against a service.

use rune_i18n::t;

use crate::{ChannelId, RemoteImage, ScheduledEventId, ServerId, Timestamp, UserId};

/// Characters of an event's name, of its description and of the place
/// written for one held elsewhere, at most.
pub const MAX_EVENT_NAME: usize = 100;
pub const MAX_EVENT_DESCRIPTION: usize = 1000;
pub const MAX_EVENT_LOCATION: usize = 100;

/// Where an event happens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventPlace {
    /// A stage channel of its server.
    Stage(ChannelId),
    /// A voice channel of its server.
    Voice(ChannelId),
    /// Somewhere else, as written: an address or a link.
    External(String),
}

impl EventPlace {
    /// The channel it happens in, if it happens in one.
    pub fn channel(&self) -> Option<ChannelId> {
        match self {
            Self::Stage(channel) | Self::Voice(channel) => Some(*channel),
            Self::External(_) => None,
        }
    }
}

/// How far along an event is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventStatus {
    Scheduled,
    /// Happening now.
    Active,
    Completed,
    Canceled,
}

impl EventStatus {
    /// Whether it is over, one way or the other.
    pub fn ended(self) -> bool {
        matches!(self, Self::Completed | Self::Canceled)
    }
}

/// An event of a server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledEvent {
    pub id: ScheduledEventId,
    pub server: ServerId,
    pub name: String,
    pub description: Option<String>,
    pub start: Timestamp,
    /// When it ends; one held elsewhere always says.
    pub end: Option<Timestamp>,
    pub place: EventPlace,
    pub status: EventStatus,
    /// Its cover.
    pub image: Option<RemoteImage>,
    /// Who planned it, when the provider says.
    pub creator: Option<UserId>,
    /// How many people it interests, when the provider says.
    pub interested: Option<u64>,
    /// Whether it interests the account.
    pub account_interested: bool,
}

/// What the account writes of an event: everything it chooses of one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventDraft {
    pub name: String,
    pub description: Option<String>,
    pub start: Timestamp,
    pub end: Option<Timestamp>,
    pub place: EventPlace,
}

impl EventDraft {
    /// What `event` has of a draft, to edit it.
    pub fn of(event: &ScheduledEvent) -> Self {
        Self {
            name: event.name.clone(),
            description: event.description.clone(),
            start: event.start,
            end: event.end,
            place: event.place.clone(),
        }
    }

    /// Why it cannot be saved at `now`, if it cannot. `starts` says
    /// whether its start still matters: it does for a new event and for
    /// one that has not started.
    pub fn problem(&self, now: Timestamp, starts: bool) -> Option<&'static str> {
        let name = self.name.trim();
        if name.is_empty() {
            return Some(t("Poné un nombre al evento."));
        }
        if name.chars().count() > MAX_EVENT_NAME {
            return Some(t("El nombre del evento es demasiado largo."));
        }
        if self
            .description
            .as_ref()
            .is_some_and(|text| text.chars().count() > MAX_EVENT_DESCRIPTION)
        {
            return Some(t("La descripción es demasiado larga."));
        }
        if let EventPlace::External(location) = &self.place {
            if location.trim().is_empty() {
                return Some(t("Escribí dónde es el evento."));
            }
            if location.chars().count() > MAX_EVENT_LOCATION {
                return Some(t("El lugar es demasiado largo."));
            }
            if self.end.is_none() {
                return Some(t("Un evento en otro lugar necesita una hora de fin."));
            }
        }
        if starts && self.start <= now {
            return Some(t("El evento tiene que empezar más adelante."));
        }
        if self.end.is_some_and(|end| end <= self.start) {
            return Some(t("El evento tiene que terminar después de empezar."));
        }
        None
    }
}

/// `events` in the order a list shows them: happening now first, then the
/// next to start; those over last.
pub fn in_order(events: &mut [ScheduledEvent]) {
    events.sort_by_key(|event| {
        (
            match event.status {
                EventStatus::Active => 0,
                EventStatus::Scheduled => 1,
                EventStatus::Completed | EventStatus::Canceled => 2,
            },
            event.start,
            event.id,
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(place: EventPlace, end: Option<i64>) -> EventDraft {
        EventDraft {
            name: "Noche de juegos".into(),
            description: None,
            start: Timestamp(2_000),
            end: end.map(Timestamp),
            place,
        }
    }

    #[test]
    fn a_draft_says_what_is_missing() {
        let now = Timestamp(1_000);
        assert_eq!(
            draft(EventPlace::Voice(ChannelId(4)), None).problem(now, true),
            None
        );
        let unnamed = EventDraft {
            name: "  ".into(),
            ..draft(EventPlace::Voice(ChannelId(4)), None)
        };
        assert!(unnamed.problem(now, true).is_some());
        // Elsewhere needs a place and an end.
        assert!(draft(EventPlace::External(" ".into()), Some(3_000))
            .problem(now, true)
            .is_some());
        assert!(draft(EventPlace::External("Plaza".into()), None)
            .problem(now, true)
            .is_some());
        assert_eq!(
            draft(EventPlace::External("Plaza".into()), Some(3_000)).problem(now, true),
            None
        );
        // An end before the start, and a start already past.
        assert!(draft(EventPlace::Voice(ChannelId(4)), Some(2_000))
            .problem(now, true)
            .is_some());
        let past = draft(EventPlace::Voice(ChannelId(4)), None);
        assert!(past.problem(Timestamp(5_000), true).is_some());
        assert_eq!(past.problem(Timestamp(5_000), false), None);
        let long = EventDraft {
            name: "x".repeat(MAX_EVENT_NAME + 1),
            ..draft(EventPlace::Voice(ChannelId(4)), None)
        };
        assert!(long.problem(now, true).is_some());
    }

    #[test]
    fn a_list_puts_what_happens_now_first_and_what_ended_last() {
        let event = |id: u64, start: i64, status| ScheduledEvent {
            id: ScheduledEventId(id),
            server: ServerId(1),
            name: id.to_string(),
            description: None,
            start: Timestamp(start),
            end: None,
            place: EventPlace::External("Plaza".into()),
            status,
            image: None,
            creator: None,
            interested: None,
            account_interested: false,
        };
        let mut events = vec![
            event(1, 50, EventStatus::Completed),
            event(2, 300, EventStatus::Scheduled),
            event(3, 200, EventStatus::Scheduled),
            event(4, 10, EventStatus::Active),
        ];
        in_order(&mut events);
        let order: Vec<u64> = events.iter().map(|event| event.id.0).collect();
        assert_eq!(order, [4, 3, 2, 1]);
        assert!(EventStatus::Canceled.ended() && !EventStatus::Active.ended());
        assert_eq!(
            EventPlace::Stage(ChannelId(9)).channel(),
            Some(ChannelId(9))
        );
    }
}
