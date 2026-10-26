//! AutoMod through the session's checked queue, against local fixtures
//! only: what leaves for each route, what comes back, what the Gateway
//! says, and a message AutoMod refuses. Nothing here reached Discord.

use super::*;
use rune_model::{
    AutoModAction, AutoModKind, AutoModRule, AutoModRuleId, AutoModTrigger, ServerId, WordFilter,
};
use rune_provider::Failure;

const SERVER: ServerId = ServerId(10);

fn body(request: &str) -> Value {
    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
}

/// A server with one text channel, the account's own or not.
async fn joined(app: &mut Harness, owner: &str) {
    app.dispatch
        .send((
            "GUILD_CREATE",
            json!({"id": "10", "name": "Taller", "owner_id": owner,
                "roles": [{"id": "10", "name": "@everyone", "permissions": (1u64 << 10 | 1 << 11).to_string(), "position": 0}],
                "members": [{"user": {"id": "42", "username": "Owner"}, "roles": []}],
                "channels": [
                    {"id": "30", "guild_id": "10", "type": 0, "name": "general", "permission_overwrites": []}]}),
        ))
        .await
        .unwrap();
    loop {
        if matches!(app.running.next().await, Event::ChannelChanged(channel) if channel.id == ChannelId(30))
        {
            break;
        }
    }
}

/// A rule as Discord sends it, with `id`.
fn wire_rule(id: &str, enabled: bool) -> String {
    json!({"id": id, "guild_id": "10", "name": "Sin invitaciones", "creator_id": "42",
        "event_type": 1, "trigger_type": 1,
        "trigger_metadata": {"keyword_filter": ["*discord.gg/*"], "regex_patterns": [], "allow_list": []},
        "actions": [{"type": 1, "metadata": {"custom_message": "Aquí no."}},
            {"type": 2, "metadata": {"channel_id": "30"}}],
        "enabled": enabled, "exempt_roles": [], "exempt_channels": []})
    .to_string()
}

fn keywords() -> AutoModRule {
    AutoModRule {
        trigger: AutoModTrigger::Keywords(WordFilter {
            keywords: vec!["*discord.gg/*".into()],
            ..WordFilter::default()
        }),
        actions: vec![
            AutoModAction::Block {
                notice: Some("Aquí no.".into()),
            },
            AutoModAction::Alert {
                channel: ChannelId(30),
            },
        ],
        ..AutoModRule::fresh(AutoModKind::Keywords)
    }
}

#[tokio::test]
async fn rules_are_read_made_turned_off_and_deleted_by_their_routes() {
    let mut app = Harness::new().await;
    joined(&mut app, "42").await;
    app.running
        .commands
        .send(Command::LoadAutoModRules {
            request: 1,
            server: SERVER,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("GET /api/v10/guilds/10/auto-moderation/rules "),
        "{request}"
    );
    reply
        .send(response(200, &format!("[{}]", wire_rule("77", true))))
        .unwrap();
    let Event::AutoModRules {
        request: 1, rules, ..
    } = app.next_data().await
    else {
        panic!("the rules")
    };
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].id, Some(AutoModRuleId(77)));
    // A new rule names its kind; Discord answers it with its id.
    app.running
        .commands
        .send(Command::SaveAutoModRule {
            request: 2,
            server: SERVER,
            rule: keywords(),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("POST /api/v10/guilds/10/auto-moderation/rules "),
        "{request}"
    );
    let sent = body(&request);
    assert_eq!(sent["trigger_type"], 1);
    assert_eq!(sent["event_type"], 1);
    assert_eq!(sent["name"], "Bloquear palabras propias");
    assert_eq!(
        sent["trigger_metadata"]["keyword_filter"],
        json!(["*discord.gg/*"])
    );
    assert_eq!(
        sent["actions"],
        json!([{"type": 1, "metadata": {"custom_message": "Aquí no."}},
            {"type": 2, "metadata": {"channel_id": "30"}}])
    );
    reply.send(response(200, &wire_rule("78", true))).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::AutoModRuleChanged { request: Some(2), rule, .. } if rule.id == Some(AutoModRuleId(78))
    ));
    // A change goes to the rule's own route, without its kind.
    app.running
        .commands
        .send(Command::SaveAutoModRule {
            request: 3,
            server: SERVER,
            rule: AutoModRule {
                id: Some(AutoModRuleId(78)),
                ..keywords()
            },
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("PATCH /api/v10/guilds/10/auto-moderation/rules/78 "),
        "{request}"
    );
    assert!(body(&request).get("trigger_type").is_none());
    reply.send(response(200, &wire_rule("78", true))).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::AutoModRuleChanged {
            request: Some(3),
            ..
        }
    ));
    // Off, with only that.
    app.running
        .commands
        .send(Command::SetAutoModRuleEnabled {
            server: SERVER,
            rule: AutoModRuleId(78),
            enabled: false,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PATCH /api/v10/guilds/10/auto-moderation/rules/78 "));
    assert_eq!(body(&request), json!({"enabled": false}));
    reply.send(response(200, &wire_rule("78", false))).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::AutoModRuleChanged { request: None, rule, .. } if !rule.enabled
    ));
    app.running
        .commands
        .send(Command::DeleteAutoModRule {
            server: SERVER,
            rule: AutoModRuleId(78),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("DELETE /api/v10/guilds/10/auto-moderation/rules/78 "));
    reply.send(response(204, "")).unwrap();
    assert_eq!(
        app.next_data().await,
        Event::AutoModRuleRemoved {
            server: SERVER,
            rule: AutoModRuleId(78),
        }
    );
}

#[tokio::test]
async fn nothing_leaves_without_manage_guild_or_for_a_rule_discord_would_refuse() {
    let mut app = Harness::new().await;
    joined(&mut app, "99").await;
    app.running
        .commands
        .send(Command::LoadAutoModRules {
            request: 1,
            server: SERVER,
        })
        .await
        .unwrap();
    let Event::Failed {
        failure: Failure::AutoModRules { request: 1, .. },
        reason,
        ambiguous: false,
    } = app.next_data().await
    else {
        panic!("refused here")
    };
    assert_eq!(
        reason,
        "No tienes permiso para administrar AutoMod en este servidor."
    );
    let mut owned = Harness::new().await;
    joined(&mut owned, "42").await;
    owned
        .running
        .commands
        .send(Command::SaveAutoModRule {
            request: 2,
            server: SERVER,
            rule: AutoModRule::fresh(AutoModKind::Keywords),
        })
        .await
        .unwrap();
    assert!(matches!(
        owned.next_data().await,
        Event::Failed {
            failure: Failure::AutoModSave { request: 2, .. },
            ..
        }
    ));
    assert!(owned.requests.try_recv().is_err(), "nothing was sent");
}

#[tokio::test]
async fn a_message_automod_refuses_says_what_the_server_tells() {
    for (body, notice) in [
        (
            json!({"code": 200000, "message": "No compartas\ninvitaciones."}),
            Some("No compartas invitaciones."),
        ),
        (
            json!({"code": 200000, "message": "Message was blocked by automatic moderation"}),
            None,
        ),
        (json!({"code": 200001}), None),
    ] {
        let mut app = Harness::new().await;
        app.running.commands.send(rest::send(1)).await.unwrap();
        let (request, reply) = app.pending().await;
        assert!(request.starts_with("POST /api/v10/channels/20/messages "));
        reply.send(response(400, &body.to_string())).unwrap();
        let Event::AutoModBlocked {
            failure: Failure::Send { nonce: 1, .. },
            notice: told,
        } = app.next_data().await
        else {
            panic!("AutoMod's refusal ends the send");
        };
        assert_eq!(told.as_deref(), notice);
    }
}

#[tokio::test]
async fn the_gateway_tells_rules_changed_and_what_automod_did() {
    let mut app = Harness::new().await;
    joined(&mut app, "42").await;
    app.dispatch
        .send((
            "AUTO_MODERATION_RULE_UPDATE",
            serde_json::from_str(&wire_rule("77", false)).unwrap(),
        ))
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::AutoModRuleChanged { server: SERVER, request: None, rule } if !rule.enabled
    ));
    app.dispatch
        .send((
            "AUTO_MODERATION_ACTION_EXECUTION",
            json!({"guild_id": "10", "rule_id": "77", "rule_trigger_type": 1,
                "user_id": "43", "channel_id": "30", "content": "discord.gg/abc",
                "matched_keyword": "*discord.gg/*", "matched_content": "discord.gg/",
                "action": {"type": 1, "metadata": {}}}),
        ))
        .await
        .unwrap();
    let Event::AutoModExecuted(done) = app.next_data().await else {
        panic!("what AutoMod did");
    };
    assert_eq!(
        (done.rule, done.user, done.channel, done.matched.as_deref()),
        (
            AutoModRuleId(77),
            UserId(43),
            Some(ChannelId(30)),
            Some("*discord.gg/*")
        )
    );
    app.dispatch
        .send((
            "AUTO_MODERATION_RULE_DELETE",
            json!({"id": "77", "guild_id": "10"}),
        ))
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::AutoModRuleRemoved { rule, .. } if rule == AutoModRuleId(77)
    ));
}

#[tokio::test]
async fn a_challenge_on_a_rule_is_offered_and_the_save_sent_once_more() {
    let mut app = Harness::new().await;
    joined(&mut app, "42").await;
    app.running
        .commands
        .send(Command::SaveAutoModRule {
            request: 1,
            server: SERVER,
            rule: keywords(),
        })
        .await
        .unwrap();
    let route = "POST /api/v10/guilds/10/auto-moderation/rules ";
    let (request, reply) = app.pending().await;
    assert!(request.starts_with(route), "{request}");
    reply
        .send(response(400, super::challenges::CHALLENGE))
        .unwrap();
    let Event::ChallengeRequired { challenge, .. } = app.next_data().await else {
        panic!("offered to the person")
    };
    super::challenges::solve(&mut app, challenge).await;
    let (request, reply) = app.pending().await;
    assert!(request.starts_with(route), "{request}");
    super::challenges::answered(&request);
    reply.send(response(200, &wire_rule("79", true))).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::AutoModRuleChanged { request: Some(1), rule, .. } if rule.id == Some(AutoModRuleId(79))
    ));
}
