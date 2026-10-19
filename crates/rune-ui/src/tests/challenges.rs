//! The question a verification asks: nothing opens until the person answers,
//! each answer goes out once, and the action's end closes it.

use super::*;
use rune_provider::Failure;

const ASKED: Failure = Failure::Relationship { user: None };

fn offers(host: &mut Host, id: &str) -> bool {
    host.frame()
        .regions
        .iter()
        .any(|region| region.id == id && region.kind == limn::HitKind::Button)
}

#[test]
fn a_verification_asks_first_and_each_answer_goes_out_once() {
    let mut host = Host::new();
    host.event(Event::ChallengeRequired {
        challenge: 3,
        failure: ASKED,
    });
    let painted = host.painted().join(" ");
    assert!(painted.contains("Verificación simulada"), "{painted}");
    assert!(painted.contains("cambiar la relación"), "{painted}");
    let before = host.sent.len();
    host.click("challenge:3:solve");
    assert_eq!(
        host.sent[before..],
        [Command::SolveChallenge {
            challenge: 3,
            solve: true
        }]
    );
    assert!(!offers(&mut host, "challenge:3:solve"));

    host.event(Event::ChallengeRequired {
        challenge: 4,
        failure: ASKED,
    });
    let before = host.sent.len();
    host.click("challenge:4:cancel");
    assert_eq!(
        host.sent[before..],
        [Command::SolveChallenge {
            challenge: 4,
            solve: false
        }]
    );
}

#[test]
fn the_action_ending_closes_the_question() {
    let mut host = Host::new();
    host.event(Event::ChallengeRequired {
        challenge: 5,
        failure: ASKED,
    });
    assert!(offers(&mut host, "challenge:5:cancel"));
    host.event(Event::Failed {
        failure: ASKED,
        reason: "Venció el plazo de la verificación; no se envió nada.".into(),
        ambiguous: false,
    });
    assert!(!offers(&mut host, "challenge:5:cancel"));
}
