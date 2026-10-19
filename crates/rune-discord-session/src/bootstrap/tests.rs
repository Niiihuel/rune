use super::*;
use rune_provider::Command;
use serde_json::json;

fn me() -> User {
    wire::user(&json!({"id":"9", "username":"synthetic"})).unwrap()
}

fn account() -> Value {
    json!({
        "user":{"id":"9"},
        "users":[{"id":"8","username":"friend"}],
        "guilds":[{
            "id":"10", "properties":{"name":"Taller","owner_id":"99"},
            "roles":[{"id":"10","name":"@everyone","permissions":"66560"},
                     {"id":"11","name":"writers","permissions":"2048"}],
            "channels":[{"id":"20","type":0,"name":"general","permission_overwrites":[],"last_message_id":"100"},
                        {"id":"21","type":0,"name":"hidden","permission_overwrites":[{"id":"10","type":0,"allow":"0","deny":"1024"}]}]
        }],
        "merged_members":[[{"user_id":"9","roles":["11"]}]],
        "private_channels":[{"id":"30","type":1,"recipient_ids":["8"]}],
        "read_state":{"entries":[{"id":"20","last_message_id":"90","mention_count":2}]}
    })
}

#[test]
fn account_navigation_resolves_merged_members_recipients_and_read_state() {
    let snapshot = ready(&account(), &me()).unwrap();
    assert_eq!(snapshot.servers[0].name, "Taller");
    let public = snapshot
        .channels
        .iter()
        .find(|c| c.id == ChannelId(20))
        .unwrap();
    assert!(public
        .permissions
        .contains(Permissions::VIEW | Permissions::SEND | Permissions::READ_HISTORY));
    let hidden = snapshot
        .channels
        .iter()
        .find(|c| c.id == ChannelId(21))
        .unwrap();
    assert_eq!(hidden.permissions, Permissions::NONE);
    let dm = snapshot
        .channels
        .iter()
        .find(|c| c.id == ChannelId(30))
        .unwrap();
    assert_eq!(dm.name, "friend");
    assert_eq!(dm.recipients, vec![UserId(8)]);
    assert!(dm.permissions.contains(Permissions::SEND));
    assert_eq!(
        snapshot.read_states,
        vec![(ChannelId(20), Some(MessageId(90)), 2)]
    );
    assert_eq!(
        snapshot.last_messages,
        vec![(ChannelId(20), MessageId(100))]
    );
}

#[test]
fn missing_roles_members_and_overwrites_never_grant_permissions() {
    let mut value = account();
    value["guilds"][0]["roles"] = Value::Null;
    let snapshot = ready(&value, &me()).unwrap();
    assert_eq!(snapshot.channels[0].permissions, Permissions::NONE);
    let mut value = account();
    value["merged_members"] = Value::Null;
    let snapshot = ready(&value, &me()).unwrap();
    assert_eq!(snapshot.channels[0].permissions, Permissions::NONE);
    let mut value = account();
    value["guilds"][0]["channels"][0]["permission_overwrites"] = Value::Null;
    let snapshot = ready(&value, &me()).unwrap();
    assert!(!snapshot.channels.iter().any(|c| c.id == ChannelId(20)));
}

#[test]
fn permission_translation_preserves_high_bits_without_aliasing() {
    let bits = ((1_u128 << 38) | (1_u128 << 40) | (1_u128 << 100)).to_string();
    let permissions = permission_bits(&json!(bits)).unwrap();
    assert_eq!(
        permissions,
        Permissions::SEND_IN_THREADS | Permissions::TIMEOUT_MEMBERS
    );
    assert!(!permissions.contains(Permissions::ADMINISTRATOR));
    for bad in [
        json!("-1"),
        json!("1.0"),
        json!("x"),
        json!("9".repeat(40)),
        Value::Null,
    ] {
        assert!(permission_bits(&bad).is_err());
    }
}

#[test]
fn malformed_permission_evidence_and_wrong_account_are_rejected() {
    let mut value = account();
    value["user"]["id"] = json!("777");
    assert!(ready(&value, &me()).is_err());
    let mut value = account();
    value["merged_members"] = json!([[], []]);
    assert!(ready(&value, &me()).is_err());
    let mut value = account();
    value["guilds"][0]["channels"][0]["permission_overwrites"] =
        json!([{"id":"10","type":0,"allow":"0","deny":"invalid"}]);
    assert!(ready(&value, &me()).is_err());
    let mut value = account();
    value["guilds"][0]["channels"][0]["guild_id"] = json!("999");
    assert!(ready(&value, &me()).is_err());
}

#[test]
fn unknown_channel_kinds_and_obfuscated_channels_are_not_invented() {
    let mut value = account();
    value["guilds"][0]["channels"][0]["type"] = json!(14);
    value["guilds"][0]["channels"][1]["flags"] = json!(1_u64 << 17);
    let snapshot = ready(&value, &me()).unwrap();
    assert_eq!(snapshot.channels.len(), 1);
    assert_eq!(snapshot.channels[0].kind, ChannelKind::Direct);
}

#[test]
fn role_and_member_changes_recompute_permissions_before_emitting() {
    let mut snapshot = ready(&account(), &me()).unwrap();
    let events = update(
        &mut snapshot,
        "GUILD_ROLE_UPDATE",
        &json!({"guild_id":"10","role":{"id":"11","name":"writers","permissions":"0"}}),
    )
    .unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::ServerChanged(_))));
    assert!(!snapshot.channels[0].permissions.contains(Permissions::SEND));
    let events = update(
        &mut snapshot,
        "CHANNEL_UPDATE",
        &json!({"id":"20","permission_overwrites":[{"id":"9","type":1,"allow":"0","deny":"2048"}]}),
    )
    .unwrap();
    assert!(events.iter().any(|e| matches!(e,Event::ChannelChanged(c) if c.id == ChannelId(20) && !c.permissions.contains(Permissions::SEND))));
    let events = update(
        &mut snapshot,
        "GUILD_MEMBER_REMOVE",
        &json!({"guild_id":"10","user":{"id":"9"}}),
    )
    .unwrap();
    assert!(events.iter().any(|e| matches!(e,Event::ChannelChanged(c) if c.id == ChannelId(20) && c.permissions.is_empty())));
}

#[test]
fn partial_channel_update_keeps_denies_and_clears_nullable_topic() {
    let mut snapshot = ready(&account(), &me()).unwrap();
    update(
        &mut snapshot,
        "CHANNEL_UPDATE",
        &json!({"id":"21","name":"renamed","topic":"first\nsecond"}),
    )
    .unwrap();
    let channel = snapshot
        .channels
        .iter()
        .find(|c| c.id == ChannelId(21))
        .unwrap();
    assert_eq!(channel.permissions, Permissions::NONE);
    assert_eq!(channel.topic.as_deref(), Some("first\nsecond"));
    // A topic's server emoji, as pictures; they go with the topic.
    update(
        &mut snapshot,
        "CHANNEL_UPDATE",
        &json!({"id":"21","topic":"hola <a:rana:15>"}),
    )
    .unwrap();
    let pictures = &snapshot
        .channels
        .iter()
        .find(|c| c.id == ChannelId(21))
        .unwrap()
        .topic_pictures;
    assert_eq!(pictures.len(), 1);
    assert_eq!(pictures[0].token, "<a:rana:15>");
    assert!(pictures[0]
        .image
        .url
        .ends_with("15.webp?size=48&animated=true"));
    update(
        &mut snapshot,
        "CHANNEL_UPDATE",
        &json!({"id":"21","topic":null}),
    )
    .unwrap();
    let channel = snapshot
        .channels
        .iter()
        .find(|c| c.id == ChannelId(21))
        .unwrap();
    assert!(channel.topic.is_none() && channel.topic_pictures.is_empty());
}

#[test]
fn supplemental_own_membership_unlocks_only_verified_channels() {
    let mut value = account();
    value["merged_members"] = Value::Null;
    let mut snapshot = ready(&value, &me()).unwrap();
    assert_eq!(snapshot.channels[0].permissions, Permissions::NONE);
    let events = update(
        &mut snapshot,
        "READY_SUPPLEMENTAL",
        &json!({"guilds":[{"id":"10"}],"merged_members":[[{"user_id":"9","roles":["11"]}]]}),
    )
    .unwrap();
    assert!(events.iter().any(|e| matches!(e,Event::ChannelChanged(c) if c.id == ChannelId(20) && c.permissions.contains(Permissions::SEND))));
}

#[test]
fn timeout_screening_and_quarantine_reach_the_member_and_keep_reading() {
    let until = wire::timestamp(&json!("2099-01-01T00:00:00Z")).unwrap();
    for (field, value, expected) in [
        (
            "pending",
            json!(true),
            MemberRestrictions {
                pending: true,
                ..Default::default()
            },
        ),
        (
            "communication_disabled_until",
            json!("2099-01-01T00:00:00Z"),
            MemberRestrictions {
                timeout_until: Some(until),
                ..Default::default()
            },
        ),
        (
            "flags",
            json!(1 << 7),
            MemberRestrictions {
                quarantined: true,
                ..Default::default()
            },
        ),
        (
            "flags",
            json!(1 << 10 | 1 << 1),
            MemberRestrictions {
                tag_quarantined: true,
                // Rules and onboarding (2026-10-18): `COMPLETED_ONBOARDING`.
                onboarded: true,
                ..Default::default()
            },
        ),
        (
            "flags",
            json!(1 << 4),
            MemberRestrictions {
                // A guest (`IS_GUEST`) is never onboarded.
                guest: true,
                ..Default::default()
            },
        ),
    ] {
        let mut data = account();
        data["merged_members"][0][0][field] = value;
        let snapshot = ready(&data, &me()).unwrap();
        assert_eq!(snapshot.members[0].restrictions, expected, "{field}");
        // Reading stays; taking part is refused by the state and the REST gate.
        assert!(snapshot.channels[0]
            .permissions
            .contains(Permissions::VIEW | Permissions::READ_HISTORY));
    }
    for bad in [json!("soon"), json!(12)] {
        let mut data = account();
        data["merged_members"][0][0]["communication_disabled_until"] = bad;
        assert!(ready(&data, &me()).is_err());
    }
}

#[test]
fn member_updates_keep_restrictions_they_omit_and_lift_explicit_ones() {
    let mut snapshot = ready(&account(), &me()).unwrap();
    let update_member = |snapshot: &mut Snapshot, extra: Value| {
        let mut data = json!({"guild_id":"10","user":{"id":"9"},"roles":["11"],
            "joined_at":"2020-05-01T10:00:00.000000+00:00"});
        for (key, value) in extra.as_object().unwrap() {
            data[key] = value.clone();
        }
        update(snapshot, "GUILD_MEMBER_UPDATE", &data).unwrap()
    };
    let events = update_member(
        &mut snapshot,
        json!({"communication_disabled_until":"2099-01-01T00:00:00Z"}),
    );
    assert!(events.iter().any(|e| matches!(e, Event::MemberChanged(m)
        if m.restrictions.timeout_until.is_some() && m.joined.is_some())));
    update_member(&mut snapshot, json!({}));
    assert!(
        snapshot.members[0].restrictions.timeout_until.is_some(),
        "omitted"
    );
    update_member(&mut snapshot, json!({"communication_disabled_until":null}));
    assert_eq!(
        snapshot.members[0].restrictions,
        MemberRestrictions::default()
    );
}

#[test]
fn removal_clears_navigation_reads_and_members() {
    let mut snapshot = ready(&account(), &me()).unwrap();
    let events = update(&mut snapshot, "GUILD_DELETE", &json!({"id":"10"})).unwrap();
    assert_eq!(
        events,
        vec![Event::ServerRemoved {
            server: ServerId(10),
            reason: None
        }]
    );
    assert!(snapshot.servers.is_empty() && snapshot.members.is_empty());
    assert!(snapshot.read_states.is_empty() && snapshot.last_messages.is_empty());
    assert_eq!(snapshot.channels.len(), 1);
}

#[test]
fn duplicate_navigation_and_excessive_items_are_rejected() {
    let mut value = account();
    value["private_channels"] = json!([{"id":"20","type":1}]);
    assert!(ready(&value, &me()).is_err());
    let mut value = account();
    value["guilds"] = json!(vec![json!({"id":"10"}); MAX_GUILDS + 1]);
    assert!(ready(&value, &me()).is_err());
}

#[test]
fn read_state_zero_cursor_and_other_resource_types_are_handled() {
    let mut value = account();
    value["read_state"] = json!([
        {"id":"20","last_message_id":"0","badge_count":3},
        {"id":"21","type":2,"last_message_id":"99"}
    ]);
    let snapshot = ready(&value, &me()).unwrap();
    assert_eq!(snapshot.read_states, vec![(ChannelId(20), None, 3)]);
}

#[test]
fn guild_role_snapshot_can_revoke_permissions_without_a_role_dispatch() {
    let mut snapshot = ready(&account(), &me()).unwrap();
    let events = update(
        &mut snapshot,
        "GUILD_UPDATE",
        &json!({"id":"10","roles":[{"id":"10","permissions":"0"}]}),
    )
    .unwrap();
    assert!(events.iter().any(|e| matches!(e,Event::ChannelChanged(c) if c.id == ChannelId(20) && c.permissions.is_empty())));
}

#[test]
fn supplemental_identity_fragments_preserve_role_evidence() {
    let mut snapshot = ready(&account(), &me()).unwrap();
    update(&mut snapshot,"READY_SUPPLEMENTAL",&json!({"guilds":[{"id":"10"}],"merged_members":[[{"user_id":"9","nick":"Local name"},{"user_id":"88"}]]})).unwrap();
    assert!(snapshot.channels[0].permissions.contains(Permissions::SEND));
    assert_eq!(snapshot.members[0].nick.as_deref(), Some("Local name"));
    assert_eq!(snapshot.members.len(), 1);
}

#[test]
fn conflicting_initial_own_memberships_fail_closed() {
    let mut value = account();
    value["guilds"][0]["members"] = json!([{"user_id":"9","roles":[]}]);
    let snapshot = ready(&value, &me()).unwrap();
    assert_eq!(snapshot.channels[0].permissions, Permissions::NONE);
    // Screening is not a conflict: the merged row omits it, so it stays.
    let mut value = account();
    value["guilds"][0]["members"] = json!([{"user_id":"9","roles":["11"],"pending":true}]);
    let snapshot = ready(&value, &me()).unwrap();
    assert!(snapshot.members[0].restrictions.pending);
}

#[test]
fn malformed_visibility_and_membership_flags_are_not_defaults() {
    for (field, bad) in [("flags", json!("hidden")), ("nsfw", json!("true"))] {
        let mut value = account();
        value["guilds"][0]["channels"][0][field] = bad;
        assert!(ready(&value, &me()).is_err());
    }
    let mut value = account();
    value["merged_members"][0][0]["pending"] = json!("true");
    assert!(ready(&value, &me()).is_err());
}

#[test]
fn deleting_a_role_updates_both_membership_and_permissions() {
    let mut snapshot = ready(&account(), &me()).unwrap();
    let events = update(
        &mut snapshot,
        "GUILD_ROLE_DELETE",
        &json!({"guild_id":"10","role_id":"11"}),
    )
    .unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e,Event::MemberChanged(m) if m.user == UserId(9) && m.roles.is_empty())));
    assert!(events.iter().any(|e| matches!(e,Event::ChannelChanged(c) if c.id == ChannelId(20) && !c.permissions.contains(Permissions::SEND))));
}

#[test]
fn supplemental_conflicts_cannot_restore_access_by_ordering_rows() {
    {
        let first = json!({"user_id":"9","roles":[]});
        let mut snapshot = ready(&account(), &me()).unwrap();
        let events = update(&mut snapshot,"READY_SUPPLEMENTAL",&json!({"guilds":[{"id":"10","members":[first]}],"merged_members":[[{"user_id":"9","roles":["11"]}]]})).unwrap();
        assert_eq!(snapshot.channels[0].permissions, Permissions::NONE);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::MemberRemoved {
                server: ServerId(10),
                user: UserId(9)
            }
        )));
        assert!(!events
            .iter()
            .any(|e| matches!(e,Event::MemberChanged(m) if m.user == UserId(9))));
    }
}

#[test]
fn duplicate_supplemental_guilds_are_rejected_before_applying_membership() {
    for guild in ["10", "999"] {
        let mut snapshot = ready(&account(), &me()).unwrap();
        let members = snapshot.members.clone();
        let channels = snapshot.channels.clone();
        let payload = json!({
            "guilds": [{"id": guild}, {"id": guild}],
            "merged_members": [
                [{"user_id": "9", "roles": ["11"], "pending": true}],
                [{"user_id": "9", "roles": ["11"]}]
            ]
        });
        assert!(update(&mut snapshot, "READY_SUPPLEMENTAL", &payload).is_err());
        assert_eq!(snapshot.members, members);
        assert_eq!(snapshot.channels, channels);
    }
}

#[test]
fn profile_banner_bootstrap_fragments_merge_only_into_the_current_snapshot() {
    use rune_model::ProfileBanner;
    let hash = "0123456789abcdef0123456789abcdef";
    let known = wire::user(&json!({"id":"9","username":"synthetic","banner":hash})).unwrap();
    let mut data = account();
    data["users"] = json!([{"id":"9","username":"renamed"}]);
    let mut snapshot = ready(&data, &known).unwrap();
    assert_eq!(snapshot.users[0].banner, known.banner);
    assert_eq!(snapshot.users[0].name, "renamed");
    add_user(
        &mut snapshot,
        &json!({"id":"9","username":"renamed","banner":null}),
    )
    .unwrap();
    assert_eq!(snapshot.users[0].banner, ProfileBanner::Absent);
    add_user(&mut snapshot, &json!({"id":"9","username":"renamed"})).unwrap();
    assert_eq!(snapshot.users[0].banner, ProfileBanner::Absent);
    add_user(
        &mut snapshot,
        &json!({"id":"9","username":"renamed","banner":hash}),
    )
    .unwrap();
    assert_eq!(snapshot.users[0].banner, known.banner);
    let fresh = ready(&data, &me()).unwrap();
    assert_eq!(fresh.users[0].banner, ProfileBanner::Unknown);
}

fn with_threads() -> Value {
    let mut value = account();
    let guild = &mut value["guilds"][0];
    guild["channels"].as_array_mut().unwrap().extend([
        json!({"id":"22","type":5,"name":"anuncios","permission_overwrites":[]}),
        json!({"id":"23","type":15,"name":"consultas","permission_overwrites":[],
        "available_tags":[
            {"id":"70","name":"Pregunta","emoji_name":"❓","emoji_id":null},
            {"id":"71","name":"Resuelto","moderated":true,"emoji_id":"5","emoji_name":"custom"},
            {"id":"72","name":"\u{7}bad"},
            {"id":"70","name":"repetida"}
        ]}),
    ]);
    guild["threads"] = json!([
        {"id":"40","type":11,"name":"Dudas","parent_id":"20","owner_id":"8",
         "message_count":12,"member_count":3,"last_message_id":"175928847299117063",
         "thread_metadata":{"archived":false,"locked":false},"member":{"id":"40","user_id":"9"}},
        {"id":"41","type":11,"name":"Lente","parent_id":"23","applied_tags":["70","71"],
         "thread_metadata":"broken","message_count":"many"},
        {"id":"42","type":12,"name":"Privado","parent_id":"20"},
        {"id":"43","type":11,"name":"Secreto","parent_id":"21"},
        {"id":"44","type":11,"name":"Sin padre"},
        {"id":"45","type":11,"name":"Huérfano","parent_id":"999"},
        {"id":"46","type":11,"name":"Otro servidor","parent_id":"20","guild_id":"77"}
    ]);
    value
}

fn channel(snapshot: &Snapshot, id: u64) -> Option<&Channel> {
    snapshot.channels.iter().find(|c| c.id == ChannelId(id))
}

#[test]
fn ready_maps_announcements_forums_and_active_threads_with_their_parents_access() {
    let snapshot = ready(&with_threads(), &me()).unwrap();
    assert_eq!(
        channel(&snapshot, 22).unwrap().kind,
        ChannelKind::Announcement
    );
    let forum = channel(&snapshot, 23).unwrap();
    assert_eq!(forum.kind, ChannelKind::Forum);
    // The malformed and repeated tags are left out; a custom emoji is not drawn.
    assert_eq!(forum.tags.len(), 2, "{:?}", forum.tags);
    assert_eq!(forum.tags[0].emoji.as_deref(), Some("❓"));
    assert!(forum.tags[1].moderated && forum.tags[1].emoji.is_none());

    let thread = channel(&snapshot, 40).unwrap();
    let info = thread.thread.as_ref().unwrap();
    assert_eq!(thread.kind, ChannelKind::Thread);
    assert_eq!(thread.parent, Some(ChannelId(20)));
    assert_eq!(
        thread.permissions,
        channel(&snapshot, 20).unwrap().permissions
    );
    assert!(info.joined && !info.private && !info.archived);
    assert_eq!(
        (info.owner, info.messages, info.members),
        (Some(UserId(8)), 12, 3)
    );
    // Discord's documented example snowflake: 2016-04-30 11:18:25.796 UTC.
    assert_eq!(info.last_activity.0, 1_462_015_105_796);

    // Malformed optional parts are dropped, the post is kept.
    let post = channel(&snapshot, 41).unwrap().thread.clone().unwrap();
    assert_eq!(
        post.tags,
        vec![rune_model::TagId(70), rune_model::TagId(71)]
    );
    assert!(!post.archived && post.messages == 0);
    assert!(
        channel(&snapshot, 42)
            .unwrap()
            .thread
            .as_ref()
            .unwrap()
            .private
    );
    // A thread of a hidden channel is hidden too.
    assert_eq!(
        channel(&snapshot, 43).unwrap().permissions,
        Permissions::NONE
    );
    for dropped in [44, 45, 46] {
        assert!(channel(&snapshot, dropped).is_none(), "{dropped}");
    }
    let mut broken = with_threads();
    broken["guilds"][0]["threads"] = json!("not a list");
    assert!(ready(&broken, &me())
        .unwrap()
        .channels
        .iter()
        .all(|c| !c.is_thread()));
}

#[test]
fn thread_updates_keep_membership_and_archive_state_they_do_not_mention() {
    let mut snapshot = ready(&with_threads(), &me()).unwrap();
    let events = update(
        &mut snapshot,
        "THREAD_UPDATE",
        &json!({"id":"40","guild_id":"10","type":11,"name":"Dudas resueltas","parent_id":"20",
                "thread_metadata":{"archived":true,"locked":true}}),
    )
    .unwrap();
    let thread = channel(&snapshot, 40).unwrap();
    let info = thread.thread.as_ref().unwrap();
    assert_eq!(thread.name, "Dudas resueltas");
    assert!(
        info.joined,
        "an update without a member object is not a leave"
    );
    assert!(info.archived && info.locked);
    assert_eq!(info.messages, 12, "an absent count keeps the known one");
    assert!(
        matches!(&events[..], [Event::ChannelChanged(c)] if c.id == ChannelId(40)
        && c.permissions == thread.permissions)
    );
    update(
        &mut snapshot,
        "THREAD_UPDATE",
        &json!({"id":"40","guild_id":"10","type":11,"name":"Dudas","parent_id":"20"}),
    )
    .unwrap();
    let info = channel(&snapshot, 40).unwrap().thread.clone().unwrap();
    assert!(info.archived && info.locked, "no metadata: the state stays");

    update(
        &mut snapshot,
        "THREAD_MEMBERS_UPDATE",
        &json!({"id":"40","guild_id":"10","member_count":2,"removed_member_ids":["9"]}),
    )
    .unwrap();
    let info = channel(&snapshot, 40).unwrap().thread.clone().unwrap();
    assert!(!info.joined && info.members == 2);
    update(
        &mut snapshot,
        "THREAD_MEMBER_UPDATE",
        &json!({"id":"40","guild_id":"10","user_id":"9"}),
    )
    .unwrap();
    assert!(
        channel(&snapshot, 40)
            .unwrap()
            .thread
            .as_ref()
            .unwrap()
            .joined
    );

    // A new thread arrives with its parent's access; deleting it removes only threads.
    let events = update(
        &mut snapshot,
        "THREAD_CREATE",
        &json!({"id":"47","guild_id":"10","type":11,"name":"Nuevo","parent_id":"20"}),
    )
    .unwrap();
    assert!(matches!(&events[..], [Event::ChannelChanged(c)]
        if c.id == ChannelId(47) && c.permissions.contains(Permissions::VIEW)));
    let none = update(
        &mut snapshot,
        "THREAD_DELETE",
        &json!({"id":"20","guild_id":"10"}),
    )
    .unwrap();
    assert!(none.is_empty() && channel(&snapshot, 20).is_some());
    let gone = update(
        &mut snapshot,
        "THREAD_DELETE",
        &json!({"id":"47","guild_id":"10"}),
    )
    .unwrap();
    assert!(matches!(&gone[..], [Event::ChannelRemoved { channel }] if *channel == ChannelId(47)));
    // Hidden from now on: removed.
    update(
        &mut snapshot,
        "THREAD_UPDATE",
        &json!({"id":"40","guild_id":"10","type":11,"parent_id":"20","flags":131072}),
    )
    .unwrap();
    assert!(channel(&snapshot, 40).is_none());
}

#[test]
fn a_threads_silence_comes_with_the_accounts_membership() {
    let silence = |snapshot: &Snapshot, id: u64| {
        channel(snapshot, id)
            .unwrap()
            .thread
            .as_ref()
            .unwrap()
            .muted_until
    };
    let mut value = with_threads();
    value["guilds"][0]["threads"][0]["member"]["muted"] = json!(true);
    value["guilds"][0]["threads"][0]["member"]["mute_config"] = json!({"end_time":null});
    let mut snapshot = ready(&value, &me()).unwrap();
    assert_eq!(silence(&snapshot, 40), Some(rune_model::MUTED_FOREVER));
    // An update without the member object says nothing of it.
    update(
        &mut snapshot,
        "THREAD_UPDATE",
        &json!({"id":"40","guild_id":"10","type":11,"name":"Dudas","parent_id":"20"}),
    )
    .unwrap();
    assert_eq!(silence(&snapshot, 40), Some(rune_model::MUTED_FOREVER));
    // The account's own member update: until then; someone else's: nothing.
    let until = "2026-10-18T20:00:00.000Z";
    let events = update(
        &mut snapshot,
        "THREAD_MEMBER_UPDATE",
        &json!({"id":"40","guild_id":"10","user_id":"9","muted":true,
            "mute_config":{"end_time":until,"selected_time_window":3600}}),
    )
    .unwrap();
    let at = wire::timestamp(&json!(until)).unwrap();
    assert_eq!(silence(&snapshot, 40), Some(at));
    assert!(matches!(&events[..], [Event::ChannelChanged(c)]
        if c.thread.as_ref().unwrap().muted_until == Some(at)));
    update(
        &mut snapshot,
        "THREAD_MEMBER_UPDATE",
        &json!({"id":"40","guild_id":"10","user_id":"8","muted":false}),
    )
    .unwrap();
    assert_eq!(silence(&snapshot, 40), Some(at));
    // Leaving ends it.
    update(
        &mut snapshot,
        "THREAD_MEMBERS_UPDATE",
        &json!({"id":"40","guild_id":"10","removed_member_ids":["9"]}),
    )
    .unwrap();
    assert_eq!(silence(&snapshot, 40), None);
    // A sync's members carry it too.
    update(
        &mut snapshot,
        "THREAD_LIST_SYNC",
        &json!({"guild_id":"10","channel_ids":["20"],
                "threads":[{"id":"48","type":11,"name":"Activo","parent_id":"20"}],
                "members":[{"id":"48","user_id":"9","muted":true}]}),
    )
    .unwrap();
    assert_eq!(silence(&snapshot, 48), Some(rune_model::MUTED_FOREVER));
    // Confirmed by the route's success, as asked.
    let events = thread_confirmed(
        &mut snapshot,
        &Command::MuteThread {
            thread: ChannelId(48),
            until: None,
        },
        Vec::new(),
    );
    assert!(matches!(&events[..], [Event::ChannelChanged(c)]
        if c.id == ChannelId(48) && c.thread.as_ref().unwrap().muted_until.is_none()));
    // Only a member of the thread silences it.
    let mut stranger = snapshot.clone();
    if let Some(info) = stranger
        .channels
        .iter_mut()
        .find(|c| c.id == ChannelId(48))
        .and_then(|c| c.thread.as_mut())
    {
        info.joined = false;
    }
    let mute = Command::MuteThread {
        thread: ChannelId(48),
        until: Some(rune_model::MUTED_FOREVER),
    };
    assert!(thread_allowed(&snapshot, &mute).is_ok());
    assert!(thread_allowed(&stranger, &mute).is_err());
    // Leaving it, once confirmed, ends its silence before the Gateway says so.
    thread_confirmed(&mut snapshot, &mute, Vec::new());
    assert_eq!(silence(&snapshot, 48), Some(rune_model::MUTED_FOREVER));
    let events = thread_confirmed(
        &mut snapshot,
        &Command::FollowThread {
            thread: ChannelId(48),
            follow: false,
        },
        Vec::new(),
    );
    assert!(matches!(&events[..], [Event::ChannelChanged(c)]
        if !c.thread.as_ref().unwrap().joined && c.thread.as_ref().unwrap().muted_until.is_none()));
}

#[test]
fn threads_read_by_page_stay_through_a_new_ready_under_a_parent_still_shown() {
    let before = ready(&with_threads(), &me()).unwrap();
    let paged = |id: u64, parent: u64, archived: bool, joined: bool| {
        let mut thread = channel(&before, 40).unwrap().clone();
        thread.id = ChannelId(id);
        thread.parent = Some(ChannelId(parent));
        thread.permissions = rune_model::Permissions::NONE;
        let info = thread.thread.as_mut().unwrap();
        (info.archived, info.joined) = (archived, joined);
        thread
    };
    let mut read = before.channels.clone();
    read.extend([
        paged(60, 20, true, true),
        paged(61, 20, false, false),
        paged(62, 20, false, true),
        paged(63, 21, true, false),
    ]);
    let mut after = ready(&with_threads(), &me()).unwrap();
    keep_paged_threads(&mut after, read);
    // Archived, or open and not followed: kept, with what the parent allows.
    let parent = channel(&after, 20).unwrap().permissions;
    for id in [60, 61] {
        assert_eq!(channel(&after, id).map(|c| c.permissions), Some(parent));
    }
    // An open one it followed would have been named; a hidden parent's goes.
    assert!(channel(&after, 62).is_none() && channel(&after, 63).is_none());
    // What the READY names is its own, once.
    assert_eq!(
        after
            .channels
            .iter()
            .filter(|c| c.id == ChannelId(40))
            .count(),
        1
    );
}

#[test]
fn thread_list_sync_replaces_only_its_scope_and_marks_membership() {
    let mut snapshot = ready(&with_threads(), &me()).unwrap();
    let events = update(
        &mut snapshot,
        "THREAD_LIST_SYNC",
        &json!({"guild_id":"10","channel_ids":["20"],
                "threads":[{"id":"48","type":11,"name":"Activo","parent_id":"20"},
                           {"id":"49","type":11,"name":"Fuera","parent_id":"23"}],
                "members":[{"id":"48","user_id":"9"}]}),
    )
    .unwrap();
    // 40 and 42 were under #general and are not listed: gone. 41 is a post of another parent.
    for gone in [40, 42] {
        assert!(channel(&snapshot, gone).is_none(), "{gone}");
        assert!(events.contains(&Event::ChannelRemoved {
            channel: ChannelId(gone)
        }));
    }
    assert!(channel(&snapshot, 41).is_some() && channel(&snapshot, 49).is_none());
    assert!(
        channel(&snapshot, 48)
            .unwrap()
            .thread
            .as_ref()
            .unwrap()
            .joined
    );
    // Without channel_ids the whole guild is replaced.
    update(
        &mut snapshot,
        "THREAD_LIST_SYNC",
        &json!({"guild_id":"10","threads":[]}),
    )
    .unwrap();
    assert!(snapshot.channels.iter().all(|c| !c.is_thread()));
    let ignored = update(
        &mut snapshot,
        "THREAD_LIST_SYNC",
        &json!({"guild_id":"10","threads":"x"}),
    );
    assert!(
        ignored.unwrap().is_empty(),
        "a malformed sync changes nothing and ends nothing"
    );
}

#[test]
fn forums_say_how_they_list_posts_and_whether_a_post_needs_a_tag() {
    let mut value = with_threads();
    let forum = &mut value["guilds"][0]["channels"][3];
    forum["default_forum_layout"] = json!(2);
    forum["default_sort_order"] = json!(1);
    forum["flags"] = json!(16);
    let mut snapshot = ready(&value, &me()).unwrap();
    let defaults = channel(&snapshot, 23).unwrap().forum.clone();
    assert!(defaults.gallery && defaults.by_creation && defaults.require_tag);
    // An update that leaves them out keeps them; one that sets them changes them.
    update(
        &mut snapshot,
        "CHANNEL_UPDATE",
        &json!({"id":"23","guild_id":"10","type":15,"name":"consultas"}),
    )
    .unwrap();
    assert!(channel(&snapshot, 23).unwrap().forum.gallery);
    update(
        &mut snapshot,
        "CHANNEL_UPDATE",
        &json!({"id":"23","guild_id":"10","type":15,"default_forum_layout":1,"flags":0}),
    )
    .unwrap();
    let defaults = channel(&snapshot, 23).unwrap().forum.clone();
    assert!(!defaults.gallery && defaults.by_creation && !defaults.require_tag);
    assert_eq!(
        channel(&snapshot, 20).unwrap().forum,
        rune_model::ForumDefaults::default()
    );
}

#[test]
fn thread_list_sync_keeps_archived_threads_read_a_page_at_a_time() {
    let mut snapshot = ready(&with_threads(), &me()).unwrap();
    let archived = thread_answer(&json!({"id":"60","guild_id":"10","type":11,"name":"Viejo",
        "parent_id":"20","thread_metadata":{"archived":true}}))
    .unwrap();
    let page = Command::LoadThreads {
        request: 1,
        parent: ChannelId(20),
        scope: rune_provider::ThreadScope::Archived,
        before: None,
    };
    thread_confirmed(&mut snapshot, &page, vec![Event::ChannelChanged(archived)]);
    update(
        &mut snapshot,
        "THREAD_LIST_SYNC",
        &json!({"guild_id":"10","channel_ids":["20"],"threads":[]}),
    )
    .unwrap();
    assert!(
        channel(&snapshot, 60).is_some(),
        "an archived thread is not an active one gone"
    );
    assert!(channel(&snapshot, 40).is_none());
}

#[test]
fn thread_writes_are_checked_against_the_session_and_confirmed_beside_their_parent() {
    use rune_model::{MessageId, TagId};
    use rune_provider::{Created, ThreadEdit, ThreadScope};
    let mut snapshot = ready(&with_threads(), &me()).unwrap();
    let grant = |snapshot: &mut Snapshot, id: u64, extra: Permissions| {
        let found = snapshot
            .channels
            .iter_mut()
            .find(|c| c.id == ChannelId(id))
            .unwrap();
        found.permissions = found.permissions | extra;
    };
    let create = |private: bool, message: Option<u64>, name: &str| Command::CreateThread {
        request: 3,
        channel: ChannelId(20),
        message: message.map(MessageId),
        name: name.into(),
        private,
    };
    // Without the bits to start threads, nothing goes.
    assert!(thread_allowed(&snapshot, &create(false, Some(100), "Hilo")).is_err());
    grant(&mut snapshot, 20, Permissions::CREATE_THREADS);
    assert!(thread_allowed(&snapshot, &create(false, Some(100), "Hilo")).is_ok());
    assert!(thread_allowed(&snapshot, &create(true, None, "Hilo")).is_err());
    assert!(thread_allowed(&snapshot, &create(true, Some(100), "Hilo")).is_err());
    assert!(thread_allowed(&snapshot, &create(false, None, " ")).is_err());
    assert!(thread_allowed(&snapshot, &create(false, None, &"x".repeat(101))).is_err());
    grant(&mut snapshot, 20, Permissions::CREATE_PRIVATE_THREADS);
    assert!(thread_allowed(&snapshot, &create(true, None, "Hilo")).is_ok());

    // A post carries the forum's tags; a moderated one needs moderation.
    let post = |tags: Vec<u64>| Command::CreatePost {
        request: 4,
        forum: ChannelId(23),
        title: "¿Qué lente?".into(),
        tags: tags.into_iter().map(TagId).collect(),
        content: "Para retratos.".into(),
        files: Vec::new(),
    };
    assert!(thread_allowed(&snapshot, &post(vec![70])).is_ok());
    assert!(thread_allowed(&snapshot, &post(vec![71])).is_err());
    assert!(thread_allowed(&snapshot, &post(vec![70, 70])).is_err());
    assert!(thread_allowed(&snapshot, &post(vec![999])).is_err());
    grant(&mut snapshot, 23, Permissions::MANAGE_THREADS);
    assert!(thread_allowed(&snapshot, &post(vec![71])).is_ok());

    // Pages: private ones only for moderation, a cursor only of its scope.
    let page = |scope, before: Option<&str>| Command::LoadThreads {
        request: 5,
        parent: ChannelId(20),
        scope,
        before: before.map(str::to_owned),
    };
    assert!(thread_allowed(&snapshot, &page(ThreadScope::Archived, None)).is_ok());
    assert!(thread_allowed(&snapshot, &page(ThreadScope::Private, None)).is_err());
    assert!(thread_allowed(&snapshot, &page(ThreadScope::Joined, Some("41"))).is_ok());
    assert!(thread_allowed(&snapshot, &page(ThreadScope::Joined, Some("ayer"))).is_err());
    assert!(thread_allowed(
        &snapshot,
        &page(
            ThreadScope::Archived,
            Some("2026-09-10T12:00:00.000000+00:00")
        )
    )
    .is_ok());

    // Someone else's thread: neither closed nor deleted without moderation.
    let close = Command::EditThread {
        thread: ChannelId(40),
        edit: ThreadEdit {
            archived: Some(true),
            ..Default::default()
        },
    };
    assert!(thread_allowed(&snapshot, &close).is_err());
    assert!(thread_allowed(
        &snapshot,
        &Command::DeleteThread {
            thread: ChannelId(40)
        }
    )
    .is_err());
    grant(&mut snapshot, 40, Permissions::MANAGE_THREADS);
    assert!(thread_allowed(&snapshot, &close).is_ok());

    // A created thread is reported with its parent's access, and joined.
    let answer = thread_answer(&json!({"id":"61","guild_id":"10","type":11,"name":"Hilo",
        "parent_id":"20","owner_id":"9","thread_metadata":{"archived":false}}))
    .unwrap();
    let made = vec![
        Event::ChannelChanged(answer),
        Event::Created {
            request: 3,
            created: Created::Channel(ChannelId(61)),
        },
    ];
    let events = thread_confirmed(&mut snapshot, &create(false, None, "Hilo"), made);
    let [Event::ChannelChanged(thread), Event::Created { .. }] = events.as_slice() else {
        panic!("{events:?}");
    };
    assert!(thread.thread.as_ref().unwrap().joined);
    assert_eq!(
        thread.permissions,
        channel(&snapshot, 20).unwrap().permissions
    );
    // Under a parent this session does not know: left out, creation and all.
    let stray = thread_answer(&json!({"id":"62","guild_id":"10","type":11,"name":"x",
        "parent_id":"999"}))
    .unwrap();
    let events = thread_confirmed(
        &mut snapshot,
        &create(false, None, "x"),
        vec![
            Event::ChannelChanged(stray),
            Event::Created {
                request: 3,
                created: Created::Channel(ChannelId(62)),
            },
        ],
    );
    assert!(events.is_empty(), "{events:?}");

    // A page never replaces what the Gateway reported of a known thread.
    let stale = thread_answer(&json!({"id":"40","guild_id":"10","type":11,"name":"Viejo",
        "parent_id":"20","thread_metadata":{"archived":true}}))
    .unwrap();
    let pages = page(ThreadScope::Archived, None);
    let events = thread_confirmed(&mut snapshot, &pages, vec![Event::ChannelChanged(stale)]);
    assert!(events.is_empty());
    assert_eq!(channel(&snapshot, 40).unwrap().name, "Dudas");

    // Leaving a private thread hides it; following another marks it.
    let leave = Command::FollowThread {
        thread: ChannelId(42),
        follow: false,
    };
    for channel in snapshot
        .channels
        .iter_mut()
        .filter(|c| c.id == ChannelId(42))
    {
        channel.permissions = channel.permissions.without(Permissions::MANAGE_THREADS);
        channel.thread.as_mut().unwrap().joined = true;
    }
    assert_eq!(
        thread_confirmed(&mut snapshot, &leave, Vec::new()),
        vec![Event::ChannelRemoved {
            channel: ChannelId(42)
        }]
    );
    assert!(channel(&snapshot, 42).is_none());
    let follow = Command::FollowThread {
        thread: ChannelId(61),
        follow: true,
    };
    let events = thread_confirmed(&mut snapshot, &follow, Vec::new());
    assert!(matches!(&events[..], [Event::ChannelChanged(c)] if c.thread.as_ref().unwrap().joined));
}

#[test]
fn a_picture_set_for_the_server_reaches_the_member_and_a_bad_hash_none() {
    let hash = "a_0123456789abcdef0123456789abcdef";
    let mut data = account();
    data["merged_members"][0][0]["avatar"] = json!(hash);
    let snapshot = ready(&data, &me()).unwrap();
    assert_eq!(
        snapshot.members[0].avatar.as_ref().map(|a| a.url.as_str()),
        Some(
            format!("https://cdn.discordapp.com/guilds/10/users/9/avatars/{hash}.png?size=128")
                .as_str()
        )
    );
    data["merged_members"][0][0]["avatar"] = json!("../../evil");
    assert_eq!(ready(&data, &me()).unwrap().members[0].avatar, None);
}
