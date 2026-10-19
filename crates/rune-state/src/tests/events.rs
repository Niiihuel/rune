//! Scheduled events (eventos-programados): a server's list as the provider
//! sends it, kept current by what it reports, interest counted once, and
//! what the account may do with each event.

use super::*;
use rune_model::{EventPlace, EventStatus, ScheduledEvent, ScheduledEventId, UserId};
use rune_provider::mock::{MockConfig, MockProvider};
use rune_provider::Provider;

/// The simulation's start: the account owns the second server.
fn started() -> (MockProvider, Store) {
    let provider = MockProvider::new(MockConfig::default());
    let mut store = Store::new();
    store.provider = Some(provider.info());
    store.apply(Event::Ready(Box::new(provider.snapshot())), Instant::now());
    (provider, store)
}

fn listed(provider: &MockProvider, store: &mut Store, server: ServerId) -> Vec<ScheduledEvent> {
    for event in provider.manage(Command::LoadScheduledEvents { request: 1, server }) {
        assert_eq!(
            store.apply(event, Instant::now()),
            Applied::ScheduledEvents {
                request: Some(1),
                server,
                event: None,
            }
        );
    }
    store.scheduled_events(server).unwrap().to_vec()
}

#[test]
fn a_list_is_kept_in_order_and_current() {
    let (provider, mut store) = started();
    let owned = ServerId(2);
    assert!(store.scheduled_events(owned).is_none());
    let events = listed(&provider, &mut store, owned);
    assert_eq!(store.upcoming_events(owned), 2);
    assert!(events[0].start < events[1].start);
    // A change reported keeps who it interests; one deleted goes.
    let first = events[0].clone();
    let reported = ScheduledEvent {
        status: EventStatus::Active,
        interested: None,
        account_interested: false,
        ..first.clone()
    };
    store.apply(
        Event::ScheduledEventChanged {
            request: None,
            event: reported,
        },
        Instant::now(),
    );
    let now = store.scheduled_event(owned, first.id).unwrap();
    assert_eq!(now.status, EventStatus::Active);
    assert_eq!(now.interested, first.interested);
    assert!(now.account_interested);
    store.apply(
        Event::ScheduledEventDeleted {
            request: None,
            server: owned,
            event: first.id,
        },
        Instant::now(),
    );
    assert_eq!(store.upcoming_events(owned), 1);
    // A new session reads them again.
    store.apply(Event::Ready(Box::new(provider.snapshot())), Instant::now());
    assert!(store.scheduled_events(owned).is_none());
}

#[test]
fn the_accounts_interest_counts_once_whichever_report_comes_first() {
    let (provider, mut store) = started();
    let owned = ServerId(2);
    let event = listed(&provider, &mut store, owned)[1].clone();
    assert!(!event.account_interested);
    let count = event.interested.unwrap();
    let me = store.me().unwrap();
    let said = |store: &mut Store, request, user, interested| {
        store.apply(
            Event::EventInterest {
                request,
                server: owned,
                event: event.id,
                user,
                interested,
            },
            Instant::now(),
        )
    };
    // The answer, then the Gateway's report of the same.
    for answer in provider.manage(Command::SetEventInterest {
        request: 2,
        server: owned,
        event: event.id,
        interested: true,
    }) {
        store.apply(answer, Instant::now());
    }
    said(&mut store, None, me, true);
    let now = store.scheduled_event(owned, event.id).unwrap();
    assert!(now.account_interested);
    assert_eq!(now.interested, Some(count + 1));
    // Someone else, with the list of people read: once.
    for change in provider.manage(Command::LoadEventInterested {
        request: 3,
        server: owned,
        event: event.id,
    }) {
        store.apply(change, Instant::now());
    }
    assert!(store.event_interested(event.id).unwrap().contains(&me));
    said(&mut store, None, UserId(20), true);
    said(&mut store, None, UserId(20), true);
    assert_eq!(
        store.scheduled_event(owned, event.id).unwrap().interested,
        Some(count + 2)
    );
    said(&mut store, None, me, false);
    assert_eq!(
        store.scheduled_event(owned, event.id).unwrap().interested,
        Some(count + 1)
    );
}

#[test]
fn who_may_plan_and_change_events() {
    let (provider, mut store) = started();
    let (owned, first) = (ServerId(2), ServerId(1));
    assert!(store.event_abilities(owned).create.allowed());
    // In the first server the account has only what its roles give.
    assert!(store.event_abilities(first).view.allowed());
    assert!(!store.event_abilities(first).create.allowed());
    let talk = listed(&provider, &mut store, first)
        .into_iter()
        .find(|event| matches!(event.place, EventPlace::Stage(_)))
        .unwrap();
    assert!(!store.may_change_event(&talk).allowed());
    let mine = listed(&provider, &mut store, owned)[0].clone();
    assert!(store.may_change_event(&mine).allowed());
    let over = ScheduledEvent {
        status: EventStatus::Completed,
        ..mine
    };
    assert!(!store.may_change_event(&over).allowed());
    // Without the provider's capability, nothing.
    let mut info = provider.info();
    info.capabilities.scheduled_events = false;
    store.provider = Some(info);
    assert!(!store.event_abilities(owned).view.allowed());
    assert!(!store.event_abilities(owned).create.allowed());
    assert_eq!(
        store.scheduled_event(owned, ScheduledEventId(1)),
        None,
        "an id the server does not have"
    );
}
