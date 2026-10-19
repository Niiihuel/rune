use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn preview() -> String {
    serde_json::json!({
        "type": 0, "code": "synthetic", "guild": {"id":"123", "name":"Servidor de prueba", "description":null},
        "approximate_member_count":40,"approximate_presence_count":3
    }).to_string()
}

async fn server(
    status: &str,
    headers: &str,
    body: String,
) -> (String, tokio::task::JoinHandle<String>) {
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
        body.len()
    );
    let task = tokio::spawn(async move {
        let (mut connection, _) = socket.accept().await.unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
            if connection.read(&mut byte).await.unwrap() == 0 {
                break;
            }
            request.push(byte[0]);
        }
        connection.write_all(response.as_bytes()).await.unwrap();
        String::from_utf8(request).unwrap()
    });
    (format!("http://{address}"), task)
}

#[test]
fn invitation_parser_keeps_hosts_paths_and_identity_out_of_urls() {
    let expected = target("https://discord.gg/synthetic").unwrap();
    assert_eq!(
        target("https://discord.com/invite/synthetic?utm_source=x"),
        Some(expected.clone())
    );
    assert!(!format!("{expected:?}").contains("synthetic"));
    for invalid in [
        "http://discord.gg/synthetic",
        "https://discord.gg.evil.test/synthetic",
        "https://discord.gg@evil.test/synthetic",
        "https://user@discord.gg/synthetic",
        "https://discord.gg:444/synthetic",
        "https://discord.gg/a/b",
        "https://discord.gg/%2e%2e",
        "https://discord.com/channels/123",
        "https://discord.gg/",
    ] {
        assert!(target(invalid).is_none(), "{invalid}");
    }
    assert!(target(&format!("https://discord.gg/{}", "a".repeat(101))).is_none());
}

#[test]
fn optional_description_and_artwork_stay_bounded_without_accepting_remote_urls() {
    let mut data: Value = serde_json::from_str(&preview()).unwrap();
    data["guild"]["description"] = Value::String(String::new());
    data["guild"]["icon"] = Value::String("https://evil.invalid/private.png".into());
    let result = decode("synthetic", &serde_json::to_vec(&data).unwrap()).unwrap();
    assert_eq!(result.description, None);
    assert_eq!(result.image, None);
    data["guild"]["description"] = Value::String("Primera línea\nSegunda línea".into());
    data["guild"]["icon"] = Value::String("a_0123456789abcdef0123456789abcdef".into());
    let result = decode("synthetic", &serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(result.description.unwrap().contains('\n'));
    assert_eq!(
        result.image.unwrap().url,
        "https://cdn.discordapp.com/icons/123/a_0123456789abcdef0123456789abcdef.png?size=128"
    );
    data["guild"]["description"] = Value::String("x".repeat(2049));
    assert_eq!(
        decode("synthetic", &serde_json::to_vec(&data).unwrap()),
        Err(Error::Unavailable)
    );
}

#[test]
fn group_friend_and_guest_invitations_say_where_they_lead() {
    let decoded = |data: Value| decode("synthetic", &serde_json::to_vec(&data).unwrap());
    let icon = "0123456789abcdef0123456789abcdef";
    // A group, by its name or, without one, by its people.
    let mut group = serde_json::json!({
        "type": 1, "code": "synthetic",
        "channel": {"id": "55", "type": 3, "name": "Los jueves", "icon": icon,
                    "recipients": [{"username": "ada"}, {"username": "bruno"}]},
        "inviter": {"id": "9", "username": "ada"},
        "approximate_member_count": 3
    });
    let found = decoded(group.clone()).unwrap();
    assert_eq!(found.destination, Destination::Group(ChannelId(55)));
    assert_eq!(found.name, "Los jueves");
    assert_eq!(found.members, Some(3));
    assert_eq!(
        found.image.unwrap().url,
        format!("https://cdn.discordapp.com/channel-icons/55/{icon}.png?size=128")
    );
    group["channel"]["name"] = Value::Null;
    assert_eq!(decoded(group.clone()).unwrap().name, "ada, bruno");
    group["channel"]["recipients"] = serde_json::json!([]);
    assert_eq!(decoded(group).unwrap().name, "");

    // A friendship, by who offers it.
    let friend = serde_json::json!({
        "type": 2, "code": "synthetic",
        "inviter": {"id": "9", "username": "ada", "global_name": "Ada L.", "avatar": icon},
        "channel": null
    });
    let found = decoded(friend.clone()).unwrap();
    assert_eq!(found.destination, Destination::Friend(UserId(9)));
    assert_eq!(found.name, "Ada L.");
    assert_eq!(
        found.image.unwrap().url,
        format!("https://cdn.discordapp.com/avatars/9/{icon}.png?size=128")
    );
    let mut nameless = friend;
    nameless["inviter"]["global_name"] = Value::Null;
    assert_eq!(decoded(nameless).unwrap().name, "ada");

    // A guest's way into one voice channel of a server.
    let mut guest: Value = serde_json::from_str(&preview()).unwrap();
    guest["flags"] = serde_json::json!(1);
    guest["channel"] = serde_json::json!({"id": "77", "type": 2, "name": "Al aire"});
    let found = decoded(guest.clone()).unwrap();
    assert_eq!(
        found.destination,
        Destination::Guest {
            server: ServerId(123),
            channel: ChannelId(77)
        }
    );
    assert_eq!(found.name, "Servidor de prueba");

    // Kinds and flags without a flow here, or without their ids, lead nowhere.
    for broken in [
        serde_json::json!({"type": 3, "code": "synthetic"}),
        serde_json::json!({"type": 2, "code": "synthetic", "inviter": {"id": "0"}}),
        serde_json::json!({"type": 1, "code": "synthetic", "flags": 1, "channel": {"id": "5"}}),
        serde_json::json!({"type": 0, "code": "synthetic", "flags": 1, "guild": {"id": "5", "name": "x"}}),
        serde_json::json!({"type": 0, "code": "synthetic", "flags": "1", "guild": {"id": "5", "name": "x"}}),
    ] {
        assert_eq!(decoded(broken.clone()), Err(Error::Invalid), "{broken}");
    }
}

#[test]
fn preview_expiration_is_optional_validated_and_retains_the_reported_instant() {
    let mut data: Value = serde_json::from_str(&preview()).unwrap();
    assert_eq!(
        decode("synthetic", &serde_json::to_vec(&data).unwrap())
            .unwrap()
            .expires_at,
        None
    );
    data["expires_at"] = Value::String("2030-01-02T03:04:05+02:00".into());
    assert_eq!(
        decode("synthetic", &serde_json::to_vec(&data).unwrap())
            .unwrap()
            .expires_at,
        Some(rune_model::Timestamp(1_893_546_245_000))
    );
    data["expires_at"] = Value::Null;
    assert_eq!(
        decode("synthetic", &serde_json::to_vec(&data).unwrap())
            .unwrap()
            .expires_at,
        None
    );
    for invalid in [
        serde_json::json!(123),
        serde_json::json!("not-a-date"),
        serde_json::json!("x".repeat(129)),
    ] {
        data["expires_at"] = invalid;
        assert_eq!(
            decode("synthetic", &serde_json::to_vec(&data).unwrap()),
            Err(Error::Unavailable)
        );
    }
}

#[tokio::test]
async fn preview_uses_only_public_get_and_never_sends_authorization_or_cookies() {
    let (base, task) = server("200 OK", "", preview()).await;
    let result = fetch(target("https://discord.gg/synthetic").unwrap(), &base)
        .await
        .unwrap();
    assert_eq!(result.destination, Destination::Server(ServerId(123)));
    assert_eq!(result.members, Some(40));
    let request = task.await.unwrap().to_ascii_lowercase();
    assert!(request.starts_with("get /invites/synthetic?with_counts=true http/1.1"));
    assert!(!request.contains("authorization:"));
    assert!(!request.contains("cookie:"));
}

#[tokio::test]
async fn redirects_and_remote_error_bodies_never_escape_the_transport() {
    let trap = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let header = format!(
        "Location: http://{}/capture\r\n",
        trap.local_addr().unwrap()
    );
    let (base, task) = server(
        "307 Temporary Redirect",
        &header,
        "PRIVATE_REMOTE_BODY".into(),
    )
    .await;
    let result = fetch(target("https://discord.gg/synthetic").unwrap(), &base).await;
    assert_eq!(result, Err(Error::Unavailable));
    task.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(40), trap.accept())
            .await
            .is_err()
    );
    let (base, task) = server(
        "429 Too Many Requests",
        "Retry-After: 1.5\r\n",
        "PRIVATE_REMOTE_BODY".into(),
    )
    .await;
    assert_eq!(
        fetch(target("https://discord.gg/synthetic").unwrap(), &base).await,
        Err(Error::RateLimited {
            retry_after_secs: 2
        })
    );
    task.await.unwrap();
}

#[tokio::test]
async fn oversized_and_mismatched_previews_are_rejected() {
    let (base, task) = server("200 OK", "", "x".repeat(MAX_BODY + 1)).await;
    assert_eq!(
        fetch(target("https://discord.gg/synthetic").unwrap(), &base).await,
        Err(Error::Unavailable)
    );
    task.await.unwrap();
    assert_eq!(decode("other", preview().as_bytes()), Err(Error::Invalid));
    assert_eq!(
        decode("synthetic", br#"{"type":1,"code":"synthetic"}"#),
        Err(Error::Invalid)
    );
    let invalid = InvitationTarget {
        key: "../private".into(),
        url: "https://discord.gg/synthetic".into(),
    };
    assert_eq!(
        fetch(invalid, "http://127.0.0.1:1").await,
        Err(Error::Invalid)
    );
}

// Scheduled events (eventos-programados).
#[tokio::test]
async fn an_invitation_naming_an_event_asks_for_it_and_shows_only_that_one() {
    let mut body: serde_json::Value = serde_json::from_str(&preview()).unwrap();
    body["guild_scheduled_event"] = serde_json::json!({"id": "90", "guild_id": "123",
        "channel_id": null, "creator_id": null, "name": "Salida", "description": null,
        "scheduled_start_time": "2030-01-02T18:00:00+00:00",
        "scheduled_end_time": "2030-01-02T20:00:00+00:00", "privacy_level": 2, "status": 1,
        "entity_type": 3, "entity_metadata": {"location": "Costanera"}, "user_count": 4});
    let (base, task) = server("200 OK", "", body.to_string()).await;
    let named = fetch(
        target("https://discord.gg/synthetic?event=90").unwrap(),
        &base,
    )
    .await
    .unwrap();
    let event = named.event.expect("the event it names");
    assert_eq!((event.id.0, event.interested), (90, Some(4)));
    let request = task.await.unwrap();
    assert!(
        request.starts_with(
            "GET /invites/synthetic?with_counts=true&guild_scheduled_event_id=90 HTTP/1.1"
        ),
        "{request}"
    );
    // One the link does not name is left out; a broken one too.
    let (base, _task) = server("200 OK", "", body.to_string()).await;
    let other = fetch(
        target("https://discord.gg/synthetic?event=91").unwrap(),
        &base,
    )
    .await
    .unwrap();
    assert!(other.event.is_none());
    body["guild_scheduled_event"]["name"] = serde_json::Value::Null;
    let (base, _task) = server("200 OK", "", body.to_string()).await;
    let broken = fetch(
        target("https://discord.gg/synthetic?event=90").unwrap(),
        &base,
    )
    .await
    .unwrap();
    assert!(broken.event.is_none() && broken.members == Some(40));
}
