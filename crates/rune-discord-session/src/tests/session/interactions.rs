//! Apps through the whole provider (P3): the command index, `POST
//! /interactions` of each type with the Gateway's session, the Gateway's
//! answers by nonce, and the verification a write may need. Local fixtures
//! only: none of it was tried against the service.

use super::*;
use bytes::Bytes;
use rune_model::interactions::{
    CommandKind, CommandUse, FieldValue, Interaction, ModalInput, OptionValue,
};
use rune_model::{PendingAttachment, UploadToken, UserId};
use rune_provider::{Failure, PreparedUpload};

fn body(request: &str) -> Value {
    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
}

/// A message of a bot in the conversation 20, with a button and a menu.
fn bot_message() -> Value {
    json!({"id":"100","channel_id":"20","author":{"id":"44","username":"Bot","bot":true},
        "application_id":"44","content":"","timestamp":"2026-10-14T12:34:56Z",
        "edited_timestamp":null,"attachments":[],"mentions":[],"pinned":false,"flags":0,
        "components":[{"type":1,"components":[
            {"type":2,"style":1,"label":"Sí","custom_id":"ok"}]},
            {"type":1,"components":[{"type":3,"custom_id":"menu","options":[
                {"label":"A","value":"a"},{"label":"B","value":"b"}]}]}]})
}

async fn loaded(app: &mut Harness) {
    app.running.commands.send(rest::history(1)).await.unwrap();
    let (_, reply) = app.pending().await;
    reply
        .send(response(200, &json!([bot_message()]).to_string()))
        .unwrap();
    assert!(matches!(app.next_data().await, Event::HistoryLoaded { .. }));
}

/// Send `interaction` as `request`, and the request that carries it.
async fn interact(app: &mut Harness, request: u64, interaction: Interaction) -> Value {
    app.running
        .commands
        .send(Command::Interact {
            request,
            channel: ChannelId(20),
            interaction,
            files: Vec::new(),
        })
        .await
        .unwrap();
    let (sent, reply) = app.pending().await;
    assert!(sent.starts_with("POST /api/v10/interactions "), "{sent}");
    reply.send(response(204, "")).unwrap();
    body(&sent)
}

#[tokio::test]
async fn a_bots_part_goes_with_the_gateway_session_and_its_answers_come_by_nonce() {
    let mut app = Harness::new().await;
    loaded(&mut app).await;
    let pressed = interact(
        &mut app,
        7,
        Interaction::Part {
            message: MessageId(100),
            custom_id: "ok".into(),
            values: Vec::new(),
        },
    )
    .await;
    assert_eq!(pressed["type"], 3);
    assert_eq!(pressed["session_id"], "synthetic-session");
    assert_eq!(pressed["message_id"], "100");
    assert_eq!(pressed["application_id"], "44");
    assert_eq!(pressed["message_flags"], 0);
    assert_eq!(
        pressed["data"],
        json!({"component_type": 2, "custom_id": "ok"})
    );
    assert!(pressed.get("guild_id").is_none(), "a direct conversation");
    let nonce = pressed["nonce"].as_str().unwrap().to_owned();
    assert!(
        nonce.bytes().all(|b| b.is_ascii_digit()),
        "a snowflake: {nonce}"
    );
    // Another session's answer, or an unknown one, says nothing.
    app.dispatch
        .send(("INTERACTION_SUCCESS", json!({"id": "1", "nonce": "123"})))
        .await
        .unwrap();
    app.dispatch
        .send(("INTERACTION_CREATE", json!({"id": "900", "nonce": nonce})))
        .await
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::Interaction {
            request: 7,
            done: false
        }
    );
    app.dispatch
        .send(("INTERACTION_SUCCESS", json!({"id": "900", "nonce": nonce})))
        .await
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::Interaction {
            request: 7,
            done: true
        }
    );
    // A menu's pick says its kind twice and its values; a failure says why.
    let picked = interact(
        &mut app,
        8,
        Interaction::Part {
            message: MessageId(100),
            custom_id: "menu".into(),
            values: vec!["b".into()],
        },
    )
    .await;
    assert_eq!(
        picked["data"],
        json!({"component_type": 3, "type": 3, "custom_id": "menu", "values": ["b"]})
    );
    app.dispatch
        .send((
            "INTERACTION_FAILURE",
            json!({"id": "901", "nonce": picked["nonce"], "reason_code": 2}),
        ))
        .await
        .unwrap();
    assert!(matches!(app.next_data().await,
        Event::Failed { failure: Failure::Interaction { request: 8, .. }, reason, .. }
            if reason.contains("no respondió a tiempo")));
    // A value the menu does not offer never leaves.
    app.running
        .commands
        .send(Command::Interact {
            request: 9,
            channel: ChannelId(20),
            interaction: Interaction::Part {
                message: MessageId(100),
                custom_id: "menu".into(),
                values: vec!["z".into()],
            },
            files: Vec::new(),
        })
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::Interaction { request: 9, .. },
            ..
        }
    ));
    // A reply only the account sees, text alone, says so.
    let mut whisper = bot_message();
    whisper["id"] = json!("101");
    whisper["flags"] = json!(64);
    whisper["content"] = json!("Sólo para ti");
    whisper.as_object_mut().unwrap().remove("components");
    app.dispatch
        .send(("MESSAGE_CREATE", whisper))
        .await
        .unwrap();
    assert!(matches!(app.next_data().await,
        Event::MessageCreated { message, .. }
            if message.components.ephemeral && message.components.app == Some(UserId(44))));
    // A server's commands changed: a list of them is old.
    app.dispatch
        .send((
            "GUILD_APPLICATION_COMMAND_INDEX_UPDATE",
            json!({"guild_id": "10", "version": "5"}),
        ))
        .await
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::AppCommandsChanged {
            server: Some(rune_model::ServerId(10))
        }
    );
}

#[tokio::test]
async fn a_command_is_listed_suggested_run_and_its_modal_sent_back() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::LoadAppCommands {
            request: 1,
            channel: ChannelId(20),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("GET /api/v10/channels/20/application-command-index "),
        "{request}"
    );
    let index = json!({"applications": [{"id": "44", "name": "Bot"}],
        "application_commands": [{"id": "300", "version": "301", "application_id": "44",
            "type": 1, "name": "dado", "description": "Tira", "contexts": [1],
            "permissions": {"user": true},
            "options": [{"type": 4, "name": "caras", "description": "n", "autocomplete": true},
                {"type": 5, "name": "privado", "description": "p"}]}], "version": "9"});
    reply.send(response(200, &index.to_string())).unwrap();
    let Event::AppCommands {
        request: 1,
        commands,
        ..
    } = app.next_data().await
    else {
        panic!("the commands")
    };
    assert_eq!(commands.len(), 1);
    assert!(commands[0].in_direct && !commands[0].in_servers);
    let dice = commands[0].clone();
    // Suggestions for what is written: the text as written, focused.
    let mut asked = CommandUse::of(
        &dice,
        Vec::new(),
        vec![("privado".into(), OptionValue::Boolean(true))],
    );
    asked.focused = Some(("caras".into(), "1".into()));
    app.running
        .commands
        .send(Command::SuggestChoices {
            request: 2,
            channel: ChannelId(20),
            command: asked,
        })
        .await
        .unwrap();
    let (sent, reply) = app.pending().await;
    reply.send(response(204, "")).unwrap();
    let suggest = body(&sent);
    assert_eq!(suggest["type"], 4);
    assert!(suggest.get("analytics_location").is_none());
    assert_eq!(
        suggest["data"]["options"],
        json!([{"type": 5, "name": "privado", "value": true},
            {"type": 4, "name": "caras", "value": "1", "focused": true}])
    );
    assert!(suggest["data"]["application_command"]
        .get("permissions")
        .is_none());
    app.dispatch
        .send((
            "APPLICATION_COMMAND_AUTOCOMPLETE_RESPONSE",
            json!({"nonce": suggest["nonce"], "choices": [{"name": "Diez", "value": 10}]}),
        ))
        .await
        .unwrap();
    let Event::CommandChoices {
        request: 2,
        choices,
    } = app.next_data().await
    else {
        panic!("the suggestions")
    };
    assert_eq!(choices[0].value, OptionValue::Integer(10));
    // The command runs with its version and definition.
    let used = CommandUse::of(
        &dice,
        Vec::new(),
        vec![("caras".into(), OptionValue::Integer(10))],
    );
    let run = interact(&mut app, 3, Interaction::Command(used)).await;
    assert_eq!(run["type"], 2);
    assert_eq!(run["analytics_location"], "slash_ui");
    assert_eq!(run["application_id"], "44");
    assert_eq!(run["data"]["id"], "300");
    assert_eq!(run["data"]["version"], "301");
    assert_eq!(run["data"]["name"], "dado");
    assert_eq!(
        run["data"]["options"],
        json!([{"type": 4, "name": "caras", "value": 10}])
    );
    assert_eq!(run["data"]["attachments"], json!([]));
    // Its answer is a modal, sent back as it came.
    app.dispatch
        .send((
            "INTERACTION_MODAL_CREATE",
            json!({"id": "777", "nonce": run["nonce"], "channel_id": "20", "custom_id": "form",
                "title": "Formulario", "application": {"id": "44"}, "components": [
                {"type": 18, "label": "Nombre", "component": {"type": 4, "custom_id": "name",
                    "style": 1, "required": true}}]}),
        ))
        .await
        .unwrap();
    let Event::ModalOpened {
        request: 3, modal, ..
    } = app.next_data().await
    else {
        panic!("the modal")
    };
    assert_eq!(modal.title, "Formulario");
    assert_eq!(
        app.next_data().await,
        Event::Interaction {
            request: 3,
            done: true
        }
    );
    let sent = interact(
        &mut app,
        4,
        Interaction::Modal {
            id: 777,
            app: UserId(44),
            custom_id: "form".into(),
            answers: vec![("name".into(), FieldValue::Text("Ada".into()))],
        },
    )
    .await;
    assert_eq!(sent["type"], 5);
    assert_eq!(
        sent["data"],
        json!({"id": "777", "custom_id": "form", "components": [
            {"type": 18, "component": {"type": 4, "custom_id": "name", "value": "Ada"}}]})
    );
}

#[tokio::test]
async fn an_interaction_waits_for_the_persons_verification_and_goes_once_more() {
    let mut app = Harness::new().await;
    loaded(&mut app).await;
    let press = Command::Interact {
        request: 7,
        channel: ChannelId(20),
        interaction: Interaction::Part {
            message: MessageId(100),
            custom_id: "ok".into(),
            values: Vec::new(),
        },
        files: Vec::new(),
    };
    app.running.commands.send(press).await.unwrap();
    let (first, reply) = app.pending().await;
    reply
        .send(response(400, super::challenges::CHALLENGE))
        .unwrap();
    let Event::ChallengeRequired { challenge, failure } = app.next_data().await else {
        panic!("the verification")
    };
    assert_eq!(
        failure,
        Failure::Interaction {
            request: 7,
            channel: ChannelId(20)
        }
    );
    super::challenges::solve(&mut app, challenge).await;
    let (again, reply) = app.pending().await;
    super::challenges::answered(&again);
    assert_eq!(
        body(&again)["nonce"],
        body(&first)["nonce"],
        "the same interaction"
    );
    reply.send(response(204, "")).unwrap();
}

#[tokio::test]
async fn a_modals_files_are_staged_then_named_by_their_index() {
    let mut app = Harness::new().await;
    loaded(&mut app).await;
    let pressed = interact(
        &mut app,
        7,
        Interaction::Part {
            message: MessageId(100),
            custom_id: "ok".into(),
            values: Vec::new(),
        },
    )
    .await;
    app.dispatch
        .send((
            "INTERACTION_MODAL_CREATE",
            json!({"id": "778", "nonce": pressed["nonce"], "channel_id": "20",
                "custom_id": "form", "title": "Archivos", "application": {"id": "44"},
                "components": [{"type": 18, "label": "Fotos", "component": {"type": 19,
                    "custom_id": "pics", "min_values": 1, "max_values": 2,
                    "file_types": ["image", ".pdf"]}}]}),
        ))
        .await
        .unwrap();
    let Event::ModalOpened { modal, .. } = app.next_data().await else {
        panic!("the modal")
    };
    assert_eq!(
        modal.fields[0].input,
        ModalInput::Files {
            custom_id: "pics".into(),
            required: true,
            min: 1,
            max: 2,
            file_types: vec!["image".into(), ".pdf".into()],
        }
    );
    assert!(matches!(
        app.next_data().await,
        Event::Interaction {
            request: 7,
            done: true
        }
    ));
    let file = |token: u64, name: &str| {
        PreparedUpload::new(
            PendingAttachment {
                token: UploadToken(token),
                filename: name.into(),
                size_bytes: 4,
            },
            Bytes::from_static(b"hola"),
        )
        .unwrap()
    };
    let carrying = |request, named: Vec<u64>, files| Command::Interact {
        request,
        channel: ChannelId(20),
        interaction: Interaction::Modal {
            id: 778,
            app: UserId(44),
            custom_id: "form".into(),
            answers: vec![(
                "pics".into(),
                FieldValue::Files(named.into_iter().map(UploadToken).collect()),
            )],
        },
        files,
    };
    let sent = |request, named| carrying(request, named, vec![file(1, "a.png"), file(2, "b.png")]);
    // A file the modal does not name never leaves, nor does anything else;
    // nor more than its field takes, nor one of a kind it does not list.
    let refused = [
        sent(8, vec![1]),
        carrying(
            10,
            vec![1, 2, 3],
            vec![file(1, "a.png"), file(2, "b.png"), file(3, "c.pdf")],
        ),
        carrying(11, vec![4], vec![file(4, "programa.exe")]),
    ];
    for command in refused {
        app.running.commands.send(command).await.unwrap();
        assert!(matches!(
            app.next_data().await,
            Event::Failed {
                failure: Failure::Interaction { .. },
                ..
            }
        ));
    }
    assert!(app.requests.try_recv().is_err(), "nothing left");
    // Each file is staged in the conversation, then the modal names them.
    app.running
        .commands
        .send(sent(9, vec![2, 1]))
        .await
        .unwrap();
    for index in 0..2 {
        let (request, reply) = app.pending().await;
        assert!(
            request.starts_with("POST /api/v10/channels/20/attachments "),
            "{request}"
        );
        let host = request
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .expect("loopback host");
        reply
            .send(response(
                200,
                &json!({"attachments": [{"id": index.to_string(),
                    "upload_url": format!("http://{host}/storage/{index}"),
                    "upload_filename": format!("synthetic/{index}")}]})
                .to_string(),
            ))
            .unwrap();
        let (request, reply) = app.pending().await;
        assert!(request.starts_with(&format!("PUT /storage/{index} ")));
        assert!(!request.contains(SECRET));
        reply.send(response(200, "")).unwrap();
    }
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("POST /api/v10/interactions "),
        "{request}"
    );
    reply.send(response(204, "")).unwrap();
    let sent = body(&request);
    assert_eq!(sent["type"], 5);
    assert_eq!(
        sent["data"]["components"],
        json!([{"type": 18, "component": {"type": 19, "custom_id": "pics",
            "values": ["1", "0"]}}])
    );
    assert_eq!(
        sent["data"]["attachments"],
        json!([{"id": "0", "filename": "a.png", "uploaded_filename": "synthetic/0"},
            {"id": "1", "filename": "b.png", "uploaded_filename": "synthetic/1"}])
    );
}

#[tokio::test]
async fn a_modals_files_need_permission_to_attach_in_the_channel() {
    let mut app = Harness::new().await;
    // It sees and writes in the server's conversation 30, but attaches nothing.
    super::message_recovery::guild_channel(&mut app, (1 << 10) | (1 << 11)).await;
    let file = PreparedUpload::new(
        PendingAttachment {
            token: UploadToken(1),
            filename: "a.png".into(),
            size_bytes: 4,
        },
        Bytes::from_static(b"hola"),
    )
    .unwrap();
    app.running
        .commands
        .send(Command::Interact {
            request: 5,
            channel: ChannelId(30),
            interaction: Interaction::Modal {
                id: 778,
                app: UserId(44),
                custom_id: "form".into(),
                answers: vec![("pics".into(), FieldValue::Files(vec![UploadToken(1)]))],
            },
            files: vec![file],
        })
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed { failure: Failure::Interaction { request: 5, .. }, reason, .. }
            if reason.contains("adjuntar archivos")
    ));
    assert!(app.requests.try_recv().is_err(), "nothing was staged");
}

#[tokio::test]
async fn a_user_command_is_listed_by_its_kind_and_runs_on_a_person() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::LoadAppCommands {
            request: 1,
            channel: ChannelId(20),
        })
        .await
        .unwrap();
    let (_, reply) = app.pending().await;
    // Types 1 (chat), 2 (on a person) and 3 (on a message) are listed; 4 is not.
    let index = json!({"applications": [{"id": "44", "name": "Bot"}],
        "application_commands": [
            {"id": "300", "version": "301", "application_id": "44", "type": 1,
                "name": "dado", "description": "Tira", "contexts": [1]},
            {"id": "310", "version": "311", "application_id": "44", "type": 2,
                "name": "Saludar", "description": "", "contexts": [1]},
            {"id": "320", "version": "321", "application_id": "44", "type": 3,
                "name": "Traducir", "description": "", "contexts": [1]},
            {"id": "330", "version": "331", "application_id": "44", "type": 4,
                "name": "Otro", "description": "", "contexts": [1]}],
        "version": "9"});
    reply.send(response(200, &index.to_string())).unwrap();
    let Event::AppCommands {
        request: 1,
        commands,
        ..
    } = app.next_data().await
    else {
        panic!("the commands")
    };
    let kinds: Vec<(u64, CommandKind)> = commands.iter().map(|c| (c.id, c.kind)).collect();
    assert_eq!(
        kinds,
        [
            (300, CommandKind::ChatInput),
            (310, CommandKind::User),
            (320, CommandKind::Message)
        ]
    );
    // Run on a person: type 2 with the person as its target, and no options.
    let greet = commands[1].clone();
    let used = CommandUse {
        target: Some(UserId(55)),
        ..CommandUse::of(&greet, Vec::new(), Vec::new())
    };
    let run = interact(&mut app, 2, Interaction::Command(used)).await;
    assert_eq!(run["type"], 2);
    assert_eq!(run["application_id"], "44");
    assert_eq!(run["channel_id"], "20");
    assert_eq!(run["data"]["type"], 2);
    assert_eq!(run["data"]["target_id"], "55");
    assert_eq!(run["data"]["id"], "310");
    assert_eq!(run["data"]["version"], "311");
    assert_eq!(run["data"]["name"], "Saludar");
    assert!(run["data"].get("options").is_none(), "{run}");
    assert!(run.get("analytics_location").is_none(), "{run}");
}
