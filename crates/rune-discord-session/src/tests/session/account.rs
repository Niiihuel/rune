//! What the account keeps with Discord, through the whole provider: its
//! status on the Gateway and in `settings-proto/1`, notifications, folders,
//! privacy and notes. Local fixtures only; the harness answers each read of
//! the settings with none set.

use super::*;
use rune_model::{
    NotificationChange, PrivacyChange, Rgb, ServerFolder, ServerId, SpamFilter, Status, Timestamp,
    UserId, MUTED_FOREVER,
};
use rune_provider::Failure;

use crate::settings_proto;

fn body(request: &str) -> Value {
    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
}

/// What Discord keeps after `request`, a `PATCH` of the settings, over none.
fn kept(request: &str) -> String {
    let before = settings_proto::decode(&settings_proto::answer(1, &[])).unwrap();
    settings_proto::applied(&before, &body(request)).to_string()
}

async fn presence_update(app: &mut Harness) -> Value {
    tokio::time::timeout(Duration::from_secs(1), app.presences.recv())
        .await
        .expect("a presence update")
        .unwrap()["d"]
        .clone()
}

async fn next_account(app: &mut Harness) -> Event {
    loop {
        let event = app.running.next_any().await;
        if !matches!(event, Event::UploadLimit { .. } | Event::Connection(_)) {
            return event;
        }
    }
}

#[tokio::test]
async fn the_account_starts_invisible_then_shows_its_status_and_changes_it_on_both_paths() {
    let mut app = Harness::new().await;
    // Read with none set: online, and the Gateway is told so.
    assert_eq!(
        presence_update(&mut app).await,
        json!({"since": null, "activities": [], "status": "online", "afk": false})
    );
    app.running
        .commands
        .send(Command::SetCustomStatus {
            text: Some(" De viaje ".into()),
            emoji: Some("🌙".into()),
            until: Some(Timestamp(1_900_000_000_000)),
        })
        .await
        .unwrap();
    // The Gateway hears it at once, as a custom status activity.
    assert_eq!(
        presence_update(&mut app).await["activities"],
        json!([{"name": "Custom Status", "type": 4, "state": "De viaje", "emoji": {"name": "🌙"}}])
    );
    // The account keeps it: read, then written over that version.
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PATCH /api/v10/users/@me/settings-proto/1 "));
    assert_eq!(body(&request)["required_data_version"], 1);
    reply.send(response(200, &kept(&request))).unwrap();
    assert!(matches!(next_account(&mut app).await,
        Event::PresenceChanged { user: UserId(42), presence }
            if presence.custom.as_deref() == Some("🌙 De viaje") && presence.status == Status::Online));
    // A refused status goes back to what the account keeps.
    app.running
        .commands
        .send(Command::SetStatus(Status::DoNotDisturb))
        .await
        .unwrap();
    assert_eq!(presence_update(&mut app).await["status"], "dnd");
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PATCH /api/v10/users/@me/settings-proto/1 "));
    reply.send(response(403, "{}")).unwrap();
    assert!(matches!(
        next_account(&mut app).await,
        Event::Failed {
            failure: Failure::Status,
            ..
        }
    ));
    let back = presence_update(&mut app).await;
    assert_eq!(back["status"], "online");
    assert_eq!(back["activities"][0]["state"], "De viaje");
}

#[tokio::test]
async fn other_sessions_and_other_devices_are_followed() {
    let mut app = Harness::new().await;
    presence_update(&mut app).await;
    // What every session shows.
    app.dispatch
        .send((
            "SESSIONS_REPLACE",
            json!([{"session_id": "phone", "status": "online"}, {"session_id": "all", "status": "dnd"}]),
        ))
        .await
        .unwrap();
    assert!(matches!(next_account(&mut app).await,
        Event::PresenceChanged { user: UserId(42), presence } if presence.status == Status::DoNotDisturb));
    // Another device changed the settings: they are read again.
    app.dispatch
        .send((
            "USER_SETTINGS_PROTO_UPDATE",
            json!({"settings": {"type": 1, "proto": "WgA="}, "partial": true}),
        ))
        .await
        .unwrap();
    assert!(matches!(
        next_account(&mut app).await,
        Event::PresenceChanged { .. }
    ));
    assert_eq!(
        next_account(&mut app).await,
        Event::ServerFolders(Vec::new())
    );
    assert!(matches!(
        next_account(&mut app).await,
        Event::MessagingPrivacy(_)
    ));
    assert_eq!(next_account(&mut app).await, Event::ActivitySharing(true));
    // Notifications changed elsewhere, and a note.
    app.dispatch
        .send((
            "USER_GUILD_SETTINGS_UPDATE",
            json!({"guild_id": null, "channel_overrides": [{"channel_id": "20", "muted": true, "mute_config": null}]}),
        ))
        .await
        .unwrap();
    assert!(matches!(next_account(&mut app).await,
        Event::NotificationSettings { settings, replace: false }
            if settings[0].server.is_none() && settings[0].channels[0].muted_until == Some(MUTED_FOREVER)));
    app.dispatch
        .send((
            "USER_NOTE_UPDATE",
            json!({"id": "43", "note": "de la facu"}),
        ))
        .await
        .unwrap();
    assert_eq!(
        next_account(&mut app).await,
        Event::NoteChanged {
            user: UserId(43),
            note: "de la facu".into()
        }
    );
}

#[tokio::test]
async fn a_direct_conversation_is_muted_for_an_hour_and_the_answer_confirms_it() {
    let mut app = Harness::new().await;
    let until = Timestamp(crate::messages::wall_now().0 + 3_600_000);
    app.running
        .commands
        .send(Command::SetNotifications {
            server: None,
            change: NotificationChange::ChannelMute(ChannelId(20), Some(until)),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PATCH /api/v10/users/@me/guilds/@me/settings "));
    let sent = body(&request);
    let row = &sent["channel_overrides"]["20"];
    assert_eq!(row["muted"], true);
    assert!(row["mute_config"]["end_time"]
        .as_str()
        .unwrap()
        .ends_with('Z'));
    assert!((3599..=3600).contains(&row["mute_config"]["selected_time_window"].as_i64().unwrap()));
    let end = row["mute_config"]["end_time"].clone();
    reply
        .send(response(
            200,
            &json!({"guild_id": null, "channel_overrides": [
                {"channel_id": "20", "muted": true, "mute_config": {"end_time": end}}
            ]})
            .to_string(),
        ))
        .unwrap();
    assert!(matches!(next_account(&mut app).await,
        Event::NotificationSettings { settings, replace: false }
            if settings[0].channels[0].muted_until.is_some_and(|at| at != MUTED_FOREVER)));
    // An answer that does not show the change is not taken for it.
    app.running
        .commands
        .send(Command::SetNotifications {
            server: None,
            change: NotificationChange::ChannelMute(ChannelId(20), None),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert_eq!(
        body(&request)["channel_overrides"]["20"],
        json!({"muted": false, "mute_config": null})
    );
    reply
        .send(response(
            200,
            &json!({"guild_id": null, "channel_overrides": [{"channel_id": "20", "muted": true}]})
                .to_string(),
        ))
        .unwrap();
    assert!(matches!(
        next_account(&mut app).await,
        Event::Failed {
            failure: Failure::Notifications { server: None },
            ambiguous: true,
            ..
        }
    ));
    // A server the account is not in never gets a request.
    app.running
        .commands
        .send(Command::SetNotifications {
            server: Some(ServerId(9)),
            change: NotificationChange::Mute(None),
        })
        .await
        .unwrap();
    assert!(matches!(
        next_account(&mut app).await,
        Event::Failed {
            failure: Failure::Notifications { .. },
            ..
        }
    ));
}

#[tokio::test]
async fn folders_and_privacy_are_written_over_what_was_read() {
    let mut app = Harness::new().await;
    let folders = vec![ServerFolder {
        id: Some(3),
        name: Some("Amigos".into()),
        color: Some(Rgb(0x5865F2)),
        servers: vec![ServerId(1), ServerId(2)],
    }];
    app.running
        .commands
        .send(Command::SetServerFolders(folders.clone()))
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PATCH /api/v10/users/@me/settings-proto/1 "));
    reply.send(response(200, &kept(&request))).unwrap();
    assert_eq!(
        next_account(&mut app).await,
        Event::ServerFolders(folders.clone())
    );
    // Discord, as the harness answers, has none again: another device
    // changed them, so nothing is written and they are shown as they are.
    app.running
        .commands
        .send(Command::SetServerFolders(Vec::new()))
        .await
        .unwrap();
    assert!(matches!(
        next_account(&mut app).await,
        Event::Failed { failure: Failure::Folders, reason, .. } if reason.contains("otro dispositivo")
    ));
    assert!(matches!(
        next_account(&mut app).await,
        Event::PresenceChanged { .. }
    ));
    assert_eq!(
        next_account(&mut app).await,
        Event::ServerFolders(Vec::new())
    );
    // The privacy and the activity sharing read with them.
    next_account(&mut app).await;
    assert_eq!(next_account(&mut app).await, Event::ActivitySharing(true));
    app.running
        .commands
        .send(Command::SetMessagingPrivacy(PrivacyChange::Spam(
            SpamFilter::Everyone,
        )))
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    reply.send(response(200, &kept(&request))).unwrap();
    assert!(matches!(next_account(&mut app).await,
        Event::MessagingPrivacy(privacy) if privacy.spam == SpamFilter::Everyone));
}

#[tokio::test]
async fn notes_read_empty_when_never_written_and_are_kept() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::LoadNote { user: UserId(43) })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/users/@me/notes/43 "));
    reply.send(response(404, r#"{"code":10013}"#)).unwrap();
    assert_eq!(
        next_account(&mut app).await,
        Event::NoteChanged {
            user: UserId(43),
            note: String::new()
        }
    );
    app.running
        .commands
        .send(Command::SetNote {
            user: UserId(43),
            note: " compañera de la facu ".into(),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PUT /api/v10/users/@me/notes/43 "));
    assert_eq!(body(&request), json!({"note": "compañera de la facu"}));
    reply
        .send("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into())
        .unwrap();
    assert_eq!(
        next_account(&mut app).await,
        Event::NoteChanged {
            user: UserId(43),
            note: "compañera de la facu".into()
        }
    );
}

#[tokio::test]
async fn a_challenge_on_the_settings_is_solved_by_the_person_and_sent_once() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::SetMessagingPrivacy(PrivacyChange::ServerDms(
            false,
        )))
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PATCH /api/v10/users/@me/settings-proto/1 "));
    reply.send(response(400, challenges::CHALLENGE)).unwrap();
    let Event::ChallengeRequired { challenge, failure } = next_account(&mut app).await else {
        panic!("a challenge first")
    };
    assert_eq!(failure, Failure::Privacy);
    challenges::solve(&mut app, challenge).await;
    // The settings are read again and the same write goes with the answer.
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PATCH /api/v10/users/@me/settings-proto/1 "));
    challenges::answered(&request);
    reply.send(response(200, &kept(&request))).unwrap();
    assert!(matches!(next_account(&mut app).await,
        Event::MessagingPrivacy(privacy) if !privacy.server_dms));
}

// Desktop session and games (escritorio-sesion-y-juegos).
#[tokio::test]
async fn activity_sharing_writes_only_its_field_and_the_sessions_say_who_sees_the_game() {
    let mut app = Harness::new().await;
    presence_update(&mut app).await;
    app.running
        .commands
        .send(Command::SetActivitySharing(false))
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PATCH /api/v10/users/@me/settings-proto/1 "));
    // Field 11 holding only `show_current_game` = false, over version 1.
    assert_eq!(
        body(&request),
        json!({"settings": "WgQaAggA", "required_data_version": 1})
    );
    reply.send(response(200, &kept(&request))).unwrap();
    assert_eq!(next_account(&mut app).await, Event::ActivitySharing(false));
    // A game this device publishes, and what the sessions say of it.
    app.running
        .commands
        .send(Command::SetLocalActivity(Some(rune_model::LocalActivity {
            application: Some(77),
            name: Some("Juego".into()),
            source: rune_model::ActivitySource::Game,
            ..Default::default()
        })))
        .await
        .unwrap();
    assert_eq!(
        presence_update(&mut app).await["activities"][0]["application_id"],
        "77"
    );
    let game = json!({"name": "Juego", "type": 0, "application_id": "77"});
    app.dispatch
        .send((
            "SESSIONS_REPLACE",
            json!([{"session_id": "all", "status": "online", "activities": [], "hidden_activities": [game]}]),
        ))
        .await
        .unwrap();
    assert_eq!(
        next_account(&mut app).await,
        Event::ActivityVisibility(rune_model::ActivityVisibility::Hidden)
    );
}
