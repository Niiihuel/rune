//! Apps' parts, modals and commands (P3), over the simulation's RuneBot.

use rune_components::interactions::{command_ids, modal_ids};
use rune_components::message::app_ids;
use rune_model::interactions::{
    FieldValue, Interaction, Modal, ModalField, ModalInput, OptionValue,
};
use rune_model::{Message, MessageId};

use super::*;

/// The IDs of what can be pressed in `frame`.
fn controls(frame: &Frame) -> Vec<String> {
    frame
        .regions
        .iter()
        .filter(|region| region.kind == limn::HitKind::Button)
        .map(|region| region.id.clone())
        .collect()
}

impl Host {
    fn p3_act(&mut self, action: UiAction) {
        let intents = update(&mut self.ui, &mut self.store, &action, self.now);
        self.run(intents);
    }

    fn clicked(&mut self, id: impl Into<String>) {
        self.p3_act(UiAction::Clicked { id: id.into() });
    }

    /// RuneBot's newest message whose parts say `custom_id`.
    fn app_message(&self, channel: ChannelId, custom_id: &str) -> Message {
        self.store
            .timeline(channel)
            .unwrap()
            .messages()
            .iter()
            .rev()
            .find(|message| {
                rune_model::components::control(&message.components.parts, custom_id).is_some()
            })
            .map(|message| Message::clone(message))
            .expect("RuneBot's message is loaded")
    }

    /// The last interaction sent.
    fn interacted(&self) -> Option<(rune_provider::RequestId, Interaction)> {
        self.sent.iter().rev().find_map(|command| match command {
            Command::Interact {
                request,
                interaction,
                ..
            } => Some((*request, interaction.clone())),
            _ => None,
        })
    }

    /// The simulation's commands, answered as its session would.
    fn list_commands(&mut self) {
        let asked = self.sent.iter().rev().find_map(|command| match command {
            Command::LoadAppCommands { request, channel } => Some((*request, *channel)),
            _ => None,
        });
        let (request, channel) = asked.expect("the commands were asked for");
        for event in self
            .provider
            .manage(Command::LoadAppCommands { request, channel })
        {
            self.event(event);
        }
    }
}

#[test]
fn a_button_tells_its_app_waits_for_it_and_its_private_reply_can_be_dismissed() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let app = host.app_message(general, "rb:confirmar");
    let button = app_ids::part(app.id, "rb:confirmar");
    assert!(
        controls(&host.frame()).contains(&button),
        "the button can be pressed"
    );
    host.clicked(button.clone());
    let (request, interaction) = host.interacted().expect("it went to the app");
    assert_eq!(
        interaction,
        Interaction::Part {
            message: app.id,
            custom_id: "rb:confirmar".into(),
            values: Vec::new()
        }
    );
    host.event(Event::Interaction {
        request,
        done: false,
    });
    assert!(host.painted().iter().any(|said| said.contains("pensando")));
    assert!(
        !controls(&host.frame()).contains(&button),
        "nothing else goes while it waits"
    );
    let mut whisper = app.clone();
    whisper.id = MessageId(u64::MAX / 2);
    whisper.content = "¡Anotado! (simulado)".into();
    whisper.components = rune_model::Components {
        app: app.components.app,
        ephemeral: true,
        flags: 64,
        ..Default::default()
    };
    host.event(Event::MessageCreated {
        message: whisper.clone(),
        nonce: None,
    });
    host.event(Event::Interaction {
        request,
        done: true,
    });
    host.tree.scroll_to_end(&timeline_id(general));
    let painted = host.painted().join(" ");
    assert!(painted.contains("Sólo tú puedes ver esto"), "{painted}");
    host.clicked(app_ids::dismiss(whisper.id));
    assert!(host
        .store
        .timeline(general)
        .unwrap()
        .get(whisper.id)
        .is_none());
}

#[test]
fn a_refusal_shows_beside_the_parts_that_failed() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let app = host.app_message(general, "rb:no-puedo");
    host.clicked(app_ids::part(app.id, "rb:no-puedo"));
    let (request, _) = host.interacted().unwrap();
    host.event(Event::Failed {
        failure: rune_provider::Failure::Interaction {
            request,
            channel: general,
        },
        reason: "Esta interacción falló (simulado)".into(),
        ambiguous: false,
    });
    assert!(host.ui.notices.is_empty(), "said in place, not in a corner");
    assert!(host
        .painted()
        .iter()
        .any(|said| said.contains("Esta interacción falló")));
}

#[test]
fn menus_send_one_choice_at_once_and_several_when_they_close() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let schedule = host.app_message(general, "rb:horario");
    host.clicked(app_ids::part(schedule.id, "rb:horario"));
    host.clicked(app_ids::pick(schedule.id, 1, "rb:horario"));
    let (request, picked) = host.interacted().unwrap();
    assert_eq!(
        picked,
        Interaction::Part {
            message: schedule.id,
            custom_id: "rb:horario".into(),
            values: vec!["viernes".into()]
        }
    );
    host.event(Event::Interaction {
        request,
        done: true,
    });
    // Several people: picked and unpicked, then sent as the menu closes.
    let people = host.app_message(general, "rb:personas");
    let sent = host.sent.len();
    host.clicked(app_ids::part(people.id, "rb:personas"));
    let offered = host.ui.apps.menu_options();
    assert!(!offered.is_empty(), "people of the server");
    host.clicked(app_ids::pick(people.id, 0, "rb:personas"));
    host.clicked(app_ids::pick(people.id, 1, "rb:personas"));
    assert_eq!(host.sent.len(), sent, "nothing goes while it is open");
    host.clicked(app_ids::close(people.id, "rb:personas"));
    let (_, picked) = host.interacted().unwrap();
    let Interaction::Part { values, .. } = picked else {
        panic!()
    };
    assert!(!values.is_empty() && values.len() <= 3, "{values:?}");
}

#[test]
fn a_modal_checks_its_fields_before_it_goes_and_closes_once_answered() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let app = host.app_message(general, "rb:formulario");
    host.clicked(app_ids::part(app.id, "rb:formulario"));
    let (request, _) = host.interacted().unwrap();
    let modal = Modal {
        id: 500,
        app: app.components.app.unwrap(),
        custom_id: "rb:formulario".into(),
        title: "Inscripción al taller (simulado)".into(),
        fields: vec![
            ModalField {
                label: Some("Nombre".into()),
                description: None,
                input: ModalInput::TextInput {
                    custom_id: "nombre".into(),
                    long: false,
                    required: true,
                    min_length: Some(2),
                    max_length: Some(40),
                    value: None,
                    placeholder: None,
                },
            },
            ModalField {
                label: Some("Avisos".into()),
                description: None,
                input: ModalInput::Check {
                    custom_id: "avisos".into(),
                    value: false,
                },
            },
        ],
    };
    host.event(Event::ModalOpened {
        request,
        channel: general,
        modal,
    });
    assert!(host
        .painted()
        .iter()
        .any(|said| said == "Inscripción al taller (simulado)"));
    let before = host.sent.len();
    host.clicked(modal_ids::SEND);
    assert_eq!(host.sent.len(), before, "the name is missing");
    assert!(host
        .painted()
        .iter()
        .any(|said| said == "Completa este campo"));
    host.p3_act(UiAction::InputChanged {
        id: modal_ids::text("nombre"),
        value: "Ada".into(),
    });
    host.clicked(modal_ids::check("avisos"));
    host.clicked(modal_ids::SEND);
    let (sent, interaction) = host.interacted().unwrap();
    let Interaction::Modal { id, answers, .. } = interaction else {
        panic!("{interaction:?}")
    };
    assert_eq!(id, 500);
    assert!(answers.contains(&("nombre".into(), FieldValue::Text("Ada".into()))));
    assert!(answers.contains(&("avisos".into(), FieldValue::Check(true))));
    host.event(Event::Interaction {
        request: sent,
        done: true,
    });
    assert!(host.store.modal().is_none());
}

#[test]
fn slash_offers_runes_and_the_apps_commands_and_runs_a_local_one() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.type_text("/");
    host.list_commands();
    let offered =
        crate::commands::completions(&host.ui, &host.store, general).expect("commands are offered");
    let labels: Vec<_> = offered
        .items
        .iter()
        .map(|item| item.label.as_str())
        .collect();
    for name in ["/shrug", "/me", "/msg", "/dado", "/config avisos activar"] {
        assert!(labels.contains(&name), "{name} in {labels:?}");
    }
    assert!(offered
        .items
        .iter()
        .any(|item| item.section.as_deref() == Some("RUNEBOT (SIMULADO)")));
    host.type_text("shr");
    host.key(Key::Tab, false, false);
    host.frame();
    assert_eq!(host.tree.input_value(&editor_id(general)), Some("/shrug "));
    host.type_text("bueno");
    host.key(Key::Enter, false, false);
    let content = host.sent.iter().rev().find_map(|command| match command {
        Command::SendMessage { content, .. } => Some(content.clone()),
        _ => None,
    });
    assert_eq!(content.as_deref(), Some(r"bueno ¯\\\_(ツ)\_/¯"));
}

#[test]
fn local_commands_shape_the_message_and_say_what_they_need() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    for (written, sent) in [
        ("/me saluda", Some("*saluda*")),
        ("/spoiler el final", Some("||el final||")),
        ("/tableflip", Some("(╯°□°)╯︵ ┻━┻")),
        ("/me", None),
    ] {
        let before = host.sent.len();
        host.p3_act(UiAction::Submitted {
            id: editor_id(general),
            value: written.into(),
        });
        let content = host.sent[before..]
            .iter()
            .find_map(|command| match command {
                Command::SendMessage { content, .. } => Some(content.clone()),
                _ => None,
            });
        assert_eq!(content.as_deref(), sent, "{written}");
    }
    // `/gif` opens the picker on its tab.
    host.p3_act(UiAction::Submitted {
        id: editor_id(general),
        value: "/gif gatos".into(),
    });
    assert!(host.ui.emoji_open);
    assert_eq!(
        host.ui.messages.expression_tab,
        rune_components::emoji::PickerTab::Gif
    );
}

#[test]
fn an_apps_command_checks_its_options_and_goes_with_them() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.type_text("/");
    host.list_commands();
    let reminder = crate::commands::completions(&host.ui, &host.store, general)
        .unwrap()
        .items
        .into_iter()
        .find(|item| item.label == "/recordatorio")
        .unwrap();
    host.clicked(format!(
        "{}{}",
        rune_components::composer::ids::MENTION_PICK,
        reminder.insert
    ));
    let painted = host.painted().join(" ");
    assert!(
        painted.contains("/recordatorio") && painted.contains("texto"),
        "{painted}"
    );
    let before = host.sent.len();
    host.clicked(command_ids::SEND);
    assert_eq!(host.sent.len(), before, "what it requires is missing");
    assert!(host
        .painted()
        .iter()
        .any(|said| said == "Completa este campo"));
    host.p3_act(UiAction::InputChanged {
        id: command_ids::option("texto"),
        value: "regar las plantas".into(),
    });
    host.clicked(command_ids::menu("en"));
    host.clicked(command_ids::pick(1, "en"));
    host.clicked(command_ids::SEND);
    let (_, interaction) = host.interacted().expect("it went");
    let Interaction::Command(used) = interaction else {
        panic!()
    };
    assert_eq!(used.name, "recordatorio");
    assert!(used
        .values
        .contains(&("en".into(), OptionValue::Integer(60))));
    assert!(used.values.contains(&(
        "texto".into(),
        OptionValue::Text("regar las plantas".into())
    )));
    assert_eq!(host.tree.input_value(&editor_id(general)), Some(""));
}

#[test]
fn an_option_the_app_suggests_is_asked_for_as_it_is_written() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.type_text("/");
    host.list_commands();
    let search = crate::commands::completions(&host.ui, &host.store, general)
        .unwrap()
        .items
        .into_iter()
        .find(|item| item.label == "/buscar")
        .unwrap();
    host.clicked(format!(
        "{}{}",
        rune_components::composer::ids::MENTION_PICK,
        search.insert
    ));
    host.p3_act(UiAction::InputChanged {
        id: command_ids::option("tema"),
        value: "re".into(),
    });
    let asked = host.sent.iter().rev().find_map(|command| match command {
        Command::SuggestChoices {
            request, command, ..
        } => Some((*request, command.clone())),
        _ => None,
    });
    let (request, used) = asked.expect("the app is asked");
    assert_eq!(used.focused, Some(("tema".into(), "re".into())));
    host.event(Event::CommandChoices {
        request,
        choices: vec![rune_model::interactions::OptionChoice {
            name: "recetas (simulado)".into(),
            value: OptionValue::Text("recetas".into()),
        }],
    });
    assert!(host
        .painted()
        .iter()
        .any(|said| said == "recetas (simulado)"));
    host.clicked(command_ids::pick(0, "tema"));
    host.clicked(command_ids::SEND);
    let (_, Interaction::Command(used)) = host.interacted().unwrap() else {
        panic!()
    };
    assert_eq!(
        used.values,
        [("tema".to_owned(), OptionValue::Text("recetas".into()))]
    );
}

#[test]
fn an_answer_to_a_command_says_who_ran_which() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let app = host.app_message(general, "rb:confirmar");
    let mut answer = app.clone();
    answer.id = MessageId(u64::MAX / 2);
    answer.content = "🎲 Salió 4 (simulado)".into();
    answer.components = rune_model::Components {
        app: app.components.app,
        invoked: Some(rune_model::components::Invoked {
            user: host.store.me().unwrap(),
            command: "dado".into(),
        }),
        ..Default::default()
    };
    host.event(Event::MessageCreated {
        message: answer,
        nonce: None,
    });
    host.tree.scroll_to_end(&timeline_id(general));
    let painted = host.painted().join(" ");
    assert!(painted.contains("usó /dado"), "{painted}");
}

#[test]
fn a_modals_files_are_chosen_apart_from_the_draft_go_with_it_and_are_let_go() {
    use rune_model::{PendingAttachment, UploadToken};
    let mut host = Host::new();
    let general = host.open_channel("general");
    let app = host.app_message(general, "rb:formulario");
    host.clicked(app_ids::part(app.id, "rb:formulario"));
    let (request, _) = host.interacted().unwrap();
    let modal = Modal {
        id: 501,
        app: app.components.app.unwrap(),
        custom_id: "rb:formulario".into(),
        title: "Archivos (simulado)".into(),
        fields: vec![ModalField {
            label: Some("Fotos".into()),
            description: None,
            input: ModalInput::Files {
                custom_id: "fotos".into(),
                required: true,
                min: 1,
                max: 2,
                file_types: vec!["image".into()],
            },
        }],
    };
    host.event(Event::ModalOpened {
        request,
        channel: general,
        modal,
    });
    host.clicked(modal_ids::SEND);
    assert!(host
        .painted()
        .iter()
        .any(|said| said == "Adjunta un archivo"));
    let me = host.store.me().unwrap();
    let choose = |host: &mut Host, token: u64| {
        host.clicked(modal_ids::files("fotos"));
        let Some(Intent::PickAttachment { request, channel }) = host.upload_intents.last().cloned()
        else {
            panic!("{:?}", host.upload_intents);
        };
        assert_eq!(channel, general);
        let intents = crate::upload_update(
            &mut host.ui,
            &host.store,
            rune_provider::UploadUpdate::Prepared {
                account: me,
                channel,
                request,
                result: Ok(vec![PendingAttachment {
                    token: UploadToken(token),
                    filename: format!("foto-{token}.png"),
                    size_bytes: 2048,
                }]),
            },
        );
        host.run(intents);
    };
    choose(&mut host, 77);
    // In the modal, apart from the conversation's draft.
    assert!(host.painted().iter().any(|said| said == "foto-77.png"));
    assert!(host
        .ui
        .uploads
        .drafts
        .get(&general)
        .is_none_or(Vec::is_empty));
    let sent = host.sent.len();
    host.clicked(modal_ids::SEND);
    assert_eq!(host.sent.len(), sent, "the files go through the services");
    let Some(Intent::InteractWithFiles {
        command:
            Command::Interact {
                request,
                interaction,
                ..
            },
        files,
    }) = host.upload_intents.last().cloned()
    else {
        panic!("{:?}", host.upload_intents);
    };
    assert_eq!(files, [UploadToken(77)]);
    assert_eq!(interaction.files(), files);
    // Refused: the modal says why and keeps its file for another try.
    host.event(Event::Failed {
        failure: rune_provider::Failure::Interaction {
            request,
            channel: general,
        },
        reason: "Ocupado".into(),
        ambiguous: false,
    });
    let painted = host.painted();
    assert!(painted.iter().any(|said| said == "Ocupado"));
    assert!(painted.iter().any(|said| said == "foto-77.png"));
    // Taken out, it is let go; another, chosen, goes when it closes.
    host.clicked(modal_ids::unfile(77, "fotos"));
    let removed = |token| Intent::RemoveAttachment {
        channel: general,
        token: UploadToken(token),
    };
    assert!(host.upload_intents.contains(&removed(77)));
    choose(&mut host, 78);
    host.clicked(modal_ids::CLOSE);
    assert!(host.upload_intents.contains(&removed(78)));
    assert!(host.store.modal().is_none());
}

#[test]
fn a_modals_files_of_another_kind_stay_out_and_answered_they_are_let_go() {
    use rune_model::{PendingAttachment, UploadToken};
    let mut host = Host::new();
    let general = host.open_channel("general");
    let app = host.app_message(general, "rb:formulario");
    host.clicked(app_ids::part(app.id, "rb:formulario"));
    let (request, _) = host.interacted().unwrap();
    host.event(Event::ModalOpened {
        request,
        channel: general,
        modal: Modal {
            id: 502,
            app: app.components.app.unwrap(),
            custom_id: "rb:formulario".into(),
            title: "Archivos (simulado)".into(),
            fields: vec![ModalField {
                label: Some("Fotos".into()),
                description: None,
                input: ModalInput::Files {
                    custom_id: "fotos".into(),
                    required: true,
                    min: 1,
                    max: 1,
                    file_types: vec!["image".into()],
                },
            }],
        },
    });
    let me = host.store.me().unwrap();
    let choose = |host: &mut Host, token: u64, filename: &str| {
        host.clicked(modal_ids::files("fotos"));
        let Some(Intent::PickAttachment { request, channel }) = host.upload_intents.last().cloned()
        else {
            panic!("{:?}", host.upload_intents);
        };
        let intents = crate::upload_update(
            &mut host.ui,
            &host.store,
            rune_provider::UploadUpdate::Prepared {
                account: me,
                channel,
                request,
                result: Ok(vec![PendingAttachment {
                    token: UploadToken(token),
                    filename: filename.into(),
                    size_bytes: 2048,
                }]),
            },
        );
        host.run(intents);
    };
    let removed = |token| Intent::RemoveAttachment {
        channel: general,
        token: UploadToken(token),
    };
    // Not a kind the field takes: said, and let go at once.
    choose(&mut host, 80, "programa.exe");
    assert!(host.upload_intents.contains(&removed(80)));
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title.contains("programa.exe")));
    choose(&mut host, 81, "foto.png");
    assert!(host.painted().iter().any(|said| said == "foto.png"));
    host.clicked(modal_ids::SEND);
    let Some(Intent::InteractWithFiles {
        command: Command::Interact { request, .. },
        ..
    }) = host.upload_intents.last().cloned()
    else {
        panic!("{:?}", host.upload_intents);
    };
    // On their way, the files reach the app anyway: it is not cancelled.
    host.clicked(modal_ids::CLOSE);
    assert!(host.store.modal().is_some());
    assert!(!host.upload_intents.contains(&removed(81)));
    // The app answered: the modal closes and its files are let go.
    host.event(Event::Interaction {
        request,
        done: true,
    });
    assert!(host.store.modal().is_none());
    assert!(host.upload_intents.contains(&removed(81)));
}
