//! Scheduled events (eventos-programados), over the simulation: the entry
//! over the channels, the list, planning, interest and deleting in the
//! owned server, what someone without permission sees, and an invitation's
//! card that names an event.

use super::*;
use rune_components::events::ids;
use rune_model::{EventPlace, EventStatus, ScheduledEventId, ServerId, TextReference, TextTarget};
use rune_provider::{InvitationDestination, InvitationPreview, InvitationResult, InvitationTarget};

/// The server the account owns, and the first one, where it has only what
/// its roles give.
const OWNED: ServerId = ServerId(2);
const FIRST: ServerId = ServerId(1);

/// Carry out the event commands sent since `from` on the simulation, as
/// its session would answer them.
fn answer(host: &mut Host, from: usize) {
    let mut next = from;
    while let Some(command) = host.sent.get(next).cloned() {
        next += 1;
        if matches!(
            command,
            Command::LoadEventInterested { .. }
                | Command::SetEventInterest { .. }
                | Command::CreateScheduledEvent { .. }
                | Command::EditScheduledEvent { .. }
                | Command::SetScheduledEventStatus { .. }
                | Command::DeleteScheduledEvent { .. }
        ) {
            for event in host.provider.manage(command) {
                host.event(event);
            }
        }
    }
}

fn press(host: &mut Host, id: &str) {
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked { id: id.into() },
        host.now,
    );
    host.run(intents);
    host.frame();
}

fn write(host: &mut Host, id: &str, value: &str) {
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::InputChanged {
            id: id.into(),
            value: value.into(),
        },
        host.now,
    );
    host.run(intents);
}

fn controls(host: &mut Host) -> Vec<String> {
    host.frame()
        .regions
        .iter()
        .filter(|region| region.kind == limn::HitKind::Button)
        .map(|region| region.id.clone())
        .collect()
}

#[test]
fn the_owner_plans_marks_and_deletes_an_event() {
    let mut host = Host::new();
    host.open_channel("limn-core");
    // The entry read the list once and counts what is coming.
    let painted = host.painted().join(" ");
    assert!(painted.contains("Eventos 2"), "{painted}");
    assert_eq!(host.store.upcoming_events(OWNED), 2);
    host.click(ids::OPEN);
    let painted = host.painted().join(" ");
    assert!(painted.contains("Revisión del diseño de limn") && painted.contains("Asado"));
    assert!(painted.contains("Simulación local"));

    // Plan one elsewhere: it needs a place and an end.
    host.click(ids::NEW);
    let from = host.sent.len();
    write(&mut host, ids::NAME, "Hackatón");
    press(&mut host, ids::ELSEWHERE);
    write(&mut host, ids::START_DATE, "2031-03-01");
    write(&mut host, ids::START_TIME, "18:00");
    write(&mut host, ids::END_DATE, "2031-03-01");
    write(&mut host, ids::END_TIME, "20:00");
    press(&mut host, ids::SAVE);
    assert_eq!(host.sent.len(), from, "nothing goes without a place");
    assert!(host
        .painted()
        .join(" ")
        .contains("Escribí dónde es el evento."));
    write(&mut host, ids::LOCATION, "Sede del club");
    // A day picked in the calendar is written in its field.
    press(&mut host, &format!("{}:day:2031-03-02", ids::END_DATE));
    press(&mut host, ids::SAVE);
    let Some(Command::CreateScheduledEvent { draft, .. }) = host.sent.last().cloned() else {
        panic!("planned: {:?}", host.sent.last())
    };
    assert_eq!(draft.place, EventPlace::External("Sede del club".into()));
    assert_eq!(draft.name, "Hackatón");
    assert!(draft.end.unwrap().0 - draft.start.0 > 24 * 3600 * 1000);
    answer(&mut host, from);
    let painted = host.painted().join(" ");
    assert!(painted.contains("Hackatón") && painted.contains("Sede del club"));
    let created = host
        .store
        .scheduled_events(OWNED)
        .unwrap()
        .iter()
        .find(|event| event.name == "Hackatón")
        .unwrap()
        .clone();
    // Who plans it is interested; «Te interesa» lets it go.
    assert!(created.account_interested);
    let from = host.sent.len();
    host.click(&ids::interest(created.id));
    assert!(matches!(
        host.sent.last(),
        Some(Command::SetEventInterest {
            interested: false,
            ..
        })
    ));
    answer(&mut host, from);
    assert!(host.painted().join(" ").contains("Me interesa"));

    // Starting it, then deleting it after the question.
    let from = host.sent.len();
    host.click(&ids::start(created.id));
    answer(&mut host, from);
    assert_eq!(
        host.store
            .scheduled_event(OWNED, created.id)
            .map(|event| event.status),
        Some(EventStatus::Active)
    );
    host.click(&ids::delete(created.id));
    assert!(host.painted().join(" ").contains("¿Borrar «Hackatón»?"));
    let from = host.sent.len();
    host.click(ids::DELETE);
    answer(&mut host, from);
    assert!(host.store.scheduled_event(OWNED, created.id).is_none());
    assert!(!host.painted().join(" ").contains("Hackatón"));
    // Esc closes the list.
    host.key(Key::Escape, false, false);
    assert!(!host.painted().join(" ").contains("Eventos de"));
}

#[test]
fn without_permission_the_list_reads_but_offers_no_changes() {
    let mut host = Host::new();
    host.frame();
    assert!(host.store.scheduled_events(FIRST).is_some());
    host.click(ids::OPEN);
    let found = controls(&mut host);
    assert!(!found.contains(&ids::NEW.to_owned()), "{found:?}");
    let talk = host
        .store
        .scheduled_events(FIRST)
        .unwrap()
        .iter()
        .find(|event| event.status == EventStatus::Active)
        .unwrap()
        .clone();
    assert!(host.painted().join(" ").contains("En curso"));
    let from = host.sent.len();
    host.click(&ids::show(talk.id));
    answer(&mut host, from);
    let painted = host.painted().join(" ");
    assert!(painted.contains("Cómo pensar los futuros"), "{painted}");
    assert!(painted.contains("Les interesa a 12 personas"));
    let found = controls(&mut host);
    assert!(found.contains(&ids::interest(talk.id)));
    assert!(!found.contains(&ids::edit(talk.id)) && !found.contains(&ids::delete(talk.id)));
    // Esc goes back to the list first.
    host.key(Key::Escape, false, false);
    assert!(host.painted().join(" ").contains("Eventos de"));
}

#[test]
fn an_invitation_that_names_an_event_shows_it_and_opens_its_detail() {
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    let capabilities = &mut host.store.provider.as_mut().unwrap().capabilities;
    capabilities.invitation_preview = true;
    let meeting = host.store.scheduled_events(OWNED).unwrap()[0].clone();
    let mut message = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    message.id = MessageId(message.id.0 + 1);
    message.content = "https://fixture.invalid/invite/k?event=1".into();
    message.text_references = vec![TextReference {
        source: message.content.clone(),
        target: TextTarget::Invitation,
    }];
    let id = message.id;
    host.event(Event::MessageCreated {
        message,
        nonce: None,
    });
    host.frame();
    let recognize = |url: &str| {
        url.starts_with("https://fixture.invalid/")
            .then(|| InvitationTarget {
                key: "k".into(),
                url: url.into(),
            })
    };
    let wanted =
        wanted_invitation_cards(&mut host.ui, &host.store, &host.tree, host.now, recognize);
    let (request, target) = wanted.into_iter().next().expect("a lookup");
    invitation_card_result(
        &mut host.ui,
        &host.store,
        InvitationResult {
            request,
            target,
            result: Ok(InvitationPreview {
                destination: InvitationDestination::Server(OWNED),
                name: "Laboratorio limn".into(),
                description: None,
                image: None,
                members: Some(8),
                online: None,
                expires_at: None,
                event: Some(meeting.clone()),
            }),
        },
        host.now,
    );
    let painted = host.painted().join(" ");
    assert!(
        painted.contains(&meeting.name) && painted.contains("Ver evento"),
        "{painted}"
    );
    host.click(&format!("msg:{id}:invitation:0:event"));
    let painted = host.painted().join(" ");
    assert!(
        painted.contains("Repasamos los componentes nuevos"),
        "{painted}"
    );
    assert_eq!(meeting.id, ScheduledEventId(8_000_000_101));
}
