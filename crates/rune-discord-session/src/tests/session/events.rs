//! A server's scheduled events through the session's checked queue, against
//! local fixtures only: what leaves for each route, what comes back, what
//! the Gateway reports and the hCaptcha a write may meet. Nothing here
//! reached Discord.

use super::*;
use rune_model::{EventDraft, EventPlace, EventStatus, ScheduledEventId, ServerId, UserId};

const SERVER: ServerId = ServerId(10);
const ME: UserId = UserId(42);
const MEETING: ScheduledEventId = ScheduledEventId(90);

fn body(request: &str) -> Value {
    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
}

/// A server with a voice room: the account's own (`owner` 42), or one
/// where it has nothing but `@everyone`'s.
async fn joined(app: &mut Harness, owner: &str) {
    app.dispatch
        .send((
            "GUILD_CREATE",
            json!({"id": "10", "name": "Taller", "owner_id": owner,
                "roles": [{"id": "10", "name": "@everyone",
                    "permissions": (1u64 << 10 | 1 << 11 | 1 << 20).to_string(), "position": 0}],
                "members": [{"user": {"id": "42", "username": "Owner"}, "roles": []}],
                "channels": [
                    {"id": "30", "guild_id": "10", "type": 0, "name": "general", "permission_overwrites": []},
                    {"id": "31", "guild_id": "10", "type": 2, "name": "sala", "permission_overwrites": []}]}),
        ))
        .await
        .unwrap();
    loop {
        if matches!(app.running.next().await, Event::ChannelChanged(channel) if channel.id == ChannelId(31))
        {
            break;
        }
    }
}

const MEETING_JSON: &str = r#"{"id": "90", "guild_id": "10", "channel_id": "31",
    "creator_id": "42", "name": "Repaso", "description": "Del martes",
    "scheduled_start_time": "2030-01-02T18:00:00+00:00", "scheduled_end_time": null,
    "privacy_level": 2, "status": 1, "entity_type": 2, "entity_id": null,
    "entity_metadata": null, "user_count": 3, "image": null}"#;

fn draft() -> EventDraft {
    EventDraft {
        name: "Asado".into(),
        description: None,
        start: rune_model::Timestamp(1_893_607_200_000),
        end: Some(rune_model::Timestamp(1_893_621_600_000)),
        place: EventPlace::External("Parque".into()),
    }
}

#[tokio::test]
async fn events_are_read_whole_marked_once_and_kept_current_by_the_gateway() {
    let mut app = Harness::new().await;
    joined(&mut app, "42").await;
    app.running
        .commands
        .send(Command::LoadScheduledEvents {
            request: 1,
            server: SERVER,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("GET /api/v10/guilds/10/scheduled-events?with_user_count=true "),
        "{request}"
    );
    reply
        .send(response(200, &format!("[{MEETING_JSON}]")))
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("GET /api/v10/users/@me/scheduled-events?guild_ids=10 "),
        "{request}"
    );
    reply
        .send(response(
            200,
            r#"[{"guild_scheduled_event_id": "90", "user_id": "42", "response": 1}]"#,
        ))
        .unwrap();
    let Event::ScheduledEvents {
        request: 1, events, ..
    } = app.next_data().await
    else {
        panic!("the events")
    };
    assert_eq!(events.len(), 1);
    assert!(events[0].account_interested && events[0].interested == Some(3));
    assert_eq!(events[0].place, EventPlace::Voice(ChannelId(31)));

    // «Me interesa» off, and the account is named in the answer.
    app.running
        .commands
        .send(Command::SetEventInterest {
            request: 2,
            server: SERVER,
            event: MEETING,
            interested: false,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("DELETE /api/v10/guilds/10/scheduled-events/90/users/@me "),
        "{request}"
    );
    reply
        .send("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into())
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::EventInterest {
            request: Some(2),
            server: SERVER,
            event: MEETING,
            user: ME,
            interested: false,
        }
    );
    // And on again, with the official client's body.
    app.running
        .commands
        .send(Command::SetEventInterest {
            request: 3,
            server: SERVER,
            event: MEETING,
            interested: true,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("PUT /api/v10/guilds/10/scheduled-events/90/users/@me "),
        "{request}"
    );
    assert_eq!(body(&request), json!({"response": 1}));
    reply.send(response(200, "{}")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::EventInterest {
            user: ME,
            interested: true,
            ..
        }
    ));

    // What the Gateway reports.
    let mut updated: Value = serde_json::from_str(MEETING_JSON).unwrap();
    updated["status"] = json!(2);
    app.dispatch
        .send(("GUILD_SCHEDULED_EVENT_UPDATE", updated))
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::ScheduledEventChanged { request: None, event } if event.status == EventStatus::Active
    ));
    app.dispatch
        .send((
            "GUILD_SCHEDULED_EVENT_USER_ADD",
            json!({"guild_id": "10", "guild_scheduled_event_id": "90", "user_id": "43"}),
        ))
        .await
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::EventInterest {
            request: None,
            server: SERVER,
            event: MEETING,
            user: UserId(43),
            interested: true,
        }
    );
    app.dispatch
        .send((
            "GUILD_SCHEDULED_EVENT_DELETE",
            serde_json::from_str(MEETING_JSON).unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::ScheduledEventDeleted {
            request: None,
            server: SERVER,
            event: MEETING,
        }
    );
}

#[tokio::test]
async fn planning_meets_a_challenge_and_goes_once_with_the_answer() {
    let mut app = Harness::new().await;
    joined(&mut app, "42").await;
    app.running
        .commands
        .send(Command::CreateScheduledEvent {
            request: 4,
            server: SERVER,
            draft: draft(),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("POST /api/v10/guilds/10/scheduled-events "),
        "{request}"
    );
    let sent = body(&request);
    assert_eq!(sent["entity_type"], json!(3));
    assert_eq!(sent["privacy_level"], json!(2));
    assert_eq!(sent["entity_metadata"], json!({"location": "Parque"}));
    assert_eq!(sent["channel_id"], Value::Null);
    assert_eq!(
        sent["scheduled_start_time"],
        json!("2030-01-02T18:00:00.000Z")
    );
    reply
        .send(response(400, super::challenges::CHALLENGE))
        .unwrap();
    let Event::ChallengeRequired { challenge, .. } = app.next_data().await else {
        panic!("offered to the person")
    };
    super::challenges::solve(&mut app, challenge).await;
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("POST /api/v10/guilds/10/scheduled-events "),
        "{request}"
    );
    super::challenges::answered(&request);
    let mut created: Value = serde_json::from_str(MEETING_JSON).unwrap();
    created["id"] = json!("91");
    created["entity_type"] = json!(3);
    created["channel_id"] = Value::Null;
    created["entity_metadata"] = json!({"location": "Parque"});
    created["scheduled_end_time"] = json!("2030-01-02T22:00:00+00:00");
    reply.send(response(200, &created.to_string())).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::ScheduledEventChanged { request: Some(4), event }
            if event.id == ScheduledEventId(91) && event.place == EventPlace::External("Parque".into())
    ));
}

#[tokio::test]
async fn without_permission_nothing_leaves() {
    let mut app = Harness::new().await;
    joined(&mut app, "99").await;
    app.running
        .commands
        .send(Command::DeleteScheduledEvent {
            request: 5,
            server: SERVER,
            event: MEETING,
        })
        .await
        .unwrap();
    let Event::Failed { failure, .. } = app.next_data().await else {
        panic!("refused here")
    };
    assert_eq!(
        failure,
        rune_provider::Failure::ScheduledEvent {
            request: 5,
            server: SERVER
        }
    );
    assert!(app.requests.try_recv().is_err(), "nothing went to Discord");
}
