//! The full profile over REST and the member list over the Gateway, against
//! local fixtures only. Nothing here reached Discord.

use super::message_recovery::guild_channel;
use super::*;
use rune_model::{ListRow, ServerId, UserId};
use rune_provider::Failure;

const VIEW: u64 = 1 << 10;

fn load(request: u64, user: u64) -> Command {
    Command::LoadProfile {
        request,
        user: UserId(user),
        server: Some(ServerId(10)),
        effect: true,
    }
}

impl Harness {
    pub(super) async fn frame(&mut self) -> Value {
        tokio::time::timeout(Duration::from_secs(1), self.frames.recv())
            .await
            .expect("expected a Gateway frame")
            .unwrap()
    }

    async fn list(&mut self) -> rune_model::MemberList {
        loop {
            if let Event::MemberListChanged(list) = self.running.next().await {
                return list;
            }
        }
    }
}

#[tokio::test]
async fn a_full_profile_is_one_get_and_a_newer_card_replaces_the_one_in_flight() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, VIEW).await;
    app.running.commands.send(load(1, 43)).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with(
        "GET /api/v10/users/43/profile?with_mutual_guilds=true&with_mutual_friends=true&with_mutual_friends_count=false&guild_id=10 "
    ));
    reply
        .send(response(
            200,
            &json!({"user":{"id":"43","username":"peer"},
                "user_profile":{"bio":"Hola","pronouns":"elle"},
                "guild_member":{"user":{"id":"43"},"nick":"Apodo","roles":[]},
                "mutual_guilds":[{"id":"10"}],
                "mutual_friends":[{"id":"44","username":"friend"}]})
            .to_string(),
        ))
        .unwrap();
    assert!(matches!(app.running.next().await, Event::UserChanged(user) if user.id == UserId(44)));
    let Event::ProfileLoaded {
        request: 1,
        profile,
    } = app.running.next().await
    else {
        panic!("profile");
    };
    assert_eq!(profile.about.as_deref(), Some("Hola"));
    assert_eq!(profile.nick.as_deref(), Some("Apodo"));
    assert_eq!(profile.mutual_servers, Some(vec![ServerId(10)]));
    assert_eq!(profile.mutual_friends, Some(vec![UserId(44)]));

    // A second card while the first read is in flight: only the newer answers.
    app.running.commands.send(load(2, 45)).await.unwrap();
    let (_, stale) = app.pending().await;
    app.running.commands.send(load(3, 46)).await.unwrap();
    let _ = stale.send(response(
        200,
        &json!({"user":{"id":"45","username":"x"}}).to_string(),
    ));
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/users/46/profile?"));
    reply
        .send(response(
            200,
            &json!({"user":{"id":"46","username":"y"}}).to_string(),
        ))
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::ProfileLoaded { request: 3, .. }
    ));

    // A refused read fails that read alone; the session goes on.
    app.running.commands.send(load(4, 47)).await.unwrap();
    let (_, reply) = app.pending().await;
    reply.send(response(404, "{}")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::ProfileLoad { request: 4, .. },
            ambiguous: false,
            ..
        }
    ));
    // The account's own profile asks for no mutuals; a server left is refused.
    app.running
        .commands
        .send(Command::LoadProfile {
            request: 5,
            user: UserId(42),
            server: None,
            effect: true,
        })
        .await
        .unwrap();
    let (request, _) = app.pending().await;
    assert!(request.starts_with(
        "GET /api/v10/users/42/profile?with_mutual_guilds=false&with_mutual_friends=false"
    ));
    assert!(!request.contains("guild_id"));
    app.running
        .commands
        .send(Command::LoadProfile {
            request: 6,
            user: UserId(43),
            server: Some(ServerId(99)),
            effect: true,
        })
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::ProfileLoad { request: 6, .. },
            ..
        }
    ));
}

fn person(id: u64, status: &str) -> Value {
    json!({"member": {"user": {"id": id.to_string(), "username": format!("p{id}")}, "roles": [],
        "presence": {"status": status}}})
}

#[tokio::test]
async fn the_member_list_subscribes_follows_updates_survives_bad_ones_and_unsubscribes() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, VIEW).await;
    let watch = |first, last| Command::WatchMemberList {
        server: ServerId(10),
        channel: rune_model::ChannelId(30),
        first,
        last,
    };
    app.running.commands.send(watch(0, 20)).await.unwrap();
    let open = app.frame().await;
    assert_eq!(open["op"], 37);
    assert_eq!(open["d"]["subscriptions"]["10"]["channels"], json!({}));
    let ranges = app.frame().await;
    assert_eq!(
        ranges["d"]["subscriptions"]["10"]["channels"],
        json!({"30": [[0, 99]]})
    );
    // The same view again sends nothing.
    app.running.commands.send(watch(1, 21)).await.unwrap();

    let groups = json!([{"id": "online", "count": 2}, {"id": "offline", "count": 1}]);
    app.dispatch
        .send((
            "GUILD_MEMBER_LIST_UPDATE",
            json!({"guild_id": "10", "id": "everyone", "groups": groups,
            "ops": [{"op": "SYNC", "range": [0, 99], "items": [
                {"group": {"id": "online", "count": 2}}, person(43, "online"), person(44, "idle"),
                {"group": {"id": "offline", "count": 1}}, person(45, "offline")]}]}),
        ))
        .await
        .unwrap();
    let list = app.list().await;
    assert_eq!(list.len(), 5);
    assert_eq!(list.rows[1], (1, ListRow::Member(UserId(43))));

    // Malformed updates are dropped; the connection stays and goes on.
    for bad in [
        json!({"guild_id": "10", "id": "everyone", "ops": "x"}),
        json!({"guild_id": "10", "id": "everyone", "ops": [{"op": "DELETE", "index": "1"}]}),
        json!({"guild_id": "10"}),
    ] {
        app.dispatch
            .send(("GUILD_MEMBER_LIST_UPDATE", bad))
            .await
            .unwrap();
    }
    app.dispatch
        .send(("GUILD_MEMBER_LIST_UPDATE", json!({"guild_id": "10", "id": "everyone",
            "groups": [{"id": "online", "count": 1}, {"id": "offline", "count": 2}],
            "ops": [{"op": "DELETE", "index": 2}, {"op": "INSERT", "index": 4, "item": person(44, "offline")}]})))
        .await
        .unwrap();
    let list = app.list().await;
    assert_eq!(
        list.rows
            .iter()
            .filter_map(|(at, row)| match row {
                ListRow::Member(user) => Some((*at, user.0)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![(1, 43), (3, 45), (4, 44)]
    );

    // Scrolling changes only the ranges; leaving unsubscribes.
    app.running.commands.send(watch(150, 180)).await.unwrap();
    let scrolled = app.frame().await;
    assert_eq!(
        scrolled["d"]["subscriptions"]["10"]["channels"],
        json!({"30": [[100, 199]]})
    );
    app.running
        .commands
        .send(Command::UnwatchMemberList)
        .await
        .unwrap();
    let left = app.frame().await;
    assert_eq!(left["d"]["subscriptions"]["10"]["typing"], false);
    assert_eq!(left["d"]["subscriptions"]["10"]["channels"], json!({}));
    assert!(app.frames.try_recv().is_err(), "one frame per change");
}
