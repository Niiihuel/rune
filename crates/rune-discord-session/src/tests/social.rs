//! Writes about people and conversations, over loopback fixtures: the route
//! and body Serein uses, the answer read back, and refusals explained.

use super::*;
use rune_model::{ChannelId, ChannelKind};
use rune_provider::{Created, GroupIcon};
use serde_json::{json, Value};

#[test]
fn a_groups_history_lines_name_who_they_are_about() {
    for (kind, expected) in [
        (1, rune_model::MessageKind::RecipientAdd),
        (2, rune_model::MessageKind::RecipientRemove),
        (4, rune_model::MessageKind::ChannelRename),
        (5, rune_model::MessageKind::ChannelIcon),
    ] {
        let line = json!({"id":"500","channel_id":"31","type":kind,
            "author":{"id":"9","username":"owner"},"content":"Asado",
            "timestamp":"2026-10-17T12:00:00Z","mentions":[{"id":"7","username":"asks"}]});
        let (message, _) = crate::wire::message(&line).unwrap();
        assert_eq!(message.kind, expected);
        assert_eq!(message.mentions, [UserId(7)]);
    }
}

/// The request line and the JSON body of what the fixture received.
fn sent(request: &str) -> (String, Option<Value>) {
    let line = request.lines().next().unwrap().to_owned();
    let body = request.split_once("\r\n\r\n").unwrap().1;
    (
        line,
        (!body.is_empty()).then(|| serde_json::from_str(body).unwrap()),
    )
}

async fn exchange(
    command: Command,
    reply: String,
) -> (Result<Vec<Event>, String>, String, Option<Value>) {
    let (http, captured) = fixture(reply).await;
    let result = http
        .execute(&secret(), &command, "")
        .await
        .map_err(|error| {
            format!(
                "{}{}",
                error.reason,
                if error.ambiguous { " (ambiguous)" } else { "" }
            )
        });
    let (line, body) = sent(&captured.await.unwrap());
    (result, line, body)
}

fn no_content() -> String {
    "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into()
}

#[tokio::test]
async fn relationship_writes_use_the_routes_serein_uses_and_wait_for_the_gateway() {
    let user = UserId(7);
    for (command, route, body) in [
        (
            Command::SendFriendRequest {
                username: " @Zoe.Dev ".into(),
            },
            "POST /api/v10/users/@me/relationships",
            Some(json!({"username":"zoe.dev","discriminator":null})),
        ),
        (
            Command::AcceptFriendRequest { user },
            "PUT /api/v10/users/@me/relationships/7",
            Some(json!({})),
        ),
        (
            Command::RemoveRelationship { user },
            "DELETE /api/v10/users/@me/relationships/7",
            None,
        ),
        (
            Command::BlockUser { user },
            "PUT /api/v10/users/@me/relationships/7",
            Some(json!({"type":2})),
        ),
        (
            Command::SetFriendNickname {
                user,
                nickname: Some(" Bestie ".into()),
            },
            "PATCH /api/v10/users/@me/relationships/7",
            Some(json!({"nickname":"Bestie"})),
        ),
        (
            Command::SetFriendNickname {
                user,
                nickname: Some("  ".into()),
            },
            "PATCH /api/v10/users/@me/relationships/7",
            Some(json!({"nickname":null})),
        ),
    ] {
        let (result, line, sent) = exchange(command, no_content()).await;
        // Nothing is reported from here: RELATIONSHIP_* confirms the change.
        assert_eq!(result, Ok(Vec::new()));
        assert_eq!(line, format!("{route} HTTP/1.1"));
        assert_eq!(sent, body);
    }
}

#[tokio::test]
async fn relationship_success_without_a_document_is_not_reported_as_a_failure() {
    for status in [200, 201, 204] {
        let (result, _, _) = exchange(
            Command::AcceptFriendRequest { user: UserId(7) },
            response(status, ""),
        )
        .await;
        assert_eq!(result, Ok(Vec::new()), "HTTP {status} confirms the write");
    }
    // A nonempty malformed answer remains ambiguous, rather than hiding a
    // protocol failure behind the optional-body route's success status.
    let (result, _, _) = exchange(
        Command::AcceptFriendRequest { user: UserId(7) },
        response(200, "not JSON"),
    )
    .await;
    assert!(result.unwrap_err().ends_with("(ambiguous)"));
}

#[tokio::test]
async fn a_required_document_still_rejects_an_empty_success() {
    let (result, _, _) = exchange(
        Command::OpenDirect {
            request: 3,
            user: UserId(7),
        },
        response(200, ""),
    )
    .await;
    assert!(result.unwrap_err().ends_with("(ambiguous)"));
}

#[tokio::test]
async fn a_groups_people_change_by_the_official_clients_routes_and_wait_for_the_gateway() {
    let (channel, user) = (ChannelId(31), UserId(7));
    for (command, route) in [
        (
            Command::AddGroupMember { channel, user },
            "PUT /api/v10/channels/31/recipients/7",
        ),
        (
            Command::RemoveGroupMember { channel, user },
            "DELETE /api/v10/channels/31/recipients/7",
        ),
    ] {
        let (result, line, sent) = exchange(command, no_content()).await;
        // CHANNEL_RECIPIENT_ADD/REMOVE confirm it.
        assert_eq!(result, Ok(Vec::new()));
        assert_eq!(line, format!("{route} HTTP/1.1"));
        assert_eq!(sent, None);
    }
}

#[tokio::test]
async fn a_new_group_and_a_new_owner_are_read_back_from_the_answer() {
    let group = json!({"id":"31","type":3,"owner_id":"42","name":null,
        "recipients":[{"id":"7","username":"ada"},{"id":"8","username":"bruno"}]});
    let (result, line, sent) = exchange(
        Command::CreateGroup {
            request: 4,
            users: vec![UserId(7), UserId(8)],
        },
        response(200, &group.to_string()),
    )
    .await;
    assert_eq!(line, "POST /api/v10/users/@me/channels HTTP/1.1");
    assert_eq!(sent, Some(json!({"recipients":["7","8"]})));
    let events = result.unwrap();
    assert!(events.iter().any(|event| matches!(event,
        Event::ChannelChanged(channel) if channel.id == ChannelId(31) && channel.kind == ChannelKind::Group)));
    assert_eq!(
        events.last(),
        Some(&Event::Created {
            request: 4,
            created: Created::Channel(ChannelId(31)),
        })
    );
    // An answer that is not a group with those people may have made one.
    let (result, _, _) = exchange(
        Command::CreateGroup {
            request: 5,
            users: vec![UserId(7), UserId(9)],
        },
        response(200, &group.to_string()),
    )
    .await;
    assert!(result.unwrap_err().ends_with("(ambiguous)"));

    let mut handed = group.clone();
    handed["owner_id"] = json!("7");
    let (result, line, sent) = exchange(
        Command::SetGroupOwner {
            channel: ChannelId(31),
            user: UserId(7),
        },
        response(200, &handed.to_string()),
    )
    .await;
    assert_eq!(line, "PATCH /api/v10/channels/31 HTTP/1.1");
    assert_eq!(sent, Some(json!({"owner":"7"})));
    assert!(result.unwrap().iter().any(|event| matches!(event,
        Event::ChannelChanged(channel) if channel.owner == Some(UserId(7)))));
    // Still the account's: the change did not happen as asked.
    let (result, _, _) = exchange(
        Command::SetGroupOwner {
            channel: ChannelId(31),
            user: UserId(7),
        },
        response(200, &group.to_string()),
    )
    .await;
    assert!(result.unwrap_err().ends_with("(ambiguous)"));
}

#[test]
fn a_groups_writes_are_checked_against_what_the_session_knows() {
    use rune_model::{Channel, Relationship, RelationshipKind};
    let friend = |user: u64| Relationship {
        user: UserId(user),
        kind: RelationshipKind::Friend,
        since: None,
        nickname: None,
        spam: false,
    };
    let snapshot = |owner: u64| {
        let group = json!({"id":"31","type":3,"owner_id":owner.to_string(),
            "recipients":[{"id":"7","username":"ada"}]});
        let channel: Channel = crate::bootstrap::private_channel(&group).unwrap().0;
        rune_provider::Snapshot {
            me: Some(crate::wire::user(&json!({"id":"42","username":"me"})).unwrap()),
            channels: vec![channel],
            relationships: Some(vec![friend(7), friend(8)]),
            ..Default::default()
        }
    };
    let allowed = |snapshot: &rune_provider::Snapshot, command: Command| {
        crate::social::allowed(snapshot, &command).is_ok()
    };
    let owned = snapshot(42);
    let (channel, ada, bruno, stranger) = (ChannelId(31), UserId(7), UserId(8), UserId(9));
    assert!(allowed(
        &owned,
        Command::CreateGroup {
            request: 1,
            users: vec![ada, bruno]
        }
    ));
    assert!(!allowed(
        &owned,
        Command::CreateGroup {
            request: 1,
            users: vec![ada]
        }
    ));
    assert!(!allowed(
        &owned,
        Command::CreateGroup {
            request: 1,
            users: vec![ada, stranger]
        }
    ));
    assert!(!allowed(
        &owned,
        Command::CreateGroup {
            request: 1,
            users: vec![ada, ada]
        }
    ));
    assert!(allowed(
        &owned,
        Command::AddGroupMember {
            channel,
            user: bruno
        }
    ));
    assert!(!allowed(
        &owned,
        Command::AddGroupMember { channel, user: ada }
    ));
    assert!(!allowed(
        &owned,
        Command::AddGroupMember {
            channel,
            user: stranger
        }
    ));
    assert!(allowed(
        &owned,
        Command::RemoveGroupMember { channel, user: ada }
    ));
    assert!(allowed(
        &owned,
        Command::SetGroupOwner { channel, user: ada }
    ));
    assert!(!allowed(
        &owned,
        Command::SetGroupOwner {
            channel,
            user: bruno
        }
    ));
    assert!(!allowed(
        &owned,
        Command::SetGroupOwner {
            channel,
            user: UserId(42)
        }
    ));
    // Someone else's group: only adding friends.
    let theirs = snapshot(7);
    assert!(allowed(
        &theirs,
        Command::AddGroupMember {
            channel,
            user: bruno
        }
    ));
    assert!(!allowed(
        &theirs,
        Command::RemoveGroupMember { channel, user: ada }
    ));
    assert!(!allowed(
        &theirs,
        Command::SetGroupOwner { channel, user: ada }
    ));
}

#[tokio::test]
async fn refusals_are_explained_and_a_challenge_goes_back_to_the_provider() {
    for (reply, expected) in [
        (
            response(400, r#"{"code":80004,"message":"Unknown"}"#),
            "No encontramos a nadie con ese nombre de usuario.",
        ),
        (
            response(400, r#"{"code":80001,"message":"Blocked"}"#),
            "Esa persona no acepta solicitudes de amistad.",
        ),
        (
            response(
                400,
                r#"{"captcha_key":["required"],"captcha_service":"hcaptcha","captcha_sitekey":"synthetic-sitekey"}"#,
            ),
            "Discord requiere una verificación para completar esta acción.",
        ),
        (
            response(500, "{}"),
            "Discord no pudo confirmar esta acción. (ambiguous)",
        ),
    ] {
        let command = Command::SendFriendRequest {
            username: "zoe".into(),
        };
        let (result, _, _) = exchange(command, reply).await;
        assert_eq!(result, Err(expected.into()));
    }
    // A handle Discord cannot have never leaves.
    let http = Http::local("http://127.0.0.1:9");
    let error = http
        .execute(
            &secret(),
            &Command::SendFriendRequest {
                username: "no valid!".into(),
            },
            "",
        )
        .await
        .unwrap_err();
    assert!(!error.ambiguous && error.reason.to_string().starts_with("Escribí un nombre"));
}

#[tokio::test]
async fn opening_a_direct_conversation_reports_the_channel_the_answer_names() {
    let answer = json!({"id":"30","type":1,"recipients":[{"id":"8","username":"friend"}],
        "last_message_id":null});
    let command = Command::OpenDirect {
        request: 5,
        user: UserId(8),
    };
    let (result, line, body) = exchange(command.clone(), response(200, &answer.to_string())).await;
    assert_eq!(line, "POST /api/v10/users/@me/channels HTTP/1.1");
    assert_eq!(body, Some(json!({"recipient_id":"8"})));
    let events = result.unwrap();
    assert!(matches!(&events[0], Event::UserChanged(user) if user.id == UserId(8)));
    assert!(matches!(&events[1], Event::ChannelChanged(channel)
        if channel.id == ChannelId(30) && channel.kind == ChannelKind::Direct
            && channel.recipients == [UserId(8)]));
    assert_eq!(
        events[2],
        Event::Created {
            request: 5,
            created: Created::Channel(ChannelId(30)),
        }
    );
    // An answer about someone else: the write may have happened, unclear.
    let other = json!({"id":"31","type":1,"recipients":[{"id":"9","username":"other"}]});
    let (result, _, _) = exchange(command, response(200, &other.to_string())).await;
    assert!(result.unwrap_err().ends_with("(ambiguous)"));
}

#[tokio::test]
async fn a_group_is_renamed_given_a_picture_and_left_by_its_channel_routes() {
    let hash = "0123456789abcdef0123456789abcdef";
    let group = |name: &str, icon: Value| {
        json!({"id":"31","type":3,"name":name,"owner_id":"9","icon":icon,
            "recipients":[{"id":"8","username":"friend"},{"id":"7","username":"asks"}]})
        .to_string()
    };
    let (result, line, body) = exchange(
        Command::RenameGroup {
            channel: ChannelId(31),
            name: " Asado ".into(),
        },
        response(200, &group("Asado", json!(hash))),
    )
    .await;
    assert_eq!(line, "PATCH /api/v10/channels/31 HTTP/1.1");
    assert_eq!(body, Some(json!({"name":"Asado"})));
    let events = result.unwrap();
    let Some(Event::ChannelChanged(changed)) = events.last() else {
        panic!("{events:?}");
    };
    assert_eq!(changed.name, "Asado");
    assert_eq!(changed.owner, Some(UserId(9)));
    assert_eq!(
        changed.icon.as_ref().unwrap().url,
        format!("https://cdn.discordapp.com/channel-icons/31/{hash}.png?size=128")
    );
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x40\0\0\0\x40\x08\x06\0\0\0".to_vec();
    for (icon, expected) in [
        (GroupIcon::new(png.into()), Some("data:image/png;base64,")),
        (None, None),
    ] {
        let (result, line, body) = exchange(
            Command::SetGroupIcon {
                channel: ChannelId(31),
                icon,
            },
            response(200, &group("Asado", Value::Null)),
        )
        .await;
        assert_eq!(line, "PATCH /api/v10/channels/31 HTTP/1.1");
        let sent = body.unwrap()["icon"].clone();
        match expected {
            Some(prefix) => assert!(sent.as_str().unwrap().starts_with(prefix)),
            None => assert!(sent.is_null()),
        }
        assert!(result.is_ok());
    }
    // Renaming to nothing is not offered here.
    let http = Http::local("http://127.0.0.1:9");
    let refused = http
        .execute(
            &secret(),
            &Command::RenameGroup {
                channel: ChannelId(31),
                name: " ".into(),
            },
            "",
        )
        .await
        .unwrap_err();
    assert!(!refused.ambiguous);
    for (command, reply) in [
        (
            Command::LeaveGroup {
                channel: ChannelId(31),
            },
            response(200, &group("Asado", Value::Null)),
        ),
        (
            Command::CloseDirect {
                channel: ChannelId(31),
            },
            no_content(),
        ),
    ] {
        let (result, line, body) = exchange(command, reply).await;
        assert_eq!(line, "DELETE /api/v10/channels/31 HTTP/1.1");
        assert_eq!(body, None);
        assert_eq!(
            result,
            Ok(vec![Event::ChannelRemoved {
                channel: ChannelId(31)
            }])
        );
    }
}
