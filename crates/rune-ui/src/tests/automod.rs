//! AutoMod (automod), over the simulation: the owner lists the rules of
//! its server, turns one on, edits, makes and deletes them, and a rule the
//! service would refuse is not sent; a plain member is told why not.

use super::*;
use rune_model::{AutoModAction, AutoModRuleId, AutoModTrigger, ServerId};
use rune_provider::Failure;

use crate::server_settings::automod::{KEYWORDS, NAME, PATTERNS};

const LAB: ServerId = ServerId(2);
const WORKSHOP: ServerId = ServerId(1);

/// Carry out the AutoMod commands sent since `from` on the simulation, as
/// its session would answer them.
fn answer(host: &mut Host, from: usize) {
    let sent: Vec<Command> = host.sent[from..].to_vec();
    for command in sent {
        if matches!(
            command,
            Command::LoadAutoModRules { .. }
                | Command::SaveAutoModRule { .. }
                | Command::SetAutoModRuleEnabled { .. }
                | Command::DeleteAutoModRule { .. }
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

fn page(host: &Host) -> &crate::server_settings::automod::AutoModPage {
    &host.ui.server_settings.as_ref().unwrap().automod
}

/// Open the AutoMod page of `server`'s settings, answered.
fn open(host: &mut Host, server: ServerId) {
    press(host, &format!("ss:open:{}", server.0));
    let from = host.sent.len();
    host.click("ss:page:automod");
    answer(host, from);
}

fn rule(host: &Host, id: u64) -> rune_model::AutoModRule {
    host.store
        .automod_rules(LAB)
        .unwrap()
        .iter()
        .find(|rule| rule.id == Some(AutoModRuleId(id)))
        .cloned()
        .unwrap()
}

#[test]
fn the_owner_lists_turns_on_and_edits_its_rules() {
    let mut host = Host::new();
    open(&mut host, LAB);
    let painted = host.painted().join(" ");
    for said in [
        "AutoMod",
        "Sin spoilers",
        "Spam de menciones",
        "Lo que parece spam",
        "Crear una regla",
        "Avisa en #limn-core",
    ] {
        assert!(painted.contains(said), "{said} in {painted}");
    }
    // The switch asks the provider; the rule changes once it answers.
    assert!(!rule(&host, 900_003).enabled);
    let from = host.sent.len();
    press(&mut host, "ss:am:rule:900003:enabled");
    assert!(!rule(&host, 900_003).enabled, "nothing shows ahead");
    answer(&mut host, from);
    assert!(rule(&host, 900_003).enabled);
    // The editor opens with what the rule has, as typed.
    press(&mut host, "ss:am:rule:900001:edit");
    let draft = page(&host).editing.clone().unwrap();
    assert_eq!(draft.keywords, "spoiler*, *final de temporada*");
    assert_eq!(draft.notice, "Guarda los spoilers para #componentes.");
    // A pattern the service would refuse is not sent: the page says why.
    write(&mut host, PATTERNS, "(abierto");
    let painted = host.painted().join(" ");
    assert!(
        painted.contains("no es una expresión regular válida"),
        "{painted}"
    );
    let from = host.sent.len();
    press(&mut host, "ss:am:save");
    assert!(host.sent[from..].is_empty());
    write(&mut host, PATTERNS, r"\bfiltrad[oa]\b");
    write(&mut host, KEYWORDS, "spoiler*, desenlace");
    press(&mut host, "ss:am:action:timeout");
    let from = host.sent.len();
    press(&mut host, "ss:am:save");
    let [Command::SaveAutoModRule { rule: sent, .. }] = &host.sent[from..] else {
        panic!("one save: {:?}", &host.sent[from..]);
    };
    let AutoModTrigger::Keywords(filter) = &sent.trigger else {
        panic!("words");
    };
    assert_eq!(filter.keywords, ["spoiler*", "desenlace"]);
    assert_eq!(filter.patterns, [r"\bfiltrad[oa]\b"]);
    assert!(sent
        .actions
        .contains(&AutoModAction::Timeout { seconds: 60 }));
    // While it is on its way nothing changes; its answer closes the editor.
    write(&mut host, NAME, "Otro nombre");
    assert_eq!(page(&host).editing.as_ref().unwrap().name, "Sin spoilers");
    answer(&mut host, from);
    assert!(page(&host).editing.is_none());
    let AutoModTrigger::Keywords(filter) = rule(&host, 900_001).trigger else {
        panic!("words");
    };
    assert_eq!(filter.keywords, ["spoiler*", "desenlace"]);
}

#[test]
fn the_owner_makes_and_deletes_a_rule() {
    let mut host = Host::new();
    open(&mut host, LAB);
    // The server has its one rule of mentions: no other is offered.
    press(&mut host, "ss:am:new:mentions");
    assert!(page(&host).editing.is_none());
    press(&mut host, "ss:am:new:keywords");
    assert!(page(&host).editing.is_some());
    let painted = host.painted().join(" ");
    assert!(painted.contains("Agrega al menos una palabra"), "{painted}");
    write(&mut host, KEYWORDS, "jaja*");
    press(&mut host, "ss:am:action:alert");
    let from = host.sent.len();
    press(&mut host, "ss:am:save");
    let [Command::SaveAutoModRule { rule: sent, .. }] = &host.sent[from..] else {
        panic!("one save: {:?}", &host.sent[from..]);
    };
    assert!(sent.id.is_none());
    assert!(sent
        .actions
        .iter()
        .any(|action| matches!(action, AutoModAction::Alert { channel } if channel.0 != 0)));
    answer(&mut host, from);
    let made = host
        .store
        .automod_rules(LAB)
        .unwrap()
        .last()
        .cloned()
        .unwrap();
    assert_eq!(made.name, "Bloquear palabras propias");
    let id = made.id.unwrap().0;
    // Deleting asks first.
    press(&mut host, &format!("ss:am:rule:{id}:delete"));
    assert!(host
        .painted()
        .join(" ")
        .contains("¿Seguro que quieres eliminar la regla «Bloquear palabras propias»?"));
    let from = host.sent.len();
    press(&mut host, "ss:am:delete:confirm");
    assert!(matches!(
        &host.sent[from..],
        [Command::DeleteAutoModRule { rule, .. }] if rule.0 == id
    ));
    answer(&mut host, from);
    assert_eq!(host.store.automod_rules(LAB).unwrap().len(), 3);
}

#[test]
fn a_plain_member_is_told_why_and_nothing_is_asked() {
    let mut host = Host::new();
    let from = host.sent.len();
    open(&mut host, WORKSHOP);
    assert!(!host.sent[from..]
        .iter()
        .any(|command| matches!(command, Command::LoadAutoModRules { .. })));
    let painted = host.painted().join(" ");
    assert!(
        painted.contains("No tienes permiso para administrar AutoMod en Taller de Rust"),
        "{painted}"
    );
}

#[test]
fn a_failed_read_offers_to_try_again() {
    let mut host = Host::new();
    press(&mut host, &format!("ss:open:{}", LAB.0));
    let from = host.sent.len();
    host.click("ss:page:automod");
    let [Command::LoadAutoModRules { request, .. }] = host.sent[from..] else {
        panic!("a read: {:?}", &host.sent[from..]);
    };
    host.event(Event::Failed {
        failure: Failure::AutoModRules {
            request,
            server: LAB,
        },
        reason: "sin conexión".into(),
        ambiguous: false,
    });
    assert!(host
        .painted()
        .join(" ")
        .contains("No se pudieron leer las reglas."));
    let from = host.sent.len();
    host.click("ss:am:reload");
    answer(&mut host, from);
    assert_eq!(host.store.automod_rules(LAB).unwrap().len(), 3);
}
