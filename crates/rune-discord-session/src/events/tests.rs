use super::*;
use rune_model::{ChannelId, Server};

fn stage_talk() -> Value {
    json!({"id": "90", "guild_id": "10", "channel_id": "32", "creator_id": "42",
        "name": "Charla", "description": null,
        "scheduled_start_time": "2030-01-02T18:00:00+00:00", "scheduled_end_time": null,
        "privacy_level": 2, "status": 2, "entity_type": 1, "entity_metadata": null,
        "image": "0123456789abcdef0123456789abcdef"})
}

#[test]
fn each_place_and_status_is_read_and_unknown_ones_are_left_out() {
    let talk = event(&stage_talk(), Some(ServerId(10))).unwrap().unwrap();
    assert_eq!(talk.place, EventPlace::Stage(ChannelId(32)));
    assert_eq!(talk.status, EventStatus::Active);
    assert_eq!(talk.creator, Some(UserId(42)));
    assert_eq!(talk.interested, None);
    assert!(talk.image.as_ref().is_some_and(|image| image
        .url
        .starts_with("https://cdn.discordapp.com/guild-events/90/")));
    let mut outside = stage_talk();
    outside["entity_type"] = json!(3);
    outside["channel_id"] = Value::Null;
    outside["entity_metadata"] = json!({"location": "Plaza"});
    outside["scheduled_end_time"] = json!("2030-01-02T20:00:00+00:00");
    let outside = event(&outside, None).unwrap().unwrap();
    assert_eq!(outside.place, EventPlace::External("Plaza".into()));
    assert!(outside.end.is_some());
    // A kind or a status RUNE does not know is not shown; another server's
    // event, or one without a name, is malformed.
    for (field, value) in [("entity_type", json!(4)), ("status", json!(9))] {
        let mut unknown = stage_talk();
        unknown[field] = value;
        assert_eq!(event(&unknown, None).unwrap(), None);
    }
    assert!(event(&stage_talk(), Some(ServerId(11))).is_err());
    let mut unnamed = stage_talk();
    unnamed["name"] = json!("");
    assert!(event(&unnamed, None).is_err());
    let list = json!([stage_talk(), {"id": "91", "guild_id": "10", "entity_type": 4, "status": 1}]);
    assert_eq!(events(&list, ServerId(10)).unwrap().len(), 1);
}

#[test]
fn what_interests_the_account_and_who_else_are_read_in_both_shapes() {
    let answered = json!([
        {"guild_scheduled_event_id": "90", "user_id": "42", "response": 1},
        {"guild_scheduled_event_id": "91", "user_id": "42"},
        {"guild_scheduled_event_id": "92", "user_id": "42", "response": 0},
        {"guild_scheduled_event_id": "93", "user_id": "42", "response": 1,
            "guild_scheduled_event_exception_id": "5"}
    ]);
    assert_eq!(
        interests(&answered).unwrap(),
        [ScheduledEventId(90), ScheduledEventId(91)]
    );
    let person =
        json!({"guild_scheduled_event_id": "90", "user": {"id": "43", "username": "Peer"}});
    assert_eq!(
        interested(&json!([person.clone()])).unwrap()[0].id,
        UserId(43)
    );
    assert_eq!(interested(&json!({"users": [person]})).unwrap().len(), 1);
    assert!(interested(&json!({"nobody": []})).is_err());
}

#[test]
fn a_draft_goes_with_its_place_and_times_and_leaves_the_cover_alone() {
    let sent = body(&EventDraft {
        name: " Repaso ".into(),
        description: Some("  ".into()),
        start: Timestamp(1_893_607_200_000),
        end: None,
        place: EventPlace::Voice(ChannelId(31)),
    })
    .unwrap();
    assert_eq!(
        sent,
        json!({"name": "Repaso", "description": null, "privacy_level": 2,
            "scheduled_start_time": "2030-01-02T18:00:00.000Z", "scheduled_end_time": null,
            "entity_type": 2, "channel_id": "31", "entity_metadata": null})
    );
    assert_eq!(status(EventStatus::Completed), 3);
}

#[test]
fn the_gateway_reports_only_this_sessions_servers_and_whole_events() {
    let snapshot = Snapshot {
        servers: vec![Server {
            emojis: Vec::new(),
            id: ServerId(10),
            name: "Taller".into(),
            accent: rune_model::Rgb(0),
            icon: None,
            roles: Vec::new(),
            stickers: Vec::new(),
            owner: None,
            description: None,
            options: Default::default(),
        }],
        ..Snapshot::default()
    };
    assert!(dispatch(&snapshot, "MESSAGE_CREATE", &json!({})).is_none());
    let other = json!({"guild_id": "11", "guild_scheduled_event_id": "90", "user_id": "43"});
    assert_eq!(
        dispatch(&snapshot, "GUILD_SCHEDULED_EVENT_USER_ADD", &other),
        Some(Vec::new())
    );
    let occurrence = json!({"guild_id": "10", "guild_scheduled_event_id": "90", "user_id": "43",
        "guild_scheduled_event_exception_id": "7"});
    assert_eq!(
        dispatch(&snapshot, "GUILD_SCHEDULED_EVENT_USER_REMOVE", &occurrence),
        Some(Vec::new())
    );
    let changed = dispatch(&snapshot, "GUILD_SCHEDULED_EVENT_CREATE", &stage_talk()).unwrap();
    assert!(matches!(
        &changed[..],
        [Event::ScheduledEventChanged { request: None, .. }]
    ));
}

#[test]
fn an_invitation_that_names_an_event_brings_it_and_keeps_its_link() {
    let target = crate::invitations::target("https://discord.gg/synthetic?event=90").unwrap();
    assert_eq!(target.key, "synthetic");
    assert_eq!(target.url, "https://discord.com/invite/synthetic?event=90");
    // A malformed one is the server's plain invitation.
    let plain = crate::invitations::target("https://discord.gg/synthetic?event=x").unwrap();
    assert_eq!(plain.url, "https://discord.com/invite/synthetic");
}
