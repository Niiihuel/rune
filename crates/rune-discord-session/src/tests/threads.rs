//! Threads and forum posts over loopback fixtures: the routes and bodies
//! Serein uses, the answers read back, and malformed pages refused.

use super::*;
use rune_model::{ChannelId, MessageId, TagId};
use rune_provider::{Created, ThreadEdit, ThreadScope};
use serde_json::{json, Value};

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

fn thread(id: u64, parent: u64, kind: u64, archived_at: Option<&str>) -> Value {
    let mut value = json!({"id":id.to_string(),"guild_id":"10","type":kind,"name":"Hilo",
        "parent_id":parent.to_string(),"owner_id":"9",
        "thread_metadata":{"archived":archived_at.is_some(),"locked":false}});
    if let Some(at) = archived_at {
        value["thread_metadata"]["archive_timestamp"] = json!(at);
    }
    value
}

#[tokio::test]
async fn threads_start_from_a_message_or_on_their_own_by_the_documented_routes() {
    for (message, private, route, body) in [
        (
            Some(100),
            false,
            "POST /api/v10/channels/20/messages/100/threads",
            json!({"name":"Dudas","auto_archive_duration":4320}),
        ),
        (
            None,
            true,
            "POST /api/v10/channels/20/threads",
            json!({"name":"Dudas","auto_archive_duration":4320,"type":12}),
        ),
    ] {
        let answer = thread(61, 20, if private { 12 } else { 11 }, None);
        let (result, line, sent) = exchange(
            Command::CreateThread {
                request: 3,
                channel: ChannelId(20),
                message: message.map(MessageId),
                name: " Dudas ".into(),
                private,
            },
            response(201, &answer.to_string()),
        )
        .await;
        assert_eq!(line, format!("{route} HTTP/1.1"));
        assert_eq!(sent, Some(body));
        let events = result.unwrap();
        assert!(matches!(&events[0], Event::ChannelChanged(c)
            if c.id == ChannelId(61) && c.is_thread()
                && c.thread.as_ref().unwrap().private == private));
        assert_eq!(
            events[1],
            Event::Created {
                request: 3,
                created: Created::Channel(ChannelId(61)),
            }
        );
    }
    // An answer under another parent: it may have been made, unclear.
    let (result, _, _) = exchange(
        Command::CreateThread {
            request: 3,
            channel: ChannelId(20),
            message: None,
            name: "Dudas".into(),
            private: false,
        },
        response(201, &thread(61, 21, 11, None).to_string()),
    )
    .await;
    assert!(result.unwrap_err().ends_with("(ambiguous)"));
}

#[tokio::test]
async fn a_post_carries_its_title_tags_and_first_message() {
    let (result, line, body) = exchange(
        Command::CreatePost {
            request: 4,
            forum: ChannelId(23),
            title: "¿Qué lente?".into(),
            tags: vec![TagId(70), TagId(72)],
            content: "Para retratos.".into(),
            files: Vec::new(),
        },
        response(201, &thread(62, 23, 11, None).to_string()),
    )
    .await;
    assert_eq!(line, "POST /api/v10/channels/23/threads HTTP/1.1");
    assert_eq!(
        body,
        Some(json!({"name":"¿Qué lente?","auto_archive_duration":4320,
            "applied_tags":["70","72"],
            "message":{"content":"Para retratos.",
                "allowed_mentions":{"parse":["users"],"replied_user":false}}}))
    );
    assert!(matches!(
        &result.unwrap()[1],
        Event::Created { request: 4, .. }
    ));
}

#[tokio::test]
async fn following_editing_and_deleting_use_the_thread_routes() {
    for (follow, route) in [
        (true, "PUT /api/v10/channels/40/thread-members/@me"),
        (false, "DELETE /api/v10/channels/40/thread-members/@me"),
    ] {
        let (result, line, body) = exchange(
            Command::FollowThread {
                thread: ChannelId(40),
                follow,
            },
            "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        // The provider marks it once this succeeds: see `thread_confirmed`.
        assert_eq!(result, Ok(Vec::new()));
        assert_eq!(line, format!("{route} HTTP/1.1"));
        assert_eq!(body, None);
    }
    let mut closed = thread(40, 20, 11, Some("2026-10-17T12:00:00+00:00"));
    closed["name"] = json!("Resuelto");
    closed["thread_metadata"]["locked"] = json!(true);
    let (result, line, body) = exchange(
        Command::EditThread {
            thread: ChannelId(40),
            edit: ThreadEdit {
                name: Some(" Resuelto ".into()),
                archived: Some(true),
                locked: Some(true),
                tags: None,
            },
        },
        response(200, &closed.to_string()),
    )
    .await;
    assert_eq!(line, "PATCH /api/v10/channels/40 HTTP/1.1");
    assert_eq!(
        body,
        Some(json!({"name":"Resuelto","archived":true,"locked":true}))
    );
    let events = result.unwrap();
    let [Event::ChannelChanged(changed)] = events.as_slice() else {
        panic!("{events:?}");
    };
    let info = changed.thread.as_ref().unwrap();
    assert!(info.archived && info.locked && changed.name == "Resuelto");
    let (result, line, _) = exchange(
        Command::EditThread {
            thread: ChannelId(40),
            edit: ThreadEdit {
                tags: Some(vec![TagId(70)]),
                ..Default::default()
            },
        },
        response(200, &thread(41, 20, 11, None).to_string()),
    )
    .await;
    assert_eq!(line, "PATCH /api/v10/channels/40 HTTP/1.1");
    assert!(
        result.unwrap_err().ends_with("(ambiguous)"),
        "another thread answered"
    );
    let (result, line, _) = exchange(
        Command::DeleteThread {
            thread: ChannelId(40),
        },
        response(200, &thread(40, 20, 11, None).to_string()),
    )
    .await;
    assert_eq!(line, "DELETE /api/v10/channels/40 HTTP/1.1");
    assert_eq!(
        result,
        Ok(vec![Event::ChannelRemoved {
            channel: ChannelId(40)
        }])
    );
}

#[tokio::test]
async fn archived_pages_follow_their_cursor_and_refuse_what_is_out_of_order() {
    let newer = "2026-09-10T12:00:00.123456+00:00";
    let older = "2026-09-09T08:00:00+00:00";
    let page = json!({"threads":[thread(51, 20, 11, Some(newer)), thread(50, 20, 11, Some(older))],
        "members":[{"id":"50","user_id":"9"}],"has_more":true});
    let command = |scope, before: Option<&str>| Command::LoadThreads {
        request: 8,
        parent: ChannelId(20),
        scope,
        before: before.map(str::to_owned),
    };
    let (result, line, body) = exchange(
        command(ThreadScope::Archived, None),
        response(200, &page.to_string()),
    )
    .await;
    assert_eq!(
        line,
        "GET /api/v10/channels/20/threads/archived/public?limit=25 HTTP/1.1"
    );
    assert_eq!(body, None);
    let events = result.unwrap();
    assert_eq!(events.len(), 3);
    assert!(matches!(&events[1], Event::ChannelChanged(c)
        if c.id == ChannelId(50) && c.thread.as_ref().unwrap().joined));
    assert_eq!(
        events[2],
        Event::ThreadsLoaded {
            request: 8,
            parent: ChannelId(20),
            scope: ThreadScope::Archived,
            threads: vec![ChannelId(51), ChannelId(50)],
            // The instant of the last one, and who of the page was archived then.
            next: Some(format!("{older}|50")),
        }
    );
    // A cursor without ids asks strictly before it, every byte escaped.
    let end = json!({"threads":[],"members":[],"has_more":false});
    let (result, line, _) = exchange(
        command(ThreadScope::Archived, Some(older)),
        response(200, &end.to_string()),
    )
    .await;
    let escaped: String = older.bytes().map(|b| format!("%{b:02X}")).collect();
    assert_eq!(
        line,
        format!(
            "GET /api/v10/channels/20/threads/archived/public?limit=25&before={escaped} HTTP/1.1"
        )
    );
    assert!(matches!(
        result.unwrap().last(),
        Some(Event::ThreadsLoaded { next: None, .. })
    ));
    // The account's own private ones page by id.
    let mine = json!({"threads":[thread(57, 20, 12, Some(newer))],"has_more":true});
    let (result, line, _) = exchange(
        command(ThreadScope::Joined, Some("60")),
        response(200, &mine.to_string()),
    )
    .await;
    assert_eq!(
        line,
        "GET /api/v10/channels/20/users/@me/threads/archived/private?limit=25&before=%36%30 HTTP/1.1"
    );
    assert!(matches!(result.unwrap().last(),
        Some(Event::ThreadsLoaded { next: Some(next), .. }) if next == "57"));
    // Malformed: out of order, past the cursor, the wrong kind, more with nothing.
    for (scope, before, page) in [
        (
            ThreadScope::Archived,
            None,
            json!({"threads":[thread(50, 20, 11, Some(older)), thread(51, 20, 11, Some(newer))],"has_more":false}),
        ),
        (
            ThreadScope::Archived,
            Some(older),
            json!({"threads":[thread(51, 20, 11, Some(newer))],"has_more":false}),
        ),
        (
            ThreadScope::Archived,
            None,
            json!({"threads":[thread(51, 20, 12, Some(newer))],"has_more":false}),
        ),
        (
            ThreadScope::Archived,
            None,
            json!({"threads":[thread(51, 21, 11, Some(newer))],"has_more":false}),
        ),
        (
            ThreadScope::Joined,
            Some("50"),
            json!({"threads":[thread(57, 20, 12, Some(newer))],"has_more":false}),
        ),
        (
            ThreadScope::Archived,
            None,
            json!({"threads":[],"has_more":true}),
        ),
        (
            ThreadScope::Archived,
            None,
            json!({"threads":[thread(51, 20, 11, None)],"has_more":false}),
        ),
    ] {
        let (result, _, _) =
            exchange(command(scope, before), response(200, &page.to_string())).await;
        let error = result.unwrap_err();
        assert!(
            !error.ends_with("(ambiguous)"),
            "a read is never ambiguous: {page}"
        );
    }
}

#[tokio::test]
async fn a_thread_is_silenced_by_its_member_settings_as_the_official_client_sends_them() {
    let route = "PATCH /api/v10/channels/40/thread-members/@me/settings HTTP/1.1";
    let soon = rune_model::Timestamp(crate::messages::wall_now().0 + 15 * 60_000);
    for (until, expected) in [
        (
            Some(rune_model::MUTED_FOREVER),
            json!({"muted":true,"mute_config":{"end_time":null,"selected_time_window":-1}}),
        ),
        (None, json!({"muted":false})),
    ] {
        let (result, line, body) = exchange(
            Command::MuteThread {
                thread: ChannelId(40),
                until,
            },
            response(200, r#"{"flags":0,"muted":true}"#),
        )
        .await;
        // The provider marks it once this succeeds: see `thread_confirmed`.
        assert_eq!(result, Ok(Vec::new()));
        assert_eq!(line, route);
        assert_eq!(body, Some(expected));
    }
    let (_, _, body) = exchange(
        Command::MuteThread {
            thread: ChannelId(40),
            until: Some(soon),
        },
        response(200, "{}"),
    )
    .await;
    let body = body.unwrap();
    // The window chosen, to the minute, and when it ends.
    assert_eq!(body["mute_config"]["selected_time_window"], json!(900));
    assert_eq!(
        crate::wire::timestamp(&body["mute_config"]["end_time"]).unwrap(),
        soon
    );
}

#[tokio::test]
async fn a_forums_open_page_says_what_each_card_shows_and_who_silenced_what() {
    let mut first = rest::message(62, "Busco un **lente** luminoso");
    first["channel_id"] = json!("62");
    first["reactions"] = json!([{"emoji":{"name":"👍"},"count":3,"me":false}]);
    first["attachments"] = json!([{"id":"5","filename":"lente.jpg","size":10,
        "url":"https://cdn.discordapp.com/attachments/62/5/lente.jpg",
        "proxy_url":"https://media.discordapp.net/attachments/62/5/lente.jpg",
        "content_type":"image/jpeg","width":40,"height":30}]);
    // Another thread's first message is not this page's.
    let mut stray = rest::message(99, "otro");
    stray["channel_id"] = json!("99");
    let page = json!({"threads":[thread(62, 23, 11, None), thread(63, 23, 11, None)],
        "members":[{"id":"63","user_id":"9","muted":true,"mute_config":{"end_time":null}}],
        "first_messages":[first, stray, {"broken":true}],"has_more":false});
    let (http, _) = fixtures(vec![response(200, &page.to_string())]).await;
    let events = http
        .active_threads(
            &secret(),
            9,
            ChannelId(23),
            rune_model::ServerId(10),
            true,
            None,
        )
        .await
        .unwrap();
    let thread_of = |id: u64| match events
        .iter()
        .find(|event| matches!(event, Event::ChannelChanged(c) if c.id == ChannelId(id)))
    {
        Some(Event::ChannelChanged(channel)) => channel.thread.clone().unwrap(),
        _ => panic!("{events:?}"),
    };
    let starter = thread_of(62).starter.expect("its first message was listed");
    assert_eq!(starter.excerpt, "Busco un **lente** luminoso");
    assert_eq!(starter.reactions, vec![("👍".to_owned(), 3)]);
    assert!(starter.image.is_some());
    let silenced = thread_of(63);
    assert!(silenced.joined && silenced.starter.is_none());
    assert_eq!(silenced.muted_until, Some(rune_model::MUTED_FOREVER));
}

#[tokio::test]
async fn an_archived_page_includes_its_cursors_instant_and_leaves_out_what_was_read() {
    let edge = "2026-09-09T08:00:00.250000+00:00";
    let earlier = "2026-09-08T08:00:00+00:00";
    let command = |before: &str| Command::LoadThreads {
        request: 4,
        parent: ChannelId(20),
        scope: ThreadScope::Archived,
        before: Some(before.to_owned()),
    };
    // 50 was read at the edge; 52 was archived in the same instant and not.
    let page = json!({"threads":[thread(50, 20, 11, Some(edge)), thread(52, 20, 11, Some(edge)),
        thread(49, 20, 11, Some(earlier))],"has_more":true});
    let (result, line, _) = exchange(
        command(&format!("{edge}|50")),
        response(200, &page.to_string()),
    )
    .await;
    // A millisecond past the instant, as `before` leaves it out.
    let asked: String = "2026-09-09T08:00:00.251000Z"
        .bytes()
        .map(|b| format!("%{b:02X}"))
        .collect();
    assert_eq!(
        line,
        format!(
            "GET /api/v10/channels/20/threads/archived/public?limit=25&before={asked} HTTP/1.1"
        )
    );
    let events = result.unwrap();
    assert_eq!(
        events.last(),
        Some(&Event::ThreadsLoaded {
            request: 4,
            parent: ChannelId(20),
            scope: ThreadScope::Archived,
            threads: vec![ChannelId(52), ChannelId(49)],
            next: Some(format!("{earlier}|49")),
        })
    );
    // A page ending in the cursor's instant carries the ids read before too.
    let same = json!({"threads":[thread(52, 20, 11, Some(edge))],"has_more":true});
    let (result, _, _) = exchange(
        command(&format!("{edge}|50")),
        response(200, &same.to_string()),
    )
    .await;
    assert!(matches!(result.unwrap().last(),
        Some(Event::ThreadsLoaded { next: Some(next), .. }) if *next == format!("{edge}|52,50")));
    // Nothing new in a page that claims more: it goes on before the instant.
    let read = json!({"threads":[thread(50, 20, 11, Some(edge))],"has_more":true});
    let (result, _, _) = exchange(
        command(&format!("{edge}|50")),
        response(200, &read.to_string()),
    )
    .await;
    assert!(matches!(result.unwrap().last(),
        Some(Event::ThreadsLoaded { threads, next: Some(next), .. })
            if threads.is_empty() && next == edge));
    // Past the millisecond asked for is malformed, and so is a bad cursor.
    let past = json!({"threads":[thread(51, 20, 11, Some("2026-09-09T08:00:00.252+00:00"))],
        "has_more":false});
    let (result, _, _) = exchange(
        command(&format!("{edge}|50")),
        response(200, &past.to_string()),
    )
    .await;
    assert!(result.is_err());
    for cursor in [format!("{edge}|"), format!("{edge}|0"), format!("{edge}|x")] {
        assert!(!crate::bootstrap::valid_cursor(
            ThreadScope::Archived,
            &cursor
        ));
    }
    let crowded: Vec<String> = (1..=crate::bootstrap::MAX_EDGE as u64 + 1)
        .map(|id| id.to_string())
        .collect();
    assert!(!crate::bootstrap::valid_cursor(
        ThreadScope::Archived,
        &format!("{edge}|{}", crowded.join(","))
    ));
}

/// A fixture that answers `replies` in order, one connection each, and
/// hands back the request line of each.
async fn fixtures(replies: Vec<String>) -> (Http, tokio::task::JoinHandle<Vec<String>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut lines = Vec::new();
        for reply in replies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let headers = request(&mut socket).await;
            lines.push(headers.lines().next().unwrap().to_owned());
            tokio::io::AsyncWriteExt::write_all(&mut socket, reply.as_bytes())
                .await
                .unwrap();
        }
        lines
    });
    (Http::local(&base), task)
}

#[tokio::test]
async fn open_posts_come_from_the_forum_search_and_a_channels_from_the_guild_list() {
    let search = |offset: usize| {
        format!(
            "GET /api/v10/channels/23/threads/search?archived=false&sort_by=last_message_time&sort_order=desc&limit=25&offset={offset} HTTP/1.1"
        )
    };
    let page = json!({"threads":[thread(62, 23, 11, None), thread(63, 23, 11, None)],
        "members":[{"id":"63","user_id":"9"}],"has_more":true,"total_results":3});
    let (http, lines) = fixtures(vec![response(200, &page.to_string())]).await;
    let events = http
        .active_threads(
            &secret(),
            9,
            ChannelId(23),
            rune_model::ServerId(10),
            true,
            None,
        )
        .await
        .unwrap();
    assert_eq!(lines.await.unwrap(), vec![search(0)]);
    assert!(matches!(&events[1], Event::ChannelChanged(c)
        if c.id == ChannelId(63) && c.thread.as_ref().unwrap().joined));
    assert_eq!(
        events[2],
        Event::ThreadsLoaded {
            request: 9,
            parent: ChannelId(23),
            scope: ThreadScope::Active,
            threads: vec![ChannelId(62), ChannelId(63)],
            next: Some("2".into()),
        }
    );
    // A forum whose search is refused: the guild's list, first page only.
    let guild =
        json!({"threads":[thread(62, 23, 11, None), thread(70, 20, 11, None)],"members":[]});
    let (http, lines) = fixtures(vec![
        response(404, r#"{"code":10003}"#),
        response(200, &guild.to_string()),
    ])
    .await;
    let events = http
        .active_threads(
            &secret(),
            10,
            ChannelId(23),
            rune_model::ServerId(10),
            true,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        lines.await.unwrap(),
        vec![
            search(0),
            "GET /api/v10/guilds/10/threads/active HTTP/1.1".to_owned()
        ]
    );
    // Only this forum's, and nothing more after the guild's list.
    assert!(
        matches!(events.last(), Some(Event::ThreadsLoaded { threads, next: None, request: 10, .. })
        if *threads == vec![ChannelId(62)])
    );
    // A later page that fails stays a failure.
    let (http, _) = fixtures(vec![response(500, "{}")]).await;
    assert!(http
        .active_threads(
            &secret(),
            11,
            ChannelId(23),
            rune_model::ServerId(10),
            true,
            Some("25")
        )
        .await
        .is_err());
    // An archived thread in an open page is malformed.
    let wrong =
        json!({"threads":[thread(62, 23, 11, Some("2026-09-10T12:00:00+00:00"))],"has_more":false});
    let (http, _) = fixtures(vec![response(200, &wrong.to_string())]).await;
    assert!(http
        .active_threads(
            &secret(),
            12,
            ChannelId(23),
            rune_model::ServerId(10),
            true,
            None
        )
        .await
        .is_err());
}
