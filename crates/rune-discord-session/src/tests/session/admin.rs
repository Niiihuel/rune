//! Server administration through the session's checked queue, against
//! local fixtures only: what leaves, what the answer keeps in the session
//! and the order the events take. Nothing here reached Discord.

use super::*;
use bytes::Bytes;
use rune_model::{
    AuditAction, AuditActionKind, AuditQuery, Ban, ChannelKind, EmojiId, ExpressionId, Overwrite,
    OverwriteTarget, Permissions, RoleId, ServerId, StickerId, UserId, WebhookId,
};
use rune_provider::{
    Created, ExpressionImage, ExpressionKind, Failure, FavoriteGif, NewExpression, RoleEdit,
    ServerEdit,
};

const SERVER: ServerId = ServerId(10);
const NO_CONTENT: &str = "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n";

/// A server someone else owns, where the account moderates (role 11,
/// position 2) above a peer (13, position 1) and below a boss (12, 3).
async fn moderated(app: &mut Harness) {
    let everyone: u64 = 1 | 1 << 10 | 1 << 11 | 1 << 16 | 1 << 26;
    let moderation: u64 = 1 << 1
        | 1 << 2
        | 1 << 4
        | 1 << 5
        | 1 << 7
        | 1 << 27
        | 1 << 28
        | 1 << 29
        | 1 << 30
        | 1 << 40;
    app.dispatch
        .send((
            "GUILD_CREATE",
            json!({"id": "10", "name": "Taller", "owner_id": "99",
                "roles": [
                    {"id": "10", "name": "@everyone", "permissions": everyone.to_string(), "position": 0},
                    {"id": "11", "name": "Moderación", "permissions": moderation.to_string(), "position": 2},
                    {"id": "12", "name": "Jefatura", "permissions": "0", "position": 3},
                    {"id": "13", "name": "Gente", "permissions": "0", "position": 1}],
                "members": [
                    {"user": {"id": "42", "username": "Owner"}, "roles": ["11"]},
                    {"user": {"id": "43", "username": "peer"}, "roles": ["13"]},
                    {"user": {"id": "44", "username": "boss"}, "roles": ["12"]}],
                "channels": [
                    {"id": "30", "guild_id": "10", "type": 0, "name": "general", "permission_overwrites": []},
                    {"id": "40", "guild_id": "10", "type": 4, "name": "Texto", "permission_overwrites": []}],
                "emojis": [{"id": "70", "name": "ferris"}],
                "stickers": [{"id": "80", "name": "Cangrejo", "format_type": 1}]}),
        ))
        .await
        .unwrap();
    loop {
        if matches!(app.running.next().await, Event::ChannelChanged(channel) if channel.id == ChannelId(40))
        {
            break;
        }
    }
}

fn body(request: &str) -> Value {
    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
}

/// The request's line and headers, lowercase.
fn head(request: &str) -> String {
    request
        .split_once("\r\n\r\n")
        .unwrap()
        .0
        .to_ascii_lowercase()
}

impl Harness {
    async fn send(&mut self, command: Command) {
        self.running.commands.send(command).await.unwrap();
    }

    /// The next request, which must start with `line`, answered with `reply`.
    async fn answer(&mut self, line: &str, reply: String) -> String {
        let (request, sender) = self.pending().await;
        assert!(request.starts_with(line), "{request}");
        sender.send(reply).unwrap();
        request
    }
}

#[tokio::test]
async fn a_new_role_is_kept_before_its_creation_and_its_unknown_bits_survive_an_edit() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    app.send(Command::CreateRole {
        request: 1,
        server: SERVER,
        name: " Lectores ".into(),
    })
    .await;
    let request = app
        .answer(
            "POST /api/v10/guilds/10/roles ",
            response(
                200,
                r#"{"id":"20","name":"Lectores","permissions":"0","position":1}"#,
            ),
        )
        .await;
    assert_eq!(
        body(&request),
        json!({"name": "Lectores", "permissions": "0"})
    );
    assert!(
        matches!(app.running.next().await, Event::ServerChanged(server)
        if server.roles.iter().any(|role| role.id == RoleId(20)))
    );
    assert_eq!(
        app.running.next().await,
        Event::Created {
            request: 1,
            created: Created::Role(SERVER, RoleId(20)),
        }
    );
    // Right after, before any Gateway echo: the new role is known.
    app.send(Command::EditRole {
        server: SERVER,
        role: RoleId(20),
        edit: RoleEdit {
            permissions: Some(Permissions::VIEW | Permissions::SEND),
            ..Default::default()
        },
    })
    .await;
    // Text-to-speech (12) and the soundboard (42) are not RUNE's.
    let unknown: u64 = 1 << 12 | 1 << 42;
    app.answer(
        "GET /api/v10/guilds/10/roles ",
        response(
            200,
            &json!([{"id": "20", "name": "Lectores", "permissions": unknown.to_string()}])
                .to_string(),
        ),
    )
    .await;
    let written = (unknown | 1 << 10 | 1 << 11).to_string();
    let request = app
        .answer(
            "PATCH /api/v10/guilds/10/roles/20 ",
            response(
                200,
                &json!({"id": "20", "name": "Lectores", "permissions": written, "position": 1})
                    .to_string(),
            ),
        )
        .await;
    assert_eq!(body(&request), json!({ "permissions": written }));
    assert!(
        matches!(app.running.next().await, Event::ServerChanged(server)
        if server.roles.iter().any(|role| role.id == RoleId(20) && role.permissions == Permissions::VIEW | Permissions::SEND))
    );
}

#[tokio::test]
async fn a_new_channel_takes_an_overwrite_at_once_written_as_the_whole_list() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    app.send(Command::CreateChannel {
        request: 2,
        server: SERVER,
        kind: ChannelKind::Text,
        name: "anuncios".into(),
        parent: Some(ChannelId(40)),
        private: false,
    })
    .await;
    let request = app
        .answer(
            "POST /api/v10/guilds/10/channels ",
            response(
                201,
                r#"{"id":"31","guild_id":"10","type":0,"name":"anuncios","parent_id":"40","permission_overwrites":[]}"#,
            ),
        )
        .await;
    assert_eq!(
        body(&request),
        json!({"name": "anuncios", "type": 0, "parent_id": "40"})
    );
    assert!(
        matches!(app.running.next().await, Event::ChannelChanged(channel)
        if channel.id == ChannelId(31) && channel.permissions.contains(Permissions::MANAGE_ROLES))
    );
    assert_eq!(
        app.running.next().await,
        Event::Created {
            request: 2,
            created: Created::Channel(ChannelId(31)),
        }
    );
    app.send(Command::SetOverwrite {
        channel: ChannelId(31),
        overwrite: Overwrite {
            target: OverwriteTarget::Role(RoleId(13)),
            allow: Permissions::SEND,
            deny: Permissions::NONE,
        },
    })
    .await;
    // Someone else's overwrite and a bit RUNE does not model (12) stay.
    let fresh = json!({"id": "31", "guild_id": "10", "type": 0, "name": "anuncios",
        "permission_overwrites": [
            {"id": "10", "type": 0, "allow": "0", "deny": "1024"},
            {"id": "13", "type": 0, "allow": "4096", "deny": "0"}]});
    app.answer(
        "GET /api/v10/channels/31 ",
        response(200, &fresh.to_string()),
    )
    .await;
    let list = json!([
        {"id": "10", "type": 0, "allow": "0", "deny": "1024"},
        {"id": "13", "type": 0, "allow": (4096 | 2048).to_string(), "deny": "0"}]);
    let mut saved = fresh.clone();
    saved["permission_overwrites"] = list.clone();
    let request = app
        .answer(
            "PATCH /api/v10/channels/31 ",
            response(200, &saved.to_string()),
        )
        .await;
    assert_eq!(body(&request), json!({ "permission_overwrites": list }));
    assert!(
        matches!(app.running.next().await, Event::ChannelChanged(channel)
        if channel.overwrites.iter().any(|o| o.target == OverwriteTarget::Role(RoleId(13)) && o.allow == Permissions::SEND))
    );
}

#[tokio::test]
async fn kicks_and_bans_carry_their_reason_and_answer_with_what_changed() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    app.send(Command::Kick {
        server: SERVER,
        user: UserId(43),
        reason: Some("spam y más".into()),
    })
    .await;
    let request = app
        .answer("DELETE /api/v10/guilds/10/members/43 ", NO_CONTENT.into())
        .await;
    assert!(head(&request).contains("x-audit-log-reason: spam%20y%20m%c3%a1s"));
    assert_eq!(
        app.running.next().await,
        Event::MemberRemoved {
            server: SERVER,
            user: UserId(43)
        }
    );
    app.send(Command::Ban {
        server: SERVER,
        user: UserId(77),
        reason: None,
    })
    .await;
    let request = app
        .answer("PUT /api/v10/guilds/10/bans/77 ", NO_CONTENT.into())
        .await;
    assert_eq!(body(&request), json!({"delete_message_seconds": 0}));
    assert!(!head(&request).contains("x-audit-log-reason"));
    assert!(matches!(
        app.running.next().await,
        Event::MemberRemoved {
            user: UserId(77),
            ..
        }
    ));
    assert_eq!(
        app.running.next().await,
        Event::BanAdded {
            server: SERVER,
            ban: Ban {
                user: UserId(77),
                reason: None
            }
        }
    );
    app.send(Command::Unban {
        server: SERVER,
        user: UserId(77),
    })
    .await;
    app.answer("DELETE /api/v10/guilds/10/bans/77 ", NO_CONTENT.into())
        .await;
    assert_eq!(
        app.running.next().await,
        Event::BanRemoved {
            server: SERVER,
            user: UserId(77)
        }
    );
    // A ban the Gateway reports reaches the list too.
    app.dispatch
        .send((
            "GUILD_BAN_ADD",
            json!({"guild_id": "10", "user": {"id": "78", "username": "otro"}}),
        ))
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::BanAdded { ban, .. } if ban.user == UserId(78)
    ));
}

#[tokio::test]
async fn a_timeout_and_a_nickname_come_back_as_the_member() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    let until = crate::messages::wall_now().plus_minutes(60);
    app.send(Command::TimeoutMember {
        server: SERVER,
        user: UserId(43),
        until: Some(until),
        reason: Some("calma".into()),
    })
    .await;
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PATCH /api/v10/guilds/10/members/43 "));
    assert!(head(&request).contains("x-audit-log-reason: calma"));
    let sent = body(&request)["communication_disabled_until"].clone();
    assert_eq!(crate::wire::timestamp(&sent).unwrap(), until);
    reply
        .send(response(
            200,
            &json!({"user": {"id": "43", "username": "peer"}, "roles": ["13"],
                "communication_disabled_until": sent})
            .to_string(),
        ))
        .unwrap();
    assert!(matches!(app.next_data().await, Event::MemberChanged(member)
        if member.user == UserId(43) && member.restrictions.timeout_until == Some(until)));
    // The account renames itself by `@me`.
    app.send(Command::SetNick {
        server: SERVER,
        user: UserId(42),
        nick: Some("Yo".into()),
    })
    .await;
    let request = app
        .answer(
            "PATCH /api/v10/guilds/10/members/@me ",
            response(
                200,
                r#"{"user":{"id":"42","username":"Owner"},"nick":"Yo","roles":["11"]}"#,
            ),
        )
        .await;
    assert_eq!(body(&request), json!({"nick": "Yo"}));
    assert!(matches!(app.next_data().await, Event::MemberChanged(member)
        if member.user == UserId(42) && member.nick.as_deref() == Some("Yo")));
}

#[tokio::test]
async fn bans_and_the_audit_log_page_through_their_filters() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    app.send(Command::LoadBans {
        request: 3,
        server: SERVER,
        after: Some(UserId(5)),
        limit: 2,
    })
    .await;
    app.answer(
        "GET /api/v10/guilds/10/bans?limit=2&after=5 ",
        response(
            200,
            r#"[{"user":{"id":"9","username":"b"},"reason":"spam"},{"user":{"id":"7","username":"a"},"reason":null}]"#,
        ),
    )
    .await;
    let Event::BansLoaded {
        bans, reached_end, ..
    } = app.next_data().await
    else {
        panic!("bans");
    };
    assert_eq!(
        bans.iter().map(|ban| ban.user.0).collect::<Vec<_>>(),
        [7, 9]
    );
    assert!(!reached_end);
    let query = AuditQuery {
        actor: None,
        action: Some(AuditActionKind::NickChanged),
    };
    app.send(Command::LoadAudit {
        request: 4,
        server: SERVER,
        query,
        before: None,
        limit: 2,
    })
    .await;
    // Discord's type 24 is a timeout too: a page of only those is not offered.
    let timeouts = json!({"users": [{"id": "42", "username": "Owner"}], "audit_log_entries": [
        {"id": "90", "user_id": "42", "target_id": "43", "action_type": 24,
            "changes": [{"key": "communication_disabled_until", "new_value": "2026-10-20T00:00:00Z"}]},
        {"id": "80", "user_id": "42", "target_id": "43", "action_type": 24,
            "changes": [{"key": "communication_disabled_until", "new_value": null}]}]});
    app.answer(
        "GET /api/v10/guilds/10/audit-logs?limit=2&action_type=24 ",
        response(200, &timeouts.to_string()),
    )
    .await;
    let nick = json!({"users": [], "audit_log_entries": [
        {"id": "70", "user_id": "42", "target_id": "43", "action_type": 24,
            "changes": [{"key": "nick", "new_value": "Pepe"}]}]});
    app.answer(
        "GET /api/v10/guilds/10/audit-logs?limit=2&action_type=24&before=80 ",
        response(200, &nick.to_string()),
    )
    .await;
    assert_eq!(
        app.next_data().await,
        Event::AuditLoaded {
            request: 4,
            server: SERVER,
            query,
            before: None,
            entries: vec![rune_model::AuditEntry {
                id: 70,
                actor: UserId(42),
                action: AuditAction::NickChanged {
                    user: UserId(43),
                    nick: Some("Pepe".into())
                },
                // Fixture ids are too small to carry a date.
                when: Default::default(),
                reason: None,
            }],
            reached_end: true,
        }
    );
}

#[tokio::test]
async fn invitations_are_listed_made_and_revoked() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    app.send(Command::LoadInvites {
        request: 5,
        server: SERVER,
    })
    .await;
    let invite = json!({"code": "abc", "channel": {"id": "30"},
        "inviter": {"id": "42", "username": "Owner"}, "uses": 1, "max_uses": 0,
        "max_age": 0, "temporary": false, "expires_at": null});
    app.answer(
        "GET /api/v10/guilds/10/invites ",
        response(200, &json!([invite]).to_string()),
    )
    .await;
    assert!(matches!(app.running.next().await, Event::UserChanged(user) if user.id == UserId(42)));
    assert!(
        matches!(app.running.next().await, Event::InvitesLoaded { invites, .. }
        if invites.len() == 1 && invites[0].code == "abc" && invites[0].max_uses.is_none())
    );
    app.send(Command::CreateInvite {
        request: 6,
        server: SERVER,
        channel: ChannelId(30),
        max_age_secs: 3600,
        max_uses: 5,
        temporary: false,
    })
    .await;
    let request = app
        .answer(
            "POST /api/v10/channels/30/invites ",
            response(
                200,
                r#"{"code":"new","channel":{"id":"30"},"max_age":3600,"max_uses":5,"uses":0,"temporary":false,"expires_at":"2026-10-17T13:00:00Z"}"#,
            ),
        )
        .await;
    assert_eq!(
        body(&request),
        json!({"max_age": 3600, "max_uses": 5, "temporary": false, "unique": true})
    );
    assert!(
        matches!(app.next_data().await, Event::InviteCreated { request: 6, invite, .. }
        if invite.code == "new" && invite.max_uses == Some(5) && invite.expires.is_some())
    );
    app.send(Command::RevokeInvite {
        server: SERVER,
        code: "abc".into(),
    })
    .await;
    app.answer(
        "DELETE /api/v10/invites/abc ",
        response(200, r#"{"code":"abc"}"#),
    )
    .await;
    assert_eq!(
        app.next_data().await,
        Event::InviteRemoved {
            server: SERVER,
            code: "abc".into()
        }
    );
}

#[tokio::test]
async fn expressions_are_uploaded_renamed_and_deleted() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(64u32.to_be_bytes());
    png.extend(64u32.to_be_bytes());
    png.extend([8, 6, 0, 0, 0]);
    app.send(Command::CreateExpression {
        request: 7,
        server: SERVER,
        expression: NewExpression {
            kind: ExpressionKind::Emoji,
            name: "crab".into(),
            image: ExpressionImage::png(ExpressionKind::Emoji, Bytes::from(png)).unwrap(),
            tags: String::new(),
            description: String::new(),
        },
    })
    .await;
    let request = app
        .answer(
            "POST /api/v10/guilds/10/emojis ",
            response(201, r#"{"id":"71","name":"crab"}"#),
        )
        .await;
    assert!(body(&request)["image"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,iVBORw0KGgo"));
    assert!(
        matches!(app.running.next().await, Event::ServerChanged(server)
        if server.emojis.iter().any(|emoji| emoji.id == EmojiId(71)))
    );
    assert_eq!(
        app.running.next().await,
        Event::Created {
            request: 7,
            created: Created::Expression(SERVER, ExpressionId::Emoji(EmojiId(71))),
        }
    );
    let mut gif = b"GIF89a".to_vec();
    gif.extend([0x40, 0x01, 0x40, 0x01]);
    app.send(Command::CreateExpression {
        request: 8,
        server: SERVER,
        expression: NewExpression {
            kind: ExpressionKind::Sticker,
            name: "Cangrejo feliz".into(),
            image: ExpressionImage::gif(ExpressionKind::Sticker, Bytes::from(gif)).unwrap(),
            tags: "crab".into(),
            description: String::new(),
        },
    })
    .await;
    let request = app
        .answer(
            "POST /api/v10/guilds/10/stickers ",
            response(
                201,
                r#"{"id":"81","name":"Cangrejo feliz","format_type":4}"#,
            ),
        )
        .await;
    assert!(head(&request)
        .contains("content-type: multipart/form-data; boundary=rune-sticker-00000000"));
    assert!(request.contains("name=\"tags\"\r\n\r\ncrab\r\n"));
    assert!(request.contains("filename=\"sticker.gif\"\r\nContent-Type: image/gif\r\n\r\nGIF89a"));
    assert!(
        matches!(app.running.next().await, Event::ServerChanged(server)
        if server.stickers.iter().any(|sticker| sticker.id == StickerId(81)))
    );
    assert!(matches!(
        app.running.next().await,
        Event::Created {
            request: 8,
            created: Created::Expression(_, ExpressionId::Sticker(StickerId(81)))
        }
    ));
    app.send(Command::RenameExpression {
        server: SERVER,
        expression: ExpressionId::Emoji(EmojiId(70)),
        name: "cangrejo".into(),
    })
    .await;
    let request = app
        .answer(
            "PATCH /api/v10/guilds/10/emojis/70 ",
            response(200, r#"{"id":"70","name":"cangrejo"}"#),
        )
        .await;
    assert_eq!(body(&request), json!({"name": "cangrejo"}));
    assert!(
        matches!(app.running.next().await, Event::ServerChanged(server)
        if server.emojis.iter().any(|emoji| emoji.id == EmojiId(70) && emoji.name == "cangrejo"))
    );
    app.send(Command::DeleteExpression {
        server: SERVER,
        expression: ExpressionId::Sticker(StickerId(80)),
    })
    .await;
    app.answer("DELETE /api/v10/guilds/10/stickers/80 ", NO_CONTENT.into())
        .await;
    assert!(
        matches!(app.running.next().await, Event::ServerChanged(server)
        if !server.stickers.iter().any(|sticker| sticker.id == StickerId(80)))
    );
}

#[tokio::test]
async fn a_member_page_lets_their_roles_change_and_prune_counts_first() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    app.send(Command::LoadMembers {
        request: 9,
        server: SERVER,
        after: None,
        query: Default::default(),
    })
    .await;
    let request = app
        .answer(
            "POST /api/v10/guilds/10/members-search ",
            response(
                200,
                &json!({"guild_id": "10", "page_result_count": 1, "total_result_count": 3,
                    "members": [{"member": {"user": {"id": "45", "username": "nuevo"},
                        "roles": [], "joined_at": "2026-10-01T00:00:00Z"}}]})
                .to_string(),
            ),
        )
        .await;
    assert_eq!(body(&request), json!({"limit": 100, "sort": 1}));
    assert!(matches!(app.running.next().await, Event::UserChanged(user) if user.id == UserId(45)));
    assert!(
        matches!(app.running.next().await, Event::MembersLoaded { request: 9, members, next: None, total: Some(3), .. }
        if members.len() == 1 && members[0].user == UserId(45))
    );
    // Known from the page, its roles can change: one request per role,
    // then the member read back.
    app.send(Command::SetMemberRoles {
        server: SERVER,
        user: UserId(45),
        roles: vec![RoleId(13)],
    })
    .await;
    app.answer(
        "PUT /api/v10/guilds/10/members/45/roles/13 ",
        NO_CONTENT.into(),
    )
    .await;
    app.answer(
        "GET /api/v10/guilds/10/members/45 ",
        response(
            200,
            r#"{"user":{"id":"45","username":"nuevo"},"roles":["13"]}"#,
        ),
    )
    .await;
    assert!(
        matches!(app.running.next().await, Event::MemberChanged(member)
        if member.user == UserId(45) && member.roles == [RoleId(13)])
    );
    app.send(Command::PruneMembers {
        request: 10,
        server: SERVER,
        days: 7,
        remove: false,
    })
    .await;
    app.answer(
        "GET /api/v10/guilds/10/prune?days=7 ",
        response(200, r#"{"pruned":4}"#),
    )
    .await;
    assert_eq!(
        app.next_data().await,
        Event::Pruned {
            request: 10,
            server: SERVER,
            days: 7,
            count: Some(4),
            removed: false,
        }
    );
    // The next page goes on from the member and join date its cursor names.
    app.send(Command::LoadMembers {
        request: 11,
        server: SERVER,
        after: Some("45:1759276800000".into()),
        query: Default::default(),
    })
    .await;
    let request = app
        .answer(
            "POST /api/v10/guilds/10/members-search ",
            response(
                200,
                r#"{"guild_id":"10","page_result_count":0,"total_result_count":3,"members":[]}"#,
            ),
        )
        .await;
    assert_eq!(
        body(&request)["after"],
        json!({"user_id": "45", "guild_joined_at": 1_759_276_800_000_i64})
    );
    assert!(matches!(
        app.next_data().await,
        Event::MembersLoaded {
            request: 11,
            after: Some(_),
            next: None,
            ..
        }
    ));
}

#[tokio::test]
async fn webhooks_are_managed_without_ever_keeping_their_token() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    let hook = json!({"id": "5", "guild_id": "10", "channel_id": "30", "type": 1, "name": "CI",
        "token": "SYNTHETIC_WEBHOOK_TOKEN",
        "url": "https://discord.com/api/webhooks/5/SYNTHETIC_WEBHOOK_TOKEN"});
    let list = response(200, &json!([hook]).to_string());
    app.send(Command::LoadWebhooks {
        request: 11,
        server: SERVER,
    })
    .await;
    app.answer("GET /api/v10/guilds/10/webhooks ", list.clone())
        .await;
    let loaded = app.next_data().await;
    assert!(matches!(&loaded, Event::WebhooksLoaded { webhooks, .. } if webhooks.len() == 1));
    assert!(!format!("{loaded:?}").contains("SYNTHETIC_WEBHOOK_TOKEN"));
    app.send(Command::CreateWebhook {
        server: SERVER,
        channel: ChannelId(30),
        name: "Avisos".into(),
    })
    .await;
    let mut made = hook.clone();
    made["id"] = json!("6");
    made["name"] = json!("Avisos");
    let request = app
        .answer(
            "POST /api/v10/channels/30/webhooks ",
            response(200, &made.to_string()),
        )
        .await;
    assert_eq!(body(&request), json!({"name": "Avisos"}));
    let changed = app.next_data().await;
    assert!(
        matches!(&changed, Event::WebhookChanged { webhook, .. } if webhook.id == WebhookId(6))
    );
    assert!(!format!("{changed:?}").contains("SYNTHETIC_WEBHOOK_TOKEN"));
    // Changing one first checks it is the server's own.
    app.send(Command::EditWebhook {
        server: SERVER,
        webhook: WebhookId(5),
        name: Some("CI 2".into()),
        channel: None,
    })
    .await;
    app.answer("GET /api/v10/guilds/10/webhooks ", list.clone())
        .await;
    let mut renamed = hook.clone();
    renamed["name"] = json!("CI 2");
    let request = app
        .answer(
            "PATCH /api/v10/webhooks/5 ",
            response(200, &renamed.to_string()),
        )
        .await;
    assert_eq!(body(&request), json!({"name": "CI 2"}));
    assert!(
        matches!(app.next_data().await, Event::WebhookChanged { webhook, .. } if webhook.name == "CI 2")
    );
    app.send(Command::DeleteWebhook {
        server: SERVER,
        webhook: WebhookId(5),
    })
    .await;
    app.answer("GET /api/v10/guilds/10/webhooks ", list).await;
    app.answer("DELETE /api/v10/webhooks/5 ", NO_CONTENT.into())
        .await;
    assert_eq!(
        app.next_data().await,
        Event::WebhookRemoved {
            server: SERVER,
            webhook: WebhookId(5)
        }
    );
}

#[tokio::test]
async fn integrations_are_listed_and_removed_and_the_server_keeps_its_color() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    app.send(Command::LoadIntegrations {
        request: 12,
        server: SERVER,
    })
    .await;
    app.answer(
        "GET /api/v10/guilds/10/integrations ",
        response(
            200,
            r#"[{"id":"7","name":"Bot","type":"discord","enabled":true,"application":{"id":"8","bot":{"id":"46","username":"bot","bot":true}}}]"#,
        ),
    )
    .await;
    assert!(
        matches!(app.next_data().await, Event::IntegrationsLoaded { integrations, .. }
        if integrations[0].bot == Some(UserId(46)) && integrations[0].service == "Aplicación")
    );
    app.send(Command::DeleteIntegration {
        server: SERVER,
        integration: rune_model::IntegrationId(7),
    })
    .await;
    app.answer(
        "DELETE /api/v10/guilds/10/integrations/7 ",
        NO_CONTENT.into(),
    )
    .await;
    assert!(matches!(
        app.next_data().await,
        Event::IntegrationRemoved { .. }
    ));
    app.send(Command::EditServer {
        server: SERVER,
        edit: ServerEdit {
            accent: Some(rune_model::Rgb(0xff0000)),
            description: Some(None),
            ..Default::default()
        },
    })
    .await;
    let request = app
        .answer(
            "PATCH /api/v10/guilds/10/profile ",
            response(
                200,
                r##"{"id":"10","name":"Taller","description":"","brand_color_primary":"#ff0000"}"##,
            ),
        )
        .await;
    assert_eq!(
        body(&request),
        json!({"description": "", "brand_color_primary": "#ff0000"})
    );
    assert!(matches!(app.next_data().await, Event::ServerChanged(server)
        if server.accent == rune_model::Rgb(0xff0000) && server.description.is_none()));
}

#[tokio::test]
async fn gif_favorites_are_read_and_changed_against_a_fresh_read() {
    let mut app = Harness::new().await;
    app.send(Command::LoadGifFavorites).await;
    app.answer(
        "GET /api/v10/users/@me/settings-proto/2 ",
        response(200, r#"{"settings":""}"#),
    )
    .await;
    assert_eq!(app.next_data().await, Event::GifFavorites(Vec::new()));
    let gif = FavoriteGif {
        url: "https://tenor.com/view/synthetic-1".into(),
        preview: Some("https://media.tenor.com/synthetic/1.mp4".into()),
        width: 300,
        height: 200,
    };
    app.send(Command::SetGifFavorite {
        gif: gif.clone(),
        favorite: true,
    })
    .await;
    app.answer(
        "GET /api/v10/users/@me/settings-proto/2 ",
        response(200, r#"{"settings":""}"#),
    )
    .await;
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PATCH /api/v10/users/@me/settings-proto/2 "));
    let sent = body(&request);
    assert_eq!(sent["required_data_version"], 0);
    reply
        .send(response(
            200,
            &json!({ "settings": sent["settings"] }).to_string(),
        ))
        .unwrap();
    assert_eq!(app.next_data().await, Event::GifFavorites(vec![gif]));
}

#[tokio::test]
async fn a_refused_command_never_leaves_and_only_a_write_can_be_ambiguous() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    // Above the account: refused here, nothing sent.
    app.send(Command::Kick {
        server: SERVER,
        user: UserId(44),
        reason: None,
    })
    .await;
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::Member {
                user: UserId(44),
                ..
            },
            ambiguous: false,
            ..
        }
    ));
    assert!(app.requests.try_recv().is_err());
    app.send(Command::DeleteRole {
        server: SERVER,
        role: RoleId(13),
    })
    .await;
    app.answer("DELETE /api/v10/guilds/10/roles/13 ", response(500, "{}"))
        .await;
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::Role { .. },
            ambiguous: true,
            ..
        }
    ));
    app.send(Command::LoadInvites {
        request: 13,
        server: SERVER,
    })
    .await;
    app.answer("GET /api/v10/guilds/10/invites ", response(500, "{}"))
        .await;
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::Server { .. },
            ambiguous: false,
            ..
        }
    ));
    // The member search is a POST, but a read: never ambiguous.
    app.send(Command::LoadMembers {
        request: 14,
        server: SERVER,
        after: None,
        query: Default::default(),
    })
    .await;
    app.answer(
        "POST /api/v10/guilds/10/members-search ",
        response(503, "{}"),
    )
    .await;
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            ambiguous: false,
            ..
        }
    ));
}

// Server administration, rest (P7).

/// The moderated server, now with role icons and gradients unlocked.
async fn styled(app: &mut Harness) {
    moderated(app).await;
    app.dispatch
        .send((
            "GUILD_UPDATE",
            json!({"id": "10", "features": ["ROLE_ICONS", "ENHANCED_ROLE_COLORS"]}),
        ))
        .await
        .unwrap();
    loop {
        if matches!(app.running.next().await, Event::ServerChanged(server) if server.options.role_icons)
        {
            break;
        }
    }
}

#[tokio::test]
async fn traits_and_safety_go_to_their_two_routes_with_flags_and_features_read_fresh() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    app.send(Command::EditServer {
        server: SERVER,
        edit: ServerEdit {
            traits: Some(vec![rune_model::ServerTrait {
                label: " Rust ".into(),
                emoji: Some("🦀".into()),
                emoji_id: None,
                emoji_animated: false,
            }]),
            verification: Some(rune_model::Verification::Medium),
            invites_paused: Some(true),
            ..Default::default()
        },
    })
    .await;
    let request = app
        .answer(
            "PATCH /api/v10/guilds/10/profile ",
            response(
                200,
                r#"{"id":"10","name":"Taller","traits":[{"label":"Rust","emoji_name":"🦀","emoji_id":null,"position":0}]}"#,
            ),
        )
        .await;
    assert_eq!(
        body(&request),
        json!({"traits": [{"label": "Rust", "emoji_name": "🦀", "emoji_id": null,
            "emoji_animated": false, "position": 0}]})
    );
    // Features travel whole: the ones RUNE does not know stay.
    app.answer(
        "GET /api/v10/guilds/10 ",
        response(
            200,
            r#"{"id":"10","features":["NEWS"],"system_channel_flags":0}"#,
        ),
    )
    .await;
    let request = app
        .answer(
            "PATCH /api/v10/guilds/10 ",
            response(
                200,
                r#"{"id":"10","verification_level":2,"features":["NEWS","INVITES_DISABLED"]}"#,
            ),
        )
        .await;
    assert_eq!(
        body(&request),
        json!({"verification_level": 2, "features": ["NEWS", "INVITES_DISABLED"]})
    );
    assert!(matches!(app.next_data().await, Event::ServerChanged(server)
        if server.options.traits.as_ref().is_some_and(|traits| traits[0].label == "Rust")));
    assert!(matches!(app.next_data().await, Event::ServerChanged(server)
        if server.options.invites_paused
            && server.options.verification == rune_model::Verification::Medium));
}

// Icon, banner and community (2026-10-19).

#[tokio::test]
async fn a_community_needs_administration_and_goes_with_its_channels_banner_and_emoji_trait() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    app.dispatch
        .send(("GUILD_UPDATE", json!({"id": "10", "features": ["BANNER"]})))
        .await
        .unwrap();
    loop {
        if matches!(app.running.next().await, Event::ServerChanged(server) if server.options.banner_allowed)
        {
            break;
        }
    }
    let community = ServerEdit {
        community: Some(true),
        rules_channel: Some(Some(ChannelId(30))),
        updates_channel: Some(Some(ChannelId(30))),
        verification: Some(rune_model::Verification::Low),
        media_scan: Some(rune_model::MediaScan::Everyone),
        ..Default::default()
    };
    // Without administration it never leaves.
    app.send(Command::EditServer {
        server: SERVER,
        edit: community.clone(),
    })
    .await;
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            ambiguous: false,
            ..
        }
    ));
    assert!(app.requests.try_recv().is_err());
    // A moving banner the boosts do not unlock never leaves either.
    let mut gif = b"GIF89a".to_vec();
    gif.extend(960u16.to_le_bytes());
    gif.extend(540u16.to_le_bytes());
    let moving = rune_provider::ProfileImage::new(bytes::Bytes::from(gif)).unwrap();
    app.send(Command::EditServer {
        server: SERVER,
        edit: ServerEdit {
            banner: Some(Some(moving)),
            ..Default::default()
        },
    })
    .await;
    assert!(matches!(app.next_data().await, Event::Failed { .. }));
    assert!(app.requests.try_recv().is_err());
    // Made an administrator, it becomes a community: features read fresh,
    // the rest of the requirements in the same write.
    let administrator: u64 = 1 << 3;
    app.dispatch
        .send((
            "GUILD_ROLE_UPDATE",
            json!({"guild_id": "10", "role": {"id": "11", "name": "Moderación",
                "permissions": administrator.to_string(), "position": 2}}),
        ))
        .await
        .unwrap();
    loop {
        if matches!(app.next_data().await, Event::ServerChanged(_)) {
            break;
        }
    }
    app.send(Command::EditServer {
        server: SERVER,
        edit: ServerEdit {
            traits: Some(vec![rune_model::ServerTrait {
                label: "Cangrejos".into(),
                emoji: Some("ferris".into()),
                emoji_id: Some(rune_model::EmojiId(70)),
                emoji_animated: false,
            }]),
            ..community
        },
    })
    .await;
    let request = app
        .answer(
            "PATCH /api/v10/guilds/10/profile ",
            response(
                200,
                r#"{"id":"10","traits":[{"label":"Cangrejos","emoji_name":"ferris","emoji_id":"70","position":0}]}"#,
            ),
        )
        .await;
    assert_eq!(body(&request)["traits"][0]["emoji_id"], "70");
    app.answer(
        "GET /api/v10/guilds/10 ",
        response(200, r#"{"id":"10","features":["BANNER"]}"#),
    )
    .await;
    let request = app
        .answer(
            "PATCH /api/v10/guilds/10 ",
            response(
                200,
                r#"{"id":"10","features":["BANNER","COMMUNITY"],"rules_channel_id":"30","public_updates_channel_id":"30","verification_level":1,"explicit_content_filter":2}"#,
            ),
        )
        .await;
    assert_eq!(
        body(&request),
        json!({"features": ["BANNER", "COMMUNITY"], "rules_channel_id": "30",
            "public_updates_channel_id": "30", "verification_level": 1,
            "explicit_content_filter": 2})
    );
    // What the role's change recomputed may come first.
    let server = loop {
        if let Event::ServerChanged(server) = app.next_data().await {
            if server.options.community {
                break server;
            }
        }
    };
    assert!(server.options.community_ready());
    let traits = server.options.traits.unwrap();
    assert_eq!(traits[0].emoji_id, Some(rune_model::EmojiId(70)));
}

#[tokio::test]
async fn a_challenge_on_pausing_invites_waits_for_the_person_and_goes_once() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    let pause = Command::EditServer {
        server: SERVER,
        edit: ServerEdit {
            invites_paused: Some(true),
            ..Default::default()
        },
    };
    app.send(pause).await;
    app.answer(
        "GET /api/v10/guilds/10 ",
        response(200, r#"{"id":"10","features":[]}"#),
    )
    .await;
    app.answer(
        "PATCH /api/v10/guilds/10 ",
        response(400, super::challenges::CHALLENGE),
    )
    .await;
    let Event::ChallengeRequired { challenge, failure } = app.next_data().await else {
        panic!("a challenge")
    };
    // The common path: the failure the command names, as any write's.
    assert_eq!(
        failure,
        Failure::Server {
            server: Some(SERVER)
        }
    );
    super::challenges::solve(&mut app, challenge).await;
    app.answer(
        "GET /api/v10/guilds/10 ",
        response(200, r#"{"id":"10","features":[]}"#),
    )
    .await;
    let request = app
        .answer(
            "PATCH /api/v10/guilds/10 ",
            response(200, r#"{"id":"10","features":["INVITES_DISABLED"]}"#),
        )
        .await;
    super::challenges::answered(&request);
    assert!(matches!(app.next_data().await, Event::ServerChanged(server)
        if server.options.invites_paused));
}

#[tokio::test]
async fn channels_move_at_once_by_the_guild_route_and_a_category_never_nests() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    // Into the category, taking its permissions (the moderators may change them).
    let step = rune_model::ChannelMove {
        channel: ChannelId(30),
        parent: Some(ChannelId(40)),
        position: 0,
        lock_permissions: true,
    };
    app.send(Command::ReorderChannels {
        server: SERVER,
        moves: vec![step],
    })
    .await;
    let request = app
        .answer("PATCH /api/v10/guilds/10/channels ", NO_CONTENT.into())
        .await;
    assert_eq!(
        body(&request),
        json!([{"id": "30", "position": 0, "parent_id": "40", "lock_permissions": true}])
    );
    assert!(
        matches!(app.next_data().await, Event::ChannelChanged(channel)
        if channel.id == ChannelId(30) && channel.parent == Some(ChannelId(40)))
    );
    app.send(Command::ReorderChannels {
        server: SERVER,
        moves: vec![rune_model::ChannelMove {
            channel: ChannelId(40),
            parent: Some(ChannelId(40)),
            position: 0,
            lock_permissions: false,
        }],
    })
    .await;
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::ChannelOrder { .. },
            ambiguous: false,
            ..
        }
    ));
    // Without a category there is nothing to take the permissions of.
    app.send(Command::ReorderChannels {
        server: SERVER,
        moves: vec![rune_model::ChannelMove {
            channel: ChannelId(30),
            parent: None,
            position: 0,
            lock_permissions: true,
        }],
    })
    .await;
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::ChannelOrder { .. },
            ..
        }
    ));
    // A category that denies the account changing permissions keeps them
    // to itself: the move that would take them is refused before it goes.
    let manage_roles: u64 = 1 << 28;
    app.dispatch
        .send((
            "CHANNEL_UPDATE",
            json!({"id": "40", "guild_id": "10", "type": 4, "name": "Texto",
                "permission_overwrites": [
                    {"id": "42", "type": 1, "allow": "0", "deny": manage_roles.to_string()}]}),
        ))
        .await
        .unwrap();
    loop {
        if matches!(app.next_data().await, Event::ChannelChanged(channel) if channel.id == ChannelId(40))
        {
            break;
        }
    }
    app.send(Command::ReorderChannels {
        server: SERVER,
        moves: vec![rune_model::ChannelMove {
            position: 1,
            ..step
        }],
    })
    .await;
    loop {
        match app.next_data().await {
            Event::ChannelChanged(_) => continue,
            Event::Failed {
                failure: Failure::ChannelOrder { .. },
                ..
            } => break,
            other => panic!("{other:?}"),
        }
    }
    assert!(app.requests.try_recv().is_err());
}

#[tokio::test]
async fn a_role_takes_an_emoji_and_a_gradient_only_where_the_server_allows() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    let styled_edit = || Command::EditRole {
        server: SERVER,
        role: RoleId(13),
        edit: RoleEdit {
            icon: Some(Some(rune_provider::RoleIcon::Emoji("🛡️".into()))),
            gradient: Some(vec![rune_model::Rgb(0x00ff00)]),
            ..Default::default()
        },
    };
    app.send(styled_edit()).await;
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::Role { .. },
            ..
        }
    ));
    assert!(app.requests.try_recv().is_err());
    styled(&mut app).await;
    app.send(styled_edit()).await;
    let request = app
        .answer(
            "PATCH /api/v10/guilds/10/roles/13 ",
            response(
                200,
                r#"{"id":"13","name":"Gente","permissions":"0","position":1,"unicode_emoji":"🛡️","colors":{"primary_color":0,"secondary_color":65280,"tertiary_color":null}}"#,
            ),
        )
        .await;
    assert_eq!(
        body(&request),
        json!({"unicode_emoji": "🛡️", "icon": null, "colors": {"primary_color": 0,
            "secondary_color": 0x00ff00, "tertiary_color": null}})
    );
    assert!(matches!(app.next_data().await, Event::ServerChanged(server)
        if server.roles.iter().any(|role| role.id == RoleId(13)
            && role.style.emoji.as_deref() == Some("🛡️"))));
}

#[tokio::test]
async fn role_counts_the_profile_and_ordered_member_pages_are_read() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    app.send(Command::LoadRoleCounts {
        request: 30,
        server: SERVER,
    })
    .await;
    app.answer(
        "GET /api/v10/guilds/10/roles/member-counts ",
        response(200, r#"{"11":1,"13":4}"#),
    )
    .await;
    let Event::RoleCounts { server, mut counts } = app.next_data().await else {
        panic!("counts")
    };
    counts.sort();
    assert_eq!(server, SERVER);
    assert_eq!(counts, [(RoleId(11), 1), (RoleId(13), 4)]);
    app.send(Command::LoadServerProfile {
        request: 31,
        server: SERVER,
    })
    .await;
    app.answer(
        "GET /api/v10/guilds/10/profile ",
        response(
            200,
            r#"{"id":"10","name":"Taller","icon_hash":null,"traits":[{"label":"Arte","emoji_name":null,"emoji_id":null,"position":0}]}"#,
        ),
    )
    .await;
    assert!(matches!(app.next_data().await, Event::ServerChanged(server)
        if server.options.traits.as_ref().is_some_and(|traits| traits.len() == 1)));
    app.send(Command::LoadMembers {
        request: 32,
        server: SERVER,
        after: None,
        query: rune_model::MemberQuery {
            order: rune_model::MemberOrder::OldestAccounts,
            recent: true,
        },
    })
    .await;
    let request = app
        .answer(
            "POST /api/v10/guilds/10/members-search ",
            response(
                200,
                &json!({"guild_id": "10", "page_result_count": 1, "total_result_count": 1,
                    "members": [{"member": {"user": {"id": "45", "username": "nuevo"},
                        "roles": [], "joined_at": "2026-10-16T00:00:00Z"},
                        "join_source_type": 5, "source_invite_code": "abc123"}]})
                .to_string(),
            ),
        )
        .await;
    let sent = body(&request);
    assert_eq!(sent["sort"], 4);
    assert!(sent["and_query"]["guild_joined_at"]["range"]["gte"].is_i64());
    assert!(
        matches!(app.next_data().await, Event::MembersLoaded { query, joins, .. }
        if query.recent && joins == [(UserId(45), rune_model::JoinMethod::Invite(Some("abc123".into())))])
    );
}

#[tokio::test]
async fn a_forum_s_tags_and_defaults_keep_its_other_flags_read_fresh() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    app.dispatch
        .send((
            "CHANNEL_CREATE",
            json!({"id": "50", "guild_id": "10", "type": 15, "name": "ideas",
                "permission_overwrites": [], "flags": 0,
                "available_tags": [{"id": "7", "name": "Ayuda", "moderated": false}]}),
        ))
        .await
        .unwrap();
    loop {
        if matches!(app.running.next().await, Event::ChannelChanged(channel) if channel.id == ChannelId(50))
        {
            break;
        }
    }
    app.send(Command::EditChannel {
        channel: ChannelId(50),
        edit: rune_provider::ChannelEdit {
            tags: Some(vec![rune_model::ForumTag {
                id: rune_model::TagId(0),
                name: "Nueva".into(),
                emoji: Some("✨".into()),
                moderated: true,
            }]),
            forum: Some(rune_model::ForumDefaults {
                require_tag: true,
                reaction: Some("👍".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
    })
    .await;
    app.answer(
        "GET /api/v10/channels/50 ",
        response(
            200,
            r#"{"id":"50","guild_id":"10","type":15,"name":"ideas","flags":2}"#,
        ),
    )
    .await;
    let request = app
        .answer(
            "PATCH /api/v10/channels/50 ",
            response(
                200,
                r#"{"id":"50","guild_id":"10","type":15,"name":"ideas","flags":18,"default_reaction_emoji":{"emoji_id":null,"emoji_name":"👍"},"available_tags":[{"id":"8","name":"Nueva","moderated":true,"emoji_name":"✨"}]}"#,
            ),
        )
        .await;
    let sent = body(&request);
    assert_eq!(sent["flags"], 2 | 1 << 4);
    assert!(sent["available_tags"][0].get("id").is_none());
    assert!(
        matches!(app.next_data().await, Event::ChannelChanged(channel)
        if channel.tags.len() == 1 && channel.forum.require_tag
            && channel.forum.reaction.as_deref() == Some("👍"))
    );
}

#[tokio::test]
async fn permission_changes_are_asked_for_by_each_of_their_types_and_merged() {
    let mut app = Harness::new().await;
    moderated(&mut app).await;
    let query = AuditQuery {
        actor: None,
        action: Some(AuditActionKind::PermissionsChanged),
    };
    app.send(Command::LoadAudit {
        request: 4,
        server: SERVER,
        query,
        before: None,
        limit: 2,
    })
    .await;
    let row = |id: u64, kind: u64| {
        json!({"id": id.to_string(), "user_id": "42", "target_id": "30", "action_type": kind,
            "options": {"id": "13", "type": "0"}})
    };
    let page =
        |rows: Vec<serde_json::Value>| json!({"users": [], "audit_log_entries": rows}).to_string();
    // An overwrite made, changed and removed: one type per request.
    app.answer(
        "GET /api/v10/guilds/10/audit-logs?limit=2&action_type=13 ",
        response(200, &page(vec![row(95, 13)])),
    )
    .await;
    app.answer(
        "GET /api/v10/guilds/10/audit-logs?limit=2&action_type=14 ",
        response(200, &page(vec![row(99, 14), row(90, 14)])),
    )
    .await;
    app.answer(
        "GET /api/v10/guilds/10/audit-logs?limit=2&action_type=15 ",
        response(200, &page(vec![row(85, 15)])),
    )
    .await;
    let Event::AuditLoaded {
        entries,
        reached_end,
        ..
    } = app.next_data().await
    else {
        panic!("the audit log");
    };
    // The newest two of all three; older ones wait for the next page.
    assert_eq!(
        entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        [99, 95]
    );
    assert!(entries.iter().all(|entry| matches!(
        entry.action,
        AuditAction::PermissionsChanged { channel, .. } if channel == ChannelId(30)
    )));
    assert!(!reached_end);
    // The next page starts after the last one shown, for every type.
    app.send(Command::LoadAudit {
        request: 5,
        server: SERVER,
        query,
        before: Some(95),
        limit: 2,
    })
    .await;
    app.answer(
        "GET /api/v10/guilds/10/audit-logs?limit=2&action_type=13&before=95 ",
        response(200, &page(Vec::new())),
    )
    .await;
    app.answer(
        "GET /api/v10/guilds/10/audit-logs?limit=2&action_type=14&before=95 ",
        response(200, &page(vec![row(90, 14)])),
    )
    .await;
    app.answer(
        "GET /api/v10/guilds/10/audit-logs?limit=2&action_type=15&before=95 ",
        response(200, &page(vec![row(85, 15)])),
    )
    .await;
    let Event::AuditLoaded {
        entries,
        reached_end,
        ..
    } = app.next_data().await
    else {
        panic!("the audit log");
    };
    assert_eq!(
        entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        [90, 85]
    );
    assert!(reached_end);
}
