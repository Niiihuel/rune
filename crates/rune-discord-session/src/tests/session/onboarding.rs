//! A server's rules and onboarding through the session's checked queue,
//! against local fixtures only: what leaves for each route, what comes
//! back, and the hCaptcha each write may meet. Nothing here reached Discord.

use super::*;
use rune_model::{ChoiceId, JoinOutcome, RulesAnswer, ServerId};

const SERVER: ServerId = ServerId(10);

fn body(request: &str) -> Value {
    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
}

/// A darkroom community the account just joined: it waits on the rules
/// (`pending`) and has not answered the questions. `owner` makes it the
/// account's own.
async fn joined(app: &mut Harness, owner: &str, pending: bool) {
    app.dispatch
        .send((
            "GUILD_CREATE",
            json!({"id": "10", "name": "Cuarto oscuro", "owner_id": owner,
                "features": ["GUILD_ONBOARDING", "COMMUNITY", "MEMBER_VERIFICATION_GATE_ENABLED"],
                "roles": [
                    {"id": "10", "name": "@everyone", "permissions": (1u64 << 10 | 1 << 11).to_string(), "position": 0},
                    {"id": "12", "name": "Ampliadora", "permissions": "0", "position": 1}],
                "members": [{"user": {"id": "42", "username": "Owner"}, "roles": [],
                    "pending": pending, "flags": 0}],
                "channels": [
                    {"id": "30", "guild_id": "10", "type": 0, "name": "mesada", "permission_overwrites": []},
                    {"id": "32", "guild_id": "10", "type": 0, "name": "copias", "permission_overwrites": []}]}),
        ))
        .await
        .unwrap();
    loop {
        if let Event::ServerChanged(server) = app.running.next().await {
            assert!(server.options.rules_screening && server.options.onboarding);
            break;
        }
    }
    // Its channels come after it; once the last is in, so is the member.
    loop {
        if matches!(app.running.next().await, Event::ChannelChanged(channel) if channel.id == ChannelId(32))
        {
            break;
        }
    }
}

/// The darkroom's form: its rules to agree to, with a key RUNE does not
/// model, which goes back as it came.
const FORM: &str = r#"{"description": "Lee las normas antes de entrar a la mesada.",
    "version": "2026-10-01T09:30:00.000000+00:00",
    "form_fields": [{"required": true, "field_type": "TERMS", "label": "Normas del cuarto oscuro",
        "values": ["Luz roja siempre encendida", "Devuelve los químicos a su estante"],
        "description": null, "group": 1}]}"#;

/// The darkroom's onboarding: one question whose option gives a role and
/// opens #copias.
const ONBOARDING: &str = r#"{"enabled": true, "mode": 0, "guild_id": "10",
    "default_channel_ids": ["30"], "responses": [],
    "prompts": [{"options": [{"title": "Copias en papel", "id": "1290000000000000201",
        "channel_ids": ["32"], "role_ids": ["12"], "description": "Turnos de ampliadora",
        "emoji": {"name": "🎞️", "id": null}}],
        "title": "¿Qué quieres hacer?", "id": "1290000000000000200", "type": 0,
        "required": true, "single_select": true, "in_onboarding": true}]}"#;

fn form() -> rune_model::ServerRules {
    crate::onboarding::rules(&serde_json::from_str(FORM).unwrap())
        .unwrap()
        .unwrap()
}

fn darkroom_onboarding() -> rune_model::Onboarding {
    crate::onboarding::onboarding(&serde_json::from_str(ONBOARDING).unwrap()).unwrap()
}

#[tokio::test]
async fn the_rules_and_the_questions_are_read_and_answered_by_their_routes() {
    let mut app = Harness::new().await;
    joined(&mut app, "99", true).await;
    app.running
        .commands
        .send(Command::LoadServerRules {
            request: 1,
            server: SERVER,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("GET /api/v10/guilds/10/member-verification?with_guild=false "),
        "{request}"
    );
    reply.send(response(200, FORM)).unwrap();
    let Event::ServerRules {
        request: 1,
        rules: Some(rules),
        ..
    } = app.next_data().await
    else {
        panic!("the rules")
    };
    assert_eq!(rules.fields[0].rules.len(), 2);
    app.running
        .commands
        .send(Command::LoadOnboarding {
            request: 2,
            server: SERVER,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("GET /api/v10/guilds/10/onboarding "),
        "{request}"
    );
    reply.send(response(200, ONBOARDING)).unwrap();
    let Event::OnboardingLoaded {
        request: 2,
        onboarding: Some(onboarding),
        ..
    } = app.next_data().await
    else {
        panic!("the onboarding")
    };
    let prints = ChoiceId(1_290_000_000_000_000_201);
    assert_eq!(onboarding.prompts[0].options[0].channels, [ChannelId(32)]);
    // The questions first, every one asked counted as seen.
    app.running
        .commands
        .send(Command::AnswerOnboarding {
            request: 3,
            server: SERVER,
            onboarding: onboarding.clone(),
            chosen: vec![prints],
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("POST /api/v10/guilds/10/onboarding-responses "),
        "{request}"
    );
    let sent = body(&request);
    assert_eq!(sent["onboarding_responses"], json!(["1290000000000000201"]));
    assert!(sent["onboarding_prompts_seen"]["1290000000000000200"].is_u64());
    assert!(sent["onboarding_responses_seen"]["1290000000000000201"].is_u64());
    reply
        .send(response(
            200,
            r#"{"onboarding_responses":["1290000000000000201"]}"#,
        ))
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::OnboardingAnswered {
            request: 3,
            server: SERVER,
            chosen: vec![prints],
        }
    );
    // Then the rules: the field as it came, with its answer.
    app.running
        .commands
        .send(Command::AnswerServerRules {
            request: 4,
            server: SERVER,
            rules: rules.clone(),
            answers: vec![RulesAnswer::Agreed(true)],
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("PUT /api/v10/guilds/10/requests/@me "),
        "{request}"
    );
    let sent = body(&request);
    let mut came: Value = serde_json::from_str(FORM).unwrap();
    came["form_fields"][0]["response"] = json!(true);
    assert_eq!(sent["version"], came["version"]);
    assert_eq!(sent["form_fields"], came["form_fields"]);
    reply
        .send(response(200, r#"{"application_status":"APPROVED"}"#))
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::RulesAnswered {
            request: 4,
            server: SERVER,
            outcome: JoinOutcome::Approved,
        }
    );
    // Accepted, the account writes before the member's update arrives...
    app.running
        .commands
        .send(Command::SendMessage {
            channel: ChannelId(30),
            content: "Ya entré".into(),
            reply_to: None,
            nonce: 5,
            mention_reply: false,
        })
        .await
        .unwrap();
    let (request, _) = app.pending().await;
    assert!(
        request.starts_with("POST /api/v10/channels/30/messages "),
        "{request}"
    );
    // ...and the rules are not sent again.
    app.running
        .commands
        .send(Command::AnswerServerRules {
            request: 6,
            server: SERVER,
            rules,
            answers: vec![RulesAnswer::Agreed(true)],
        })
        .await
        .unwrap();
    loop {
        if let Event::Failed { failure, .. } = app.next_data().await {
            assert_eq!(
                failure,
                rune_provider::Failure::Rules {
                    request: 6,
                    server: SERVER
                }
            );
            break;
        }
    }
}

#[tokio::test]
async fn each_write_meets_a_challenge_and_goes_once_with_the_answer() {
    let mut edited = darkroom_onboarding();
    edited.prompts[0].title = "¿Qué vas a revelar?".into();
    let writes = [
        (
            "99",
            Command::AnswerServerRules {
                request: 1,
                server: SERVER,
                rules: form(),
                answers: vec![RulesAnswer::Agreed(true)],
            },
            "PUT /api/v10/guilds/10/requests/@me ",
            r#"{"application_status":"SUBMITTED"}"#,
        ),
        (
            "99",
            Command::AnswerOnboarding {
                request: 1,
                server: SERVER,
                onboarding: darkroom_onboarding(),
                chosen: vec![ChoiceId(1_290_000_000_000_000_201)],
            },
            "POST /api/v10/guilds/10/onboarding-responses ",
            "{}",
        ),
        (
            "42",
            Command::SaveOnboarding {
                request: 1,
                server: SERVER,
                onboarding: edited,
            },
            "PUT /api/v10/guilds/10/onboarding ",
            "",
        ),
    ];
    for (owner, command, route, answer) in writes {
        let mut app = Harness::new().await;
        joined(&mut app, owner, owner == "99").await;
        app.running.commands.send(command).await.unwrap();
        let (request, reply) = app.pending().await;
        assert!(request.starts_with(route), "{request}");
        reply
            .send(response(400, super::challenges::CHALLENGE))
            .unwrap();
        let Event::ChallengeRequired { challenge, .. } = app.next_data().await else {
            panic!("offered to the person: {route}")
        };
        super::challenges::solve(&mut app, challenge).await;
        let (request, reply) = app.pending().await;
        assert!(request.starts_with(route), "{request}");
        super::challenges::answered(&request);
        reply
            .send(if answer.is_empty() {
                "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into()
            } else {
                response(200, answer)
            })
            .unwrap();
        loop {
            match app.next_data().await {
                Event::RulesAnswered { outcome, .. } => {
                    assert_eq!(outcome, JoinOutcome::InReview);
                    break;
                }
                Event::OnboardingAnswered { chosen, .. } => {
                    assert_eq!(chosen, [ChoiceId(1_290_000_000_000_000_201)]);
                    break;
                }
                Event::OnboardingLoaded {
                    onboarding: Some(saved),
                    ..
                } => {
                    assert_eq!(saved.prompts[0].title, "¿Qué vas a revelar?");
                    break;
                }
                Event::Failed { reason, .. } => panic!("{route}: {reason}"),
                _ => {}
            }
        }
    }
}

#[tokio::test]
async fn the_owner_saves_the_onboarding_as_discord_documents_it() {
    let mut app = Harness::new().await;
    joined(&mut app, "42", false).await;
    let mut onboarding = darkroom_onboarding();
    onboarding.prompts[0].title = "¿Qué vas a revelar?".into();
    app.running
        .commands
        .send(Command::SaveOnboarding {
            request: 1,
            server: SERVER,
            onboarding: onboarding.clone(),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("PUT /api/v10/guilds/10/onboarding "),
        "{request}"
    );
    let sent = body(&request);
    assert_eq!(sent["prompts"][0]["title"], json!("¿Qué vas a revelar?"));
    assert_eq!(sent["prompts"][0]["options"][0]["emoji_name"], json!("🎞️"));
    assert_eq!(sent["prompts"][0]["options"][0]["role_ids"], json!(["12"]));
    assert_eq!(sent["default_channel_ids"], json!(["30"]));
    reply
        .send(response(
            200,
            &ONBOARDING.replace("¿Qué quieres hacer?", "¿Qué vas a revelar?"),
        ))
        .unwrap();
    let Event::OnboardingLoaded {
        request: 1,
        onboarding: Some(saved),
        ..
    } = app.next_data().await
    else {
        panic!("what was saved")
    };
    assert_eq!(saved.prompts[0].title, "¿Qué vas a revelar?");
    // An answer RUNE cannot read leaves it unknown what was kept.
    app.running
        .commands
        .send(Command::SaveOnboarding {
            request: 2,
            server: SERVER,
            onboarding: onboarding.clone(),
        })
        .await
        .unwrap();
    let (_, reply) = app.pending().await;
    reply
        .send(response(200, r#"{"prompts": "no es una lista"}"#))
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: rune_provider::Failure::OnboardingEdit { request: 2, .. },
            ambiguous: true,
            ..
        }
    ));
    // An unknown role never leaves.
    onboarding.prompts[0].options[0]
        .roles
        .push(rune_model::RoleId(77));
    app.running
        .commands
        .send(Command::SaveOnboarding {
            request: 3,
            server: SERVER,
            onboarding,
        })
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: rune_provider::Failure::OnboardingEdit { request: 3, .. },
            ambiguous: false,
            ..
        }
    ));
}
