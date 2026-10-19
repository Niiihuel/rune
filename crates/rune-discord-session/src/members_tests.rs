use super::*;
use serde_json::json;

const ME: UserId = UserId(42);

fn snapshot(overwrites: Value) -> Snapshot {
    let mut snapshot = Snapshot {
        me: Some(wire::user(&json!({"id": "42", "username": "Owner"})).unwrap()),
        ..Default::default()
    };
    // View is Discord's bit 10.
    bootstrap::update(&mut snapshot, "GUILD_CREATE", &json!({"id":"10","name":"Fixture","owner_id":"43",
        "roles":[{"id":"10","name":"@everyone","permissions":(1u64 << 10).to_string()}],
        "members":[{"user":{"id":"42","username":"Owner"},"roles":[]}],
        "channels":[{"id":"30","guild_id":"10","type":0,"name":"general","permission_overwrites":overwrites}]
    })).unwrap();
    snapshot
}

fn watch(lists: &mut Lists, first: u32, last: u32) -> Option<Watch> {
    lists.watch(
        &snapshot(json!([])),
        &Command::WatchMemberList {
            server: ServerId(10),
            channel: ChannelId(30),
            first,
            last,
        },
    )
}

fn person(id: u64, status: &str) -> Value {
    json!({"member": {"user": {"id": id.to_string(), "username": format!("p{id}")},
        "roles": [], "nick": null, "joined_at": "2024-03-12T10:00:00Z",
        "presence": {"status": status, "activities": [{"type": 4, "state": "Programando"}]}}})
}

fn heading(id: &str, count: u32) -> Value {
    json!({"group": {"id": id, "count": count}})
}

fn update(ops: Value, groups: Value) -> Value {
    json!({"guild_id": "10", "id": "everyone", "ops": ops, "groups": groups})
}

fn list(events: &[Event]) -> &MemberList {
    match events.last() {
        Some(Event::MemberListChanged(list)) => list,
        other => panic!("list last, got {other:?}"),
    }
}

fn people(list: &MemberList) -> Vec<(u32, u64)> {
    list.rows
        .iter()
        .filter_map(|(at, row)| match row {
            ListRow::Member(user) => Some((*at, user.0)),
            ListRow::Heading { .. } => None,
        })
        .collect()
}

#[test]
fn murmur3_matches_published_vectors() {
    assert_eq!(murmur3(b""), 0);
    assert_eq!(murmur3(b"hello"), 0x248b_fa47);
    assert_eq!(
        murmur3(b"The quick brown fox jumps over the lazy dog"),
        0x2e4f_f723
    );
}

#[test]
fn a_channel_everyone_sees_is_the_everyone_list_and_others_hash_their_overwrites() {
    assert_eq!(
        list_id(&snapshot(json!([])), ChannelId(30)).as_deref(),
        Some("everyone")
    );
    let hidden = snapshot(json!([
        {"id": "10", "type": 0, "allow": "0", "deny": (1u64 << 10).to_string()},
        {"id": "7", "type": 1, "allow": (1u64 << 10).to_string(), "deny": "0"},
    ]));
    let expected = murmur3(b"allow:7,deny:10").to_string();
    assert_eq!(list_id(&hidden, ChannelId(30)), Some(expected));
}

#[test]
fn ranges_are_one_page_or_two_adjacent_pages_ending_at_the_last_row() {
    assert_eq!(ranges(0, 20), vec![[0, 99]]);
    assert_eq!(ranges(90, 120), vec![[0, 99], [100, 199]]);
    assert_eq!(ranges(50, 420), vec![[300, 399], [400, 499]]);
    assert_eq!(ranges(120, 90), vec![[0, 99], [100, 199]], "either order");
    assert_eq!(ranges(u32::MAX, u32::MAX).len(), 1, "bounded");
}

#[test]
fn frames_open_a_list_change_only_ranges_and_leave_before_another() {
    let mut lists = Lists::default();
    let first = watch(&mut lists, 0, 20).unwrap();
    let opened = frames(None, Some(&first));
    assert_eq!(opened.len(), 2);
    assert_eq!(opened[0]["subscriptions"]["10"]["channels"], json!({}));
    assert_eq!(opened[0]["subscriptions"]["10"]["typing"], true);
    assert_eq!(
        opened[1]["subscriptions"]["10"]["channels"],
        json!({"30": [[0, 99]]})
    );
    assert_eq!(opened[1]["subscriptions"]["10"]["activities"], true);
    assert_eq!(opened[1]["subscriptions"]["10"]["member_updates"], false);
    assert!(frames(Some(&first), Some(&first)).is_empty());
    let scrolled = watch(&mut lists, 150, 180).unwrap();
    let moved = frames(Some(&first), Some(&scrolled));
    assert_eq!(moved.len(), 1, "same list: only the ranges");
    assert_eq!(
        moved[0]["subscriptions"]["10"]["channels"]["30"],
        json!([[100, 199]])
    );
    let other = Watch {
        server: ServerId(11),
        channel: ChannelId(31),
        list: "everyone".into(),
        ranges: vec![[0, 99]],
        people: Vec::new(),
    };
    let switched = frames(Some(&scrolled), Some(&other));
    assert_eq!(switched.len(), 3);
    assert_eq!(switched[0]["subscriptions"]["10"]["typing"], false);
    assert_eq!(switched[0]["subscriptions"]["10"]["channels"], json!({}));
    assert!(switched[1]["subscriptions"]["11"].is_object());
    let left = frames(Some(&other), None);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0]["subscriptions"]["11"]["typing"], false);
}

#[test]
fn an_unreadable_or_foreign_channel_watches_nothing() {
    let mut lists = Lists::default();
    let mut hidden = snapshot(json!([]));
    hidden.channels[0].permissions = Permissions::NONE;
    let command = Command::WatchMemberList {
        server: ServerId(10),
        channel: ChannelId(30),
        first: 0,
        last: 10,
    };
    assert_eq!(lists.watch(&hidden, &command), None);
    let elsewhere = Command::WatchMemberList {
        server: ServerId(11),
        channel: ChannelId(30),
        first: 0,
        last: 10,
    };
    assert_eq!(lists.watch(&snapshot(json!([])), &elsewhere), None);
}

#[test]
fn sync_then_insert_update_and_delete_keep_places_without_duplicates() {
    let mut lists = Lists::default();
    watch(&mut lists, 0, 20);
    let groups = json!([{"id": "5", "count": 1}, {"id": "online", "count": 2}, {"id": "offline", "count": 1}]);
    let events = lists.update(
        Some(ME),
        &update(
            json!([{"op": "SYNC", "range": [0, 99], "items": [
                heading("5", 1), person(1, "online"), heading("online", 2), person(2, "idle"),
                person(42, "online"), heading("offline", 1), person(3, "offline")]}]),
            groups,
        ),
    );
    let synced = list(&events);
    assert_eq!(synced.len(), 7);
    assert_eq!(people(synced), vec![(1, 1), (3, 2), (4, 42), (6, 3)]);
    assert_eq!(
        synced.row(0),
        Some(ListRow::Heading {
            group: ListGroup::Role(RoleId(5)),
            count: 1
        })
    );
    assert!(synced.valid());
    // People arrive before the list; the account's own membership does not.
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::PresenceChanged { user, presence }
        if *user == UserId(2) && presence.status == Status::Idle
            && presence.custom.as_deref() == Some("Programando"))));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::MemberChanged(m) if m.user == UserId(3))));
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::MemberChanged(m) if m.user == ME)));
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::PresenceChanged { user, .. } if *user == ME)));

    // Person 3 comes online: removed from offline, inserted among online.
    let groups = json!([{"id": "5", "count": 1}, {"id": "online", "count": 3}]);
    let events = lists.update(
        Some(ME),
        &update(
            json!([
                {"op": "DELETE", "index": 6},
                {"op": "DELETE", "index": 5},
                {"op": "INSERT", "index": 3, "item": person(3, "online")},
                {"op": "UPDATE", "index": 2, "item": heading("online", 3)},
            ]),
            groups,
        ),
    );
    let moved = list(&events);
    assert_eq!(moved.len(), 6);
    assert_eq!(people(moved), vec![(1, 1), (3, 3), (4, 2), (5, 42)]);
    assert!(moved.valid());

    // A stray insert of someone already listed shows them once.
    let events = lists.update(
        Some(ME),
        &update(
            json!([{"op": "INSERT", "index": 1, "item": person(2, "online")}]),
            json!([{"id": "5", "count": 2}, {"id": "online", "count": 3}]),
        ),
    );
    let stray = list(&events);
    assert_eq!(people(stray), vec![(1, 2), (2, 1), (4, 3), (6, 42)]);
    assert!(stray.valid());
}

#[test]
fn invalidate_forgets_rows_and_a_malformed_update_changes_nothing() {
    let mut lists = Lists::default();
    watch(&mut lists, 0, 20);
    let groups = json!([{"id": "online", "count": 2}]);
    lists.update(
        Some(ME),
        &update(
            json!([{"op": "SYNC", "range": [0, 99], "items": [
                heading("online", 2), person(1, "online"), person(2, "online")]}]),
            groups.clone(),
        ),
    );
    for malformed in [
        update(json!([{"op": "DELETE"}]), groups.clone()),
        update(json!([{"op": "SYNC", "range": [5, 1]}]), groups.clone()),
        update(json!("ops"), groups.clone()),
        update(json!([]), json!([{"id": "", "count": 1}])),
        update(json!([]), json!([{"id": "online"}, {"id": "online"}])),
        update(
            json!(vec![json!({"op": "DELETE", "index": 0}); MAX_OPS + 1]),
            groups.clone(),
        ),
        update(
            json!([{"op": "SYNC", "range": [0, 1], "items": [person(1, "online"), person(2, "online"), person(3, "online")]}]),
            groups.clone(),
        ),
        json!({"guild_id": "11", "id": "everyone", "ops": [], "groups": groups}),
        json!({"guild_id": "10", "id": "someone-else", "ops": [], "groups": groups}),
    ] {
        assert!(lists.update(Some(ME), &malformed).is_empty(), "{malformed}");
    }
    assert_eq!(lists.rows.len(), 3, "the last good list stays");
    // A malformed item is a hole: the next row keeps its place.
    let events = lists.update(
        Some(ME),
        &update(
            json!([{"op": "UPDATE", "index": 1, "item": {"member": {"user": {"id": "bad"}}}}]),
            groups.clone(),
        ),
    );
    assert_eq!(people(list(&events)), vec![(2, 2)]);
    let events = lists.update(
        Some(ME),
        &update(json!([{"op": "INVALIDATE", "range": [0, 99]}]), groups),
    );
    assert!(list(&events).rows.is_empty());
}

#[test]
fn rows_outside_the_watched_pages_are_dropped_and_switching_retires_the_old_list() {
    let mut lists = Lists::default();
    watch(&mut lists, 0, 20);
    let groups = json!([{"id": "online", "count": 250}]);
    let items: Vec<Value> = std::iter::once(heading("online", 250))
        .chain((1..100).map(|id| person(id, "online")))
        .collect();
    lists.update(
        Some(ME),
        &update(
            json!([{"op": "SYNC", "range": [0, 99], "items": items}]),
            groups.clone(),
        ),
    );
    assert_eq!(lists.rows.len(), 100);
    watch(&mut lists, 180, 220);
    assert!(lists.rows.is_empty(), "rows 0-99 left the watched pages");
    let events = lists.update(
        Some(ME),
        &update(
            json!([{"op": "SYNC", "range": [100, 199], "items": [person(500, "online")]},
                   {"op": "SYNC", "range": [0, 99], "items": [person(501, "online")]}]),
            groups,
        ),
    );
    assert_eq!(people(list(&events)), vec![(100, 500)]);
    // Unwatching retires the list: a late update of it is not taken.
    assert_eq!(
        lists.watch(&Snapshot::default(), &Command::UnwatchMemberList),
        None
    );
    assert!(lists
        .update(Some(ME), &update(json!([]), json!([])))
        .is_empty());
    watch(&mut lists, 0, 10);
    assert_eq!(lists.list.as_deref(), Some("everyone"));
    assert!(lists.retired.iter().any(|old| old == "everyone"));
}

#[test]
fn a_populated_first_sync_of_another_list_is_taken_once_but_never_a_retired_one() {
    let mut lists = Lists::default();
    watch(&mut lists, 0, 10);
    lists.retired.push_back("old".into());
    let named = |id: &str, ops: Value| json!({"guild_id": "10", "id": id, "ops": ops, "groups": [{"id": "online", "count": 1}]});
    let sync = json!([{"op": "SYNC", "range": [0, 99], "items": [heading("online", 1), person(1, "online")]}]);
    assert!(lists
        .update(Some(ME), &named("old", sync.clone()))
        .is_empty());
    assert!(
        lists.update(Some(ME), &named("7", json!([]))).is_empty(),
        "empty is not taken"
    );
    assert_eq!(
        people(list(&lists.update(Some(ME), &named("7", sync.clone())))),
        vec![(1, 1)]
    );
    assert!(
        lists.update(Some(ME), &named("8", sync)).is_empty(),
        "only before a sync"
    );
}

#[test]
fn presence_reaches_only_people_in_the_list_and_bounds_activities() {
    let mut lists = Lists::default();
    watch(&mut lists, 0, 10);
    lists.update(
        Some(ME),
        &update(
            json!([{"op": "SYNC", "range": [0, 99], "items": [heading("online", 1), person(1, "online")]}]),
            json!([{"id": "online", "count": 1}]),
        ),
    );
    let change = |user: u64| {
        json!({"guild_id": "10", "user": {"id": user.to_string()}, "status": "dnd",
            "activities": [{"type": 0, "name": "Factorio"}]})
    };
    let Some(Event::PresenceChanged { presence, .. }) = lists.presence(Some(ME), &change(1)) else {
        panic!("presence of someone listed");
    };
    assert_eq!(presence.status, Status::DoNotDisturb);
    assert_eq!(
        presence.activity().map(|a| a.name.as_str()),
        Some("Factorio")
    );
    assert_eq!(lists.presence(Some(ME), &change(2)), None, "not listed");
    assert_eq!(
        presence_of(
            json!({"status": "online", "activities": vec![json!({"type": 0, "name": "x"}); 17]})
        ),
        None
    );
    assert_eq!(presence_of(json!({"status": "away"})), None);
    let offline =
        presence_of(json!({"status": "offline", "activities": [{"type": 4, "state": "x"}]}));
    assert_eq!(
        offline,
        Some(Presence::default()),
        "offline says nothing else"
    );
}

#[test]
fn the_people_whose_cards_show_are_subscribed_with_the_list_and_their_presence_reaches() {
    let mut lists = Lists::default();
    let first = watch(&mut lists, 0, 20).unwrap();
    assert!(first.people.is_empty());
    let with = lists.people(ServerId(10), &[UserId(7)]).unwrap();
    assert_eq!(with.people, [UserId(7)]);
    let sent = frames(Some(&first), Some(&with));
    assert_eq!(sent.len(), 1, "the same list: only its frame again");
    assert_eq!(sent[0]["subscriptions"]["10"]["members"], json!(["7"]));
    // Someone not in the rows, whose card shows: their presence comes in.
    let change = json!({"guild_id": "10", "user": {"id": "7"}, "status": "dnd",
        "client_status": {"mobile": "dnd"}});
    assert!(matches!(
        lists.presence(Some(ME), &change),
        Some(Event::PresenceChanged {
            user: UserId(7),
            ..
        })
    ));
    // Scrolling keeps them; another server's people are not this list's.
    assert_eq!(watch(&mut lists, 150, 180).unwrap().people, [UserId(7)]);
    assert!(lists
        .people(ServerId(11), &[UserId(8)])
        .unwrap()
        .people
        .is_empty());
    let none = lists.people(ServerId(10), &[]).unwrap();
    assert!(none.people.is_empty());
    assert_eq!(lists.presence(Some(ME), &change), None, "no longer shown");
}

#[test]
fn a_voice_state_names_its_member_before_the_call_reads_it() {
    let state = |user: &str| {
        json!({"guild_id": "10", "channel_id": "40", "user_id": user,
            "member": {"user": {"id": user, "username": "ada", "avatar": "0123456789abcdef0123456789abcdef"},
                "nick": "Ada de prueba", "roles": [], "joined_at": "2024-03-12T10:00:00Z"}})
    };
    let events = in_voice(Some(ME), &state("7"));
    assert!(
        matches!(&events[..], [Event::UserChanged(user), Event::MemberChanged(member)]
            if user.id == UserId(7) && user.avatar.is_some()
            && member.nick.as_deref() == Some("Ada de prueba")),
        "{events:?}"
    );
    assert!(
        in_voice(Some(UserId(7)), &state("7")).is_empty(),
        "the account's own"
    );
    let mut other = state("7");
    other["user_id"] = json!("8");
    assert!(
        in_voice(Some(ME), &other).is_empty(),
        "a member of someone else"
    );
    assert!(in_voice(Some(ME), &json!({"guild_id": "10", "user_id": "7"})).is_empty());
}

#[test]
fn a_custom_status_keeps_its_emoji_before_its_text() {
    let custom = |activity: Value| {
        presence_of(json!({"status": "online", "activities": [activity]}))
            .and_then(|presence| presence.custom)
    };
    assert_eq!(
        custom(json!({"type": 4, "state": "jugando", "emoji": {"name": "💚"}})).as_deref(),
        Some("💚 jugando")
    );
    assert_eq!(
        custom(json!({"type": 4, "emoji": {"name": "🌙", "id": null}})).as_deref(),
        Some("🌙"),
        "the emoji alone"
    );
    let server = presence_of(json!({"status": "online", "activities": [
        {"type": 4, "state": "hola", "emoji": {"name": "rana", "id": "15", "animated": true}}
    ]}))
    .unwrap();
    assert_eq!(
        server.custom.as_deref(),
        Some("hola"),
        "a server's emoji goes apart"
    );
    let picture = server.custom_emoji.unwrap();
    assert_eq!(
        (picture.name.as_str(), picture.token.as_str()),
        ("rana", "<a:rana:15>")
    );
    assert_eq!(
        picture.image.url,
        "https://cdn.discordapp.com/emojis/15.webp?size=48&animated=true"
    );
    // Alone, the picture is the whole status.
    let alone = presence_of(json!({"status": "online", "activities": [
        {"type": 4, "emoji": {"name": "rust", "id": "16"}}
    ]}))
    .unwrap();
    assert_eq!(alone.custom, None);
    assert!(alone
        .custom_emoji
        .is_some_and(|picture| picture.image.url.ends_with("16.webp?size=48")));
    // A name that is not one stays out.
    assert!(presence_of(json!({"status": "online", "activities": [
        {"type": 4, "state": "x", "emoji": {"name": "bad name", "id": "17"}}
    ]}))
    .unwrap()
    .custom_emoji
    .is_none());
    assert_eq!(custom(json!({"type": 4, "emoji": {"name": "x"}})), None);
}

fn presence_of(value: Value) -> Option<Presence> {
    presence(&value)
}
