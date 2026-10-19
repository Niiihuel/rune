//! Commands and interactions (P3) against the simulation's RuneBot.

use std::time::{Duration, Instant};

use rune_model::interactions::{CommandUse, Interaction, OptionValue};
use rune_model::{ChannelId, Message, MessageId, Permissions};
use rune_provider::mock::{MockConfig, MockProvider};
use rune_provider::{Command, Event, Failure, Provider};

use crate::{Applied, CommandsState, Store, INTERACTION_WAIT};

/// A store ready with the simulation, #general loaded to its end.
fn ready() -> (Store, MockProvider, ChannelId, Instant) {
    let provider = MockProvider::new(MockConfig::default());
    let now = Instant::now();
    let mut store = Store::new();
    store.provider = Some(provider.info());
    store.apply(Event::Ready(Box::new(provider.snapshot())), now);
    let general = *store
        .channels
        .iter()
        .find(|(_, channel)| channel.name == "general")
        .unwrap()
        .0;
    let (messages, reached_start) = provider.history(general, None, 50);
    store.apply(
        Event::HistoryLoaded {
            request: 0,
            channel: general,
            before: None,
            messages,
            reached_start,
        },
        now,
    );
    (store, provider, general, now)
}

fn app_message(store: &Store, channel: ChannelId) -> MessageId {
    store
        .timeline(channel)
        .unwrap()
        .messages()
        .iter()
        .find(|message| message.components.whole && message.components.app.is_some())
        .unwrap()
        .id
}

fn press(message: MessageId, custom_id: &str) -> Interaction {
    Interaction::Part {
        message,
        custom_id: custom_id.into(),
        values: Vec::new(),
    }
}

#[test]
fn one_interaction_at_a_time_until_its_app_answers_or_the_wait_ends() {
    let (mut store, _, general, now) = ready();
    assert!(store.interaction_abilities(general).parts.allowed());
    let message = app_message(&store, general);
    let command = store
        .begin_interaction(1, general, press(message, "rb:confirmar"), now)
        .unwrap();
    assert!(matches!(command, Command::Interact { request: 1, .. }));
    assert_eq!(
        store.interaction_pending(now).unwrap().part,
        Some((message, "rb:confirmar".to_owned()))
    );
    assert!(
        store
            .begin_interaction(2, general, press(message, "rb:no-puedo"), now)
            .is_err(),
        "the first is still waited for"
    );
    store.apply(
        Event::Interaction {
            request: 1,
            done: false,
        },
        now,
    );
    assert!(store.interaction_pending(now).unwrap().received);
    store.apply(
        Event::Interaction {
            request: 1,
            done: true,
        },
        now,
    );
    assert!(store.interaction_pending(now).is_none());
    assert!(store.interaction_problem(now).is_none());
    // No answer in time: it stops being waited for and says so.
    store
        .begin_interaction(3, general, press(message, "rb:confirmar"), now)
        .unwrap();
    let late = now + INTERACTION_WAIT + Duration::from_secs(1);
    assert!(store.interaction_pending(late).is_none());
    assert!(store
        .interaction_problem(late)
        .unwrap()
        .reason
        .contains("no respondió"));
    assert!(store
        .begin_interaction(4, general, press(message, "rb:confirmar"), late)
        .is_ok());
    // A refusal is kept where it happened.
    let applied = store.apply(
        Event::Failed {
            failure: Failure::Interaction {
                request: 4,
                channel: general,
            },
            reason: "RuneBot no pudo".into(),
            ambiguous: false,
        },
        late,
    );
    assert!(matches!(applied, Applied::Failed { .. }));
    let problem = store.interaction_problem(late).unwrap();
    assert_eq!(problem.part, Some((message, "rb:confirmar".to_owned())));
    // A refusal of an old request changes nothing.
    let stale = store.apply(
        Event::Failed {
            failure: Failure::Interaction {
                request: 99,
                channel: general,
            },
            reason: "viejo".into(),
            ambiguous: false,
        },
        late,
    );
    assert_eq!(stale, Applied::Nothing);
}

#[test]
fn only_its_own_controls_and_values_go_to_an_app() {
    let (mut store, _, general, now) = ready();
    let message = app_message(&store, general);
    assert!(store
        .begin_interaction(1, general, press(message, "rb:otro"), now)
        .is_err());
    let pick = |values: &[&str]| Interaction::Part {
        message,
        custom_id: "rb:horario".into(),
        values: values.iter().map(|value| value.to_string()).collect(),
    };
    assert!(store
        .begin_interaction(1, general, pick(&["nunca"]), now)
        .is_err());
    assert!(store
        .begin_interaction(1, general, pick(&["viernes"]), now)
        .is_ok());
    // Without the capability, nothing goes.
    let (mut store, _, general, now) = ready();
    if let Some(info) = store.provider.as_mut() {
        info.capabilities.interactions = false;
    }
    let reason = store
        .begin_interaction(1, general, press(message, "rb:confirmar"), now)
        .unwrap_err();
    assert!(reason.contains("proveedor"), "{reason}");
}

#[test]
fn a_verification_the_person_solves_does_not_count_against_the_app() {
    let (mut store, _, general, now) = ready();
    let message = app_message(&store, general);
    store
        .begin_interaction(1, general, press(message, "rb:confirmar"), now)
        .unwrap();
    store.apply(
        Event::ChallengeRequired {
            challenge: 3,
            failure: Failure::Interaction {
                request: 1,
                channel: general,
            },
        },
        now,
    );
    let later = now + INTERACTION_WAIT + Duration::from_secs(60);
    assert!(
        store.interaction_pending(later).is_some(),
        "still waited for while the person answers"
    );
    assert!(store.interaction_problem(later).is_none());
}

#[test]
fn a_reply_only_the_account_sees_does_not_count_and_can_be_dismissed() {
    let (mut store, _, general, now) = ready();
    let unread = store.read(general).unread();
    let last = store.timeline(general).unwrap().newest().unwrap().clone();
    let mut whisper = Message::clone(&last);
    whisper.id = MessageId(last.id.0 + 1);
    whisper.content = "sólo vos".into();
    whisper.components.ephemeral = true;
    store.apply(
        Event::MessageCreated {
            message: whisper.clone(),
            nonce: None,
        },
        now,
    );
    assert_eq!(store.read(general).unread(), unread);
    assert!(store.timeline(general).unwrap().get(whisper.id).is_some());
    assert!(
        !store.dismiss_ephemeral(general, last.id),
        "only replies like it"
    );
    assert!(store.dismiss_ephemeral(general, whisper.id));
    assert!(store.timeline(general).unwrap().get(whisper.id).is_none());
}

#[test]
fn commands_are_listed_once_and_filtered_by_what_the_account_may_do() {
    let (mut store, provider, general, now) = ready();
    assert_eq!(store.commands_state(general), CommandsState::Unknown);
    let Some(Command::LoadAppCommands { request, channel }) =
        store.load_commands(general, 5, now, false)
    else {
        panic!("asked for")
    };
    assert!(
        store.load_commands(general, 6, now, false).is_none(),
        "loading"
    );
    assert_eq!(store.commands_state(general), CommandsState::Loading);
    // The simulation's answer, as it would arrive.
    let commands = match provider_commands(&provider, request, channel) {
        Event::AppCommands { commands, .. } => commands,
        other => panic!("{other:?}"),
    };
    store.apply(
        Event::AppCommands {
            request: 6,
            channel,
            commands: commands.clone(),
        },
        now,
    );
    assert_eq!(
        store.commands_state(general),
        CommandsState::Loading,
        "not its answer"
    );
    store.apply(
        Event::AppCommands {
            request,
            channel,
            commands,
        },
        now,
    );
    assert_eq!(store.commands_state(general), CommandsState::Loaded);
    let names: Vec<_> = store
        .usable_commands(general)
        .iter()
        .map(|command| command.name.clone())
        .collect();
    assert!(names.contains(&"dado".to_owned()));
    assert!(
        store.load_commands(general, 7, now, false).is_none(),
        "kept"
    );
    assert!(store
        .load_commands(general, 7, now + crate::COMMANDS_KEPT, false)
        .is_some());
    // What a command needs counts: without managing messages, no `/moderar`.
    let moderate = store
        .usable_commands(general)
        .iter()
        .any(|command| command.name == "moderar");
    let manages = store
        .channel(general)
        .unwrap()
        .permissions
        .contains(Permissions::MANAGE_MESSAGES);
    assert_eq!(moderate, manages);
}

#[test]
fn a_command_runs_only_while_listed_and_its_modal_opens_where_it_ran() {
    let (mut store, provider, general, now) = ready();
    let Some(Command::LoadAppCommands { request, channel }) =
        store.load_commands(general, 5, now, false)
    else {
        panic!()
    };
    store.apply(provider_commands(&provider, request, channel), now);
    let form = store
        .usable_commands(general)
        .into_iter()
        .find(|command| command.name == "formulario")
        .unwrap()
        .clone();
    let mut unknown = CommandUse::of(&form, Vec::new(), Vec::new());
    unknown.command = 1;
    assert!(store
        .begin_interaction(8, general, Interaction::Command(unknown), now)
        .is_err());
    let wrong = CommandUse::of(
        &form,
        Vec::new(),
        vec![("x".into(), OptionValue::Integer(1))],
    );
    assert!(
        store
            .begin_interaction(9, general, Interaction::Command(wrong), now)
            .is_err(),
        "checked against its options before it goes"
    );
    let used = CommandUse::of(&form, Vec::new(), Vec::new());
    store
        .begin_interaction(9, general, Interaction::Command(used), now)
        .unwrap();
    let modal = rune_model::interactions::Modal {
        id: 77,
        app: form.app,
        custom_id: "rb:formulario".into(),
        title: "Formulario".into(),
        fields: Vec::new(),
    };
    store.apply(
        Event::ModalOpened {
            request: 9,
            channel: general,
            modal: modal.clone(),
        },
        now,
    );
    assert_eq!(store.modal(), Some((general, &modal)));
    let submit = Interaction::Modal {
        id: 77,
        app: form.app,
        custom_id: "rb:formulario".into(),
        answers: Vec::new(),
    };
    store.begin_interaction(10, general, submit, now).unwrap();
    store.apply(
        Event::Interaction {
            request: 10,
            done: true,
        },
        now,
    );
    assert!(store.modal().is_none(), "closed once answered");
}

/// The simulated app's list, as a session would answer it.
fn provider_commands(
    provider: &MockProvider,
    request: rune_provider::RequestId,
    channel: ChannelId,
) -> Event {
    provider
        .manage(Command::LoadAppCommands { request, channel })
        .remove(0)
}

#[test]
fn a_modal_with_files_needs_to_attach_and_waits_for_its_uploads() {
    use rune_model::interactions::{FieldValue, Modal};
    use rune_model::UploadToken;
    let (mut store, _, general, now) = ready();
    let message = app_message(&store, general);
    // Any part may open a modal.
    store
        .begin_interaction(1, general, press(message, "rb:confirmar"), now)
        .unwrap();
    let app = rune_model::UserId(9);
    store.apply(
        Event::ModalOpened {
            request: 1,
            channel: general,
            modal: Modal {
                id: 70,
                app,
                custom_id: "f".into(),
                title: "Archivos".into(),
                fields: Vec::new(),
            },
        },
        now,
    );
    let answer = Interaction::Modal {
        id: 70,
        app,
        custom_id: "f".into(),
        answers: vec![("a".into(), FieldValue::Files(vec![UploadToken(3)]))],
    };
    assert!(store.interaction_abilities(general).files.allowed());
    store
        .begin_interaction(2, general, answer.clone(), now)
        .unwrap();
    let wait = store.interaction_expires(now).unwrap();
    assert!(wait > INTERACTION_WAIT, "uploads included: {wait:?}");
    store.apply(
        Event::Failed {
            failure: Failure::Interaction {
                request: 2,
                channel: general,
            },
            reason: "no".into(),
            ambiguous: false,
        },
        now,
    );
    // A conversation it only reads still takes a form's files, as Serein's.
    let channel = store.channels.get_mut(&general).unwrap();
    channel.permissions = channel.permissions.without(Permissions::SEND);
    assert!(store.interaction_abilities(general).files.allowed());
    store
        .begin_interaction(3, general, answer.clone(), now)
        .unwrap();
    store.apply(
        Event::Failed {
            failure: Failure::Interaction {
                request: 3,
                channel: general,
            },
            reason: "no".into(),
            ambiguous: false,
        },
        now,
    );
    // Without permission to attach, it does not go.
    let channel = store.channels.get_mut(&general).unwrap();
    channel.permissions = channel.permissions.without(Permissions::ATTACH);
    assert!(!store.interaction_abilities(general).files.allowed());
    assert!(store.begin_interaction(4, general, answer, now).is_err());
}
