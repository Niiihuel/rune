//! Rules and onboarding (2026-10-18): what the account owes a server, and
//! what an answer changes when the provider reports only the answer, as
//! the personal session does before the member's update arrives.

use super::*;
use crate::JoinSteps;
use rune_model::JoinOutcome;
use rune_provider::mock::{MockConfig, MockProvider};
use rune_provider::Provider;

/// The simulation's start with the account owing the server of `general`
/// its rules and its questions; also the snapshot, to start it again.
fn owing() -> (Snapshot, Store, ServerId, ChannelId) {
    let provider = MockProvider::new(MockConfig::default());
    let mut snapshot = provider.snapshot();
    let general = snapshot
        .channels
        .iter()
        .find(|channel| channel.name == "general")
        .unwrap();
    let (general, server) = (general.id, general.server.unwrap());
    let found = snapshot
        .servers
        .iter_mut()
        .find(|found| found.id == server)
        .unwrap();
    found.options.onboarding = true;
    let me = provider.me();
    let mine = snapshot
        .members
        .iter_mut()
        .find(|member| member.server == server && member.user == me)
        .unwrap();
    mine.restrictions.pending = true;
    mine.restrictions.onboarded = false;
    let mut store = Store::new();
    store.provider = Some(provider.info());
    store.apply(Event::Ready(Box::new(snapshot.clone())), Instant::now());
    (snapshot, store, server, general)
}

#[test]
fn the_answers_alone_let_the_account_in() {
    let (_, mut store, server, general) = owing();
    assert_eq!(
        store.join_steps(server),
        JoinSteps {
            rules: true,
            onboarding: true
        }
    );
    assert!(!store.abilities(general).unwrap().write.allowed());
    store.apply(
        Event::OnboardingAnswered {
            request: 1,
            server,
            chosen: Vec::new(),
        },
        Instant::now(),
    );
    assert_eq!(
        store.join_steps(server),
        JoinSteps {
            rules: true,
            onboarding: false
        }
    );
    store.apply(
        Event::RulesAnswered {
            request: 2,
            server,
            outcome: JoinOutcome::Approved,
        },
        Instant::now(),
    );
    assert!(!store.join_steps(server).any());
    assert!(store.abilities(general).unwrap().write.allowed());
}

#[test]
fn reviewed_or_rejected_rules_say_so_and_stay_across_a_new_session() {
    let (snapshot, mut store, server, general) = owing();
    let name = store.server(server).unwrap().name.clone();
    let reason = |store: &Store| {
        store
            .abilities(general)
            .unwrap()
            .write
            .reason()
            .map(str::to_owned)
    };
    assert_eq!(
        reason(&store),
        Some(format!("Aceptá las reglas de {name} para participar"))
    );
    store.apply(
        Event::RulesAnswered {
            request: 1,
            server,
            outcome: JoinOutcome::InReview,
        },
        Instant::now(),
    );
    let reviewing = format!(
        "La moderación de {name} está revisando tus respuestas: vas a poder participar cuando las apruebe"
    );
    assert_eq!(reason(&store), Some(reviewing.clone()));
    assert!(store.join_steps(server).rules, "still pending");
    // Reconnected, the same account is not offered the rules again.
    store.apply(Event::Ready(Box::new(snapshot.clone())), Instant::now());
    assert_eq!(store.rules_outcome(server), Some(JoinOutcome::InReview));
    assert_eq!(reason(&store), Some(reviewing));
    store.apply(
        Event::RulesAnswered {
            request: 2,
            server,
            outcome: JoinOutcome::Rejected,
        },
        Instant::now(),
    );
    assert_eq!(
        reason(&store),
        Some(format!("La moderación de {name} rechazó tu solicitud"))
    );
}

#[test]
fn a_guest_is_never_asked_the_onboarding() {
    let (_, mut store, server, _) = owing();
    let me = store.me().unwrap();
    let mut guest = store.member_of(server, me).unwrap().clone();
    guest.restrictions.pending = false;
    guest.restrictions.guest = true;
    store.apply(Event::MemberChanged(guest), Instant::now());
    assert!(!store.join_steps(server).any());
}
