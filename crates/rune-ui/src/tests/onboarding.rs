//! Rules and onboarding (2026-10-18), over the simulation: joining the club
//! asks its questions and its rules, which let the account in once
//! answered; the darkroom's rules go to its moderators; a guest is never
//! asked; the owner edits its community's onboarding.

use super::*;
use rune_model::{ChoiceId, JoinOutcome, PromptId, RulesAnswer, ServerId};
use rune_provider::{Failure, InvitationDestination, InvitationPreview, InvitationResult};

/// The server the simulation's invitation «club» joins.
const CLUB: ServerId = ServerId(8_000_000_001);
/// The radio «radio» lets a guest into, and its voice channel.
const RADIO: ServerId = ServerId(8_000_000_002);
const RADIO_VOICE: ChannelId = ChannelId(8_000_000_003);
/// The darkroom «revelado» joins, whose moderators review its rules.
const DARKROOM: ServerId = ServerId(8_000_000_005);

/// Carry out the rules and onboarding commands sent since `from` on the
/// simulation, as its session would answer them, also those an answer
/// leads to.
fn answer(host: &mut Host, from: usize) {
    let mut next = from;
    while let Some(command) = host.sent.get(next).cloned() {
        next += 1;
        if matches!(
            command,
            Command::LoadServerRules { .. }
                | Command::AnswerServerRules { .. }
                | Command::LoadOnboarding { .. }
                | Command::AnswerOnboarding { .. }
                | Command::SaveOnboarding { .. }
        ) {
            for event in host.provider.manage(command) {
                host.event(event);
            }
        }
    }
}

/// An action as the tree reports it.
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

fn sent_since(host: &Host, from: usize) -> Vec<&Command> {
    host.sent[from..].iter().collect()
}

/// What is written in the field `id`, as the tree reports it.
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

/// Join the club through its invitation, as a person does.
fn join_club(host: &mut Host) {
    join(host, "club", InvitationDestination::Server(CLUB));
}

/// Accept the simulation's invitation `code`, which leads to `destination`.
fn join(host: &mut Host, code: &str, destination: InvitationDestination) {
    let target = host
        .provider
        .invitation_target(&format!("https://simulado.invalid/invite/{code}"))
        .unwrap();
    let intents = open_invitation(&mut host.ui, &host.store, target.clone());
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PreviewInvitation { request, .. } => Some(*request),
            _ => None,
        })
        .unwrap();
    host.run(intents);
    let intents = invitation_result(
        &mut host.ui,
        &host.store,
        InvitationResult {
            request,
            target,
            result: Ok(InvitationPreview {
                destination,
                name: "Club abierto".into(),
                description: None,
                image: None,
                members: Some(128),
                online: Some(17),
                expires_at: None,
                event: None,
            }),
        },
    );
    host.run(intents);
    host.click("invitation:accept");
    let accept = host
        .sent
        .iter()
        .rev()
        .find(|command| matches!(command, Command::AcceptInvitation { .. }))
        .cloned()
        .unwrap();
    for event in host.provider.manage(accept) {
        host.event(event);
    }
}

#[test]
fn joining_the_club_asks_its_questions_and_rules_and_lets_the_account_in_once_answered() {
    let mut host = Host::new();
    let from = host.sent.len();
    join_club(&mut host);
    // The dialog opened by itself and asked for both parts.
    assert_eq!(
        host.ui.join.as_ref().map(|dialog| dialog.server),
        Some(CLUB)
    );
    let steps = host.store.join_steps(CLUB);
    assert!(steps.rules && steps.onboarding);
    let asked = sent_since(&host, from);
    assert!(asked.iter().any(
        |command| matches!(command, Command::LoadOnboarding { server, .. } if *server == CLUB)
    ));
    assert!(asked.iter().any(
        |command| matches!(command, Command::LoadServerRules { server, .. } if *server == CLUB)
    ));
    answer(&mut host, from);
    let painted = host.painted().join(" ");
    let club = host.store.server(CLUB).unwrap().name.clone();
    for said in [
        &format!("Antes de participar en {club}"),
        "¿Qué te trae por el club?",
        "Reglas del servidor",
        "Respetá a todas las personas.",
        "Leí y acepto las reglas",
        "Simulación local: no se envía nada a ningún servicio.",
    ] {
        assert!(painted.contains(said), "{said} in {painted}");
    }
    // Until the rules are accepted, the composer says why it is shut.
    let general = host.store.first_text_channel(CLUB).unwrap();
    assert_eq!(
        host.store
            .abilities(general)
            .unwrap()
            .write
            .reason()
            .map(str::to_owned),
        Some(format!("Aceptá las reglas de {club} para participar"))
    );
    // Nothing answered: nothing goes, and each part says what is missing.
    let before = host.sent.len();
    host.click("onb:send");
    assert_eq!(host.sent.len(), before);
    let painted = host.painted().join(" ");
    assert!(
        painted.contains("Respondé «¿Qué te trae por el club?»"),
        "{painted}"
    );
    // Photography, and the rules: the questions go first.
    let photos = host.store.onboarding(CLUB).flatten().unwrap().prompts[0].options[0].clone();
    host.click(&format!("onb:option:{}", photos.id.0));
    host.click("onb:agree:0");
    let from = host.sent.len();
    host.click("onb:send");
    let [Command::AnswerOnboarding { chosen, .. }] = sent_since(&host, from).as_slice() else {
        panic!("only the questions first: {:?}", sent_since(&host, from));
    };
    assert_eq!(*chosen, vec![photos.id]);
    // Answered, the rules follow by themselves; accepted, the account takes part.
    answer(&mut host, from);
    assert!(host
        .sent
        .iter()
        .any(|command| matches!(command, Command::AnswerServerRules { .. })));
    let painted = host.painted().join(" ");
    assert!(
        painted.contains(&format!("Listo: ya podés participar en {club}.")),
        "{painted}"
    );
    assert!(!host.store.join_steps(CLUB).any());
    let me = host.store.me().unwrap();
    let member = host.store.member_of(CLUB, me).unwrap();
    assert!(photos.roles.iter().all(|role| member.roles.contains(role)));
    assert!(host.store.abilities(general).unwrap().write.allowed());
    host.click("onb:close");
    assert!(host.ui.join.is_none());
}

#[test]
fn later_leaves_a_bar_over_the_composer_that_opens_the_dialog_again() {
    let mut host = Host::new();
    let from = host.sent.len();
    join_club(&mut host);
    answer(&mut host, from);
    host.click("onb:close");
    assert!(host.ui.join.is_none());
    let open = format!("onb:open:{}", CLUB.0);
    let club = host.store.server(CLUB).unwrap().name.clone();
    let painted = host.painted().join(" ");
    assert!(
        painted.contains(&format!(
            "Antes de participar, {club} pide que aceptes sus reglas."
        )),
        "{painted}"
    );
    host.click(&open);
    // What arrived before is not asked again.
    assert_eq!(
        host.ui.join.as_ref().map(|dialog| dialog.server),
        Some(CLUB)
    );
    assert!(host
        .painted()
        .join(" ")
        .contains("¿Qué te trae por el club?"));
    host.key(Key::Escape, false, false);
    assert!(host.ui.join.is_none());
}

#[test]
fn the_owner_edits_its_communitys_onboarding_and_an_incomplete_question_never_goes() {
    let mut host = Host::new();
    let owned = host
        .store
        .servers()
        .iter()
        .find(|server| server.name == "Laboratorio limn")
        .map(|server| server.id)
        .unwrap();
    // A server that is not a community has no such page.
    let other = host.store.servers()[0].id;
    assert_ne!(other, owned);
    press(&mut host, &format!("ss:open:{}", other.0));
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "ss:page:onboarding"));
    host.key(Key::Escape, false, false);
    press(&mut host, &format!("ss:open:{}", owned.0));
    let from = host.sent.len();
    host.click("ss:page:onboarding");
    assert!(sent_since(&host, from).iter().any(
        |command| matches!(command, Command::LoadOnboarding { server, .. } if *server == owned)
    ));
    answer(&mut host, from);
    let painted = host.painted().join(" ");
    for said in [
        "Incorporación",
        "Activar la incorporación",
        "Canales por defecto",
    ] {
        assert!(painted.contains(said), "{said} in {painted}");
    }
    let prompt = host
        .ui
        .server_settings
        .as_ref()
        .and_then(|settings| settings.onboarding.draft.as_ref())
        .map(|draft| draft.prompts[0].id)
        .unwrap();
    let title = format!("ss:onb:prompt:{}:title", prompt.0);
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::InputChanged {
            id: title,
            value: "¿Qué te interesa?".into(),
        },
        host.now,
    );
    host.run(intents);
    assert!(host.ui.server_settings.as_ref().unwrap().dirty(&host.store));
    let from = host.sent.len();
    host.click("ss:save");
    assert!(matches!(
        sent_since(&host, from).as_slice(),
        [Command::SaveOnboarding { onboarding, .. }] if onboarding.prompts[0].title == "¿Qué te interesa?"
    ));
    // While the save is on its way the page waits: its answer replaces the draft.
    let draft = |host: &Host| {
        host.ui
            .server_settings
            .as_ref()
            .and_then(|settings| settings.onboarding.draft.clone())
            .unwrap()
    };
    write(
        &mut host,
        &format!("ss:onb:prompt:{}:title", prompt.0),
        "Otra cosa",
    );
    press(&mut host, &format!("ss:onb:prompt:{}:required", prompt.0));
    assert_eq!(draft(&host).prompts[0].title, "¿Qué te interesa?");
    assert!(!draft(&host).prompts[0].required);
    answer(&mut host, from);
    assert!(!host.ui.server_settings.as_ref().unwrap().dirty(&host.store));
    assert_eq!(
        host.store.onboarding(owned).flatten().unwrap().prompts[0].title,
        "¿Qué te interesa?"
    );
    // An option's emoji is one emoji: anything else is not sent.
    let option = draft(&host).prompts[0].options[0].id;
    write(
        &mut host,
        &format!("ss:onb:option:{}:emoji", option.0),
        "ab",
    );
    let from = host.sent.len();
    host.click("ss:save");
    assert!(sent_since(&host, from).is_empty());
    assert!(host.ui.notices.iter().any(|notice| notice.detail.as_deref()
        == Some("«ab» no es un solo emoji: escribí uno, como 📷.")));
    write(
        &mut host,
        &format!("ss:onb:option:{}:emoji", option.0),
        "🧩",
    );
    // A question without a title is not sent: the page says why.
    press(&mut host, "ss:onb:prompt-add");
    let from = host.sent.len();
    host.click("ss:save");
    assert!(sent_since(&host, from).is_empty());
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title == "No se guardó la incorporación"));
}

#[test]
fn an_onboarding_that_asks_nothing_is_owed_nothing_once_read() {
    let mut host = Host::new();
    let from = host.sent.len();
    join_club(&mut host);
    assert!(host.store.join_steps(CLUB).onboarding, "owed until read");
    host.event(Event::OnboardingLoaded {
        request: 0,
        server: CLUB,
        onboarding: Some(rune_model::Onboarding {
            enabled: true,
            ..Default::default()
        }),
    });
    assert!(!host.store.join_steps(CLUB).onboarding);
    // The rules are still owed.
    answer(&mut host, from);
    assert!(host.store.join_steps(CLUB).rules);
}

#[test]
fn the_darkrooms_moderators_review_written_and_chosen_answers() {
    let mut host = Host::new();
    let from = host.sent.len();
    join(
        &mut host,
        "revelado",
        InvitationDestination::Server(DARKROOM),
    );
    assert_eq!(
        host.ui.join.as_ref().map(|dialog| dialog.server),
        Some(DARKROOM)
    );
    let steps = host.store.join_steps(DARKROOM);
    assert!(steps.rules && !steps.onboarding);
    answer(&mut host, from);
    let painted = host.painted().join(" ");
    for said in [
        "Acepto las normas del taller",
        "Cuidá los químicos y el equipo.",
        "¿CÓMO TE LLAMÁS?",
        "Medio formato",
        "0/150",
    ] {
        assert!(painted.contains(said), "{said} in {painted}");
    }
    write(&mut host, "onb:text:1", "Ana");
    press(&mut host, "onb:choice:2:1");
    press(&mut host, "onb:agree:0");
    write(&mut host, "onb:text:3", "Mis primeros rollos");
    let from = host.sent.len();
    host.click("onb:send");
    let [Command::AnswerServerRules { answers, .. }] = sent_since(&host, from).as_slice() else {
        panic!("the rules alone: {:?}", sent_since(&host, from));
    };
    let answers = answers.clone();
    assert_eq!(
        answers,
        vec![
            RulesAnswer::Agreed(true),
            RulesAnswer::Text("Ana".into()),
            RulesAnswer::Choice(Some(1)),
            RulesAnswer::Text("Mis primeros rollos".into()),
        ]
    );
    // On its way, what was sent stays as it was.
    write(&mut host, "onb:text:1", "");
    press(&mut host, "onb:agree:0");
    let kept = host
        .ui
        .join
        .as_ref()
        .and_then(|dialog| dialog.answers.clone());
    assert_eq!(kept, Some(answers));
    answer(&mut host, from);
    let name = host.store.server(DARKROOM).unwrap().name.clone();
    // The line wraps: compare its words.
    let painted = host.painted().join(" ");
    let painted = painted.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        painted.contains(&format!(
            "Enviado: la moderación de {name} va a revisar tus respuestas."
        )),
        "{painted}"
    );
    let general = host.store.first_text_channel(DARKROOM).unwrap();
    let reason = |host: &Host| {
        host.store
            .abilities(general)
            .unwrap()
            .write
            .reason()
            .map(str::to_owned)
    };
    assert_eq!(
        reason(&host),
        Some(format!(
            "La moderación de {name} está revisando tus respuestas: vas a poder participar cuando las apruebe"
        ))
    );
    host.click("onb:close");
    // In review, nothing over the composer offers the rules again.
    assert!(!host
        .painted()
        .join(" ")
        .contains("pide que aceptes sus reglas"));
    host.event(Event::RulesAnswered {
        request: 0,
        server: DARKROOM,
        outcome: JoinOutcome::Rejected,
    });
    assert_eq!(
        reason(&host),
        Some(format!("La moderación de {name} rechazó tu solicitud"))
    );
}

#[test]
fn a_guest_of_a_server_with_onboarding_is_never_asked_it() {
    let mut host = Host::new();
    let from = host.sent.len();
    join(
        &mut host,
        "radio",
        InvitationDestination::Guest {
            server: RADIO,
            channel: RADIO_VOICE,
        },
    );
    assert!(host.store.server(RADIO).unwrap().options.onboarding);
    assert!(host.ui.join.is_none());
    assert!(!host.store.join_steps(RADIO).any());
    assert!(!sent_since(&host, from)
        .iter()
        .any(|command| matches!(command, Command::LoadOnboarding { .. })));
}

#[test]
fn options_reported_of_questions_asked_later_never_hold_the_answers_back() {
    let mut host = Host::new();
    let from = host.sent.len();
    join_club(&mut host);
    answer(&mut host, from);
    host.click("onb:close");
    let mut onboarding = host.store.onboarding(CLUB).flatten().unwrap().clone();
    let mut later = onboarding.prompts[0].clone();
    later.id = PromptId(77);
    later.in_onboarding = false;
    later.options.truncate(1);
    later.options[0].id = ChoiceId(78);
    onboarding.prompts.push(later);
    // A member from before: it answered a question asked only later.
    onboarding.chosen = vec![ChoiceId(78)];
    host.event(Event::OnboardingLoaded {
        request: 0,
        server: CLUB,
        onboarding: Some(onboarding.clone()),
    });
    host.click(&format!("onb:open:{}", CLUB.0));
    let photos = onboarding.prompts[0].options[0].id;
    press(&mut host, &format!("onb:option:{}", photos.0));
    press(&mut host, "onb:agree:0");
    let from = host.sent.len();
    host.click("onb:send");
    let [Command::AnswerOnboarding { chosen, .. }] = sent_since(&host, from).as_slice() else {
        panic!("the questions go: {:?}", sent_since(&host, from));
    };
    assert_eq!(*chosen, vec![photos]);
}

#[test]
fn a_part_that_did_not_arrive_is_asked_again_before_anything_goes() {
    let mut host = Host::new();
    let from = host.sent.len();
    join_club(&mut host);
    // The questions arrive; reading the rules fails.
    let asked: Vec<Command> = host.sent[from..].to_vec();
    for command in asked {
        match command {
            Command::LoadOnboarding { .. } => {
                for event in host.provider.manage(command) {
                    host.event(event);
                }
            }
            Command::LoadServerRules { request, server } => host.event(Event::Failed {
                failure: Failure::Rules { request, server },
                reason: "sin conexión".into(),
                ambiguous: false,
            }),
            _ => {}
        }
    }
    let painted = host.painted().join(" ");
    assert!(
        painted.contains("Reintentar") && !painted.contains("Terminar"),
        "{painted}"
    );
    let from = host.sent.len();
    host.click("onb:retry");
    assert!(
        matches!(
            sent_since(&host, from).as_slice(),
            [Command::LoadServerRules { .. }]
        ),
        "{:?}",
        sent_since(&host, from)
    );
    answer(&mut host, from);
    assert!(host.painted().join(" ").contains("Terminar"));
}

#[test]
fn a_verification_over_the_dialog_takes_escape_and_answered_elsewhere_it_closes() {
    let mut host = Host::new();
    let from = host.sent.len();
    join_club(&mut host);
    answer(&mut host, from);
    host.event(Event::ChallengeRequired {
        challenge: 7,
        failure: Failure::Rules {
            request: 99,
            server: CLUB,
        },
    });
    host.key(Key::Escape, false, false);
    assert!(host.ui.join.is_some(), "the verification takes the key");
    // Done from another client: nothing is owed and the dialog goes.
    let me = host.store.me().unwrap();
    let mut member = host.store.member_of(CLUB, me).unwrap().clone();
    member.restrictions.pending = false;
    member.restrictions.onboarded = true;
    host.event(Event::MemberChanged(member));
    assert!(host.ui.join.is_none());
}
