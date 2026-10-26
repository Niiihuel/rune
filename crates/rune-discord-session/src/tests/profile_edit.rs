//! Changing the account's profile, against loopback fixtures only.

use rune_model::{Member, MemberRestrictions, ProfileBanner, Rgb, ServerId, Timestamp, User};
use rune_provider::{Failure, ProfileEdit, ProfileImage, ServerProfileEdit, Snapshot};

use super::*;

const HASH: &str = "0123456789abcdef0123456789abcdef";

/// One answer per connection, in order; the requests come out as they arrive.
async fn fixtures(replies: Vec<String>) -> (Http, mpsc::UnboundedReceiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (sent, received) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        for reply in replies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let _ = sent.send(request(&mut socket).await);
            let _ = socket.write_all(reply.as_bytes()).await;
        }
    });
    (Http::local(&base), received)
}

fn png() -> ProfileImage {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    bytes.extend(64u32.to_be_bytes());
    bytes.extend(64u32.to_be_bytes());
    ProfileImage::new(bytes.into()).unwrap()
}

/// A moving picture's header (edicion-de-perfil).
fn gif() -> ProfileImage {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend(64u16.to_le_bytes());
    bytes.extend(64u16.to_le_bytes());
    ProfileImage::new(bytes.into()).unwrap()
}

fn me() -> User {
    User {
        identity: Default::default(),
        id: UserId(42),
        name: "rune-test".into(),
        display_name: Some("Rune Test".into()),
        accent: Rgb(0),
        avatar: None,
        banner: ProfileBanner::Unknown,
        bot: false,
        pronouns: None,
        about: None,
        default_avatar: None,
    }
}

/// The JSON body of a request the fixture saw.
fn body(request: &str) -> serde_json::Value {
    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap()
}

#[tokio::test]
async fn the_account_saves_its_name_and_picture_then_its_profile_then_reads_it_back() {
    let user = format!(
        r#"{{"id":"42","username":"rune-test","global_name":"Nuevo","avatar":"{HASH}","bot":false}}"#
    );
    let profile = format!(
        r#"{{"user":{{"id":"42","username":"rune-test"}},"user_profile":{{"bio":"Hola","pronouns":"elle","banner":"{HASH}","accent_color":66051}}}}"#
    );
    let (http, mut requests) = fixtures(vec![
        response(200, &user),
        response(200, r#"{"bio":"Hola"}"#),
        response(200, &profile),
    ])
    .await;
    let edit = ProfileEdit {
        display_name: Some(Some("  Nuevo ".into())),
        avatar: Some(Some(png())),
        about: Some(Some("Hola".into())),
        pronouns: Some(None),
        accent: Some(Rgb(66051)),
        banner: Some(Some(png())),
    };
    let events = http.edit_profile(&secret(), &me(), &edit).await.unwrap();

    let first = requests.recv().await.unwrap();
    assert!(
        first.starts_with("PATCH /api/v10/users/@me HTTP/1.1\r\n"),
        "{first}"
    );
    let sent = body(&first);
    assert_eq!(sent["global_name"], "Nuevo");
    assert!(sent["avatar"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,iVBORw0KGgo"));
    assert!(sent.get("bio").is_none(), "the profile's own route");
    let second = requests.recv().await.unwrap();
    assert!(second.starts_with("PATCH /api/v10/users/@me/profile HTTP/1.1\r\n"));
    let sent = body(&second);
    assert_eq!(sent["bio"], "Hola");
    assert_eq!(sent["pronouns"], "", "taken away");
    assert_eq!(sent["accent_color"], 66051);
    assert!(sent["banner"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png"));
    let third = requests.recv().await.unwrap();
    assert!(third.starts_with(
        "GET /api/v10/users/42/profile?with_mutual_guilds=false&with_mutual_friends=false"
    ));
    // Nothing that needs the password ever leaves.
    for request in [&first, &second, &third] {
        assert!(!request.contains("password") && !request.contains("email"));
    }

    let [Event::UserChanged(saved), Event::ProfileChanged(profile)] = &events[..] else {
        panic!("{events:?}");
    };
    assert_eq!(saved.display_name.as_deref(), Some("Nuevo"));
    assert!(saved.avatar.as_ref().is_some_and(|a| a.url.contains(HASH)));
    assert_eq!(saved.about.as_deref(), Some("Hola"));
    assert_eq!(saved.pronouns.as_deref(), Some("elle"), "as read back");
    assert!(matches!(&saved.banner, ProfileBanner::Image(_)));
    assert_eq!((profile.user, profile.server), (UserId(42), None));
}

#[tokio::test]
async fn the_profile_of_a_server_saves_the_membership_then_its_profile_there() {
    let member = format!(
        r#"{{"user":{{"id":"42","username":"rune-test"}},"roles":[],"nick":"Lab","avatar":"{HASH}"}}"#
    );
    let profile = r#"{"user":{"id":"42","username":"rune-test"},"guild_member":{"user":{"id":"42"},"nick":"Lab","roles":[]},"guild_member_profile":{"guild_id":"10","bio":"Aquí","banner":null}}"#;
    let (http, mut requests) = fixtures(vec![
        response(200, &member),
        response(200, r#"{"bio":"Aquí"}"#),
        response(200, profile),
    ])
    .await;
    let edit = ServerProfileEdit {
        nick: Some(Some("Lab".into())),
        about: Some(Some("Aquí".into())),
        avatar: Some(Some(gif())),
        banner: Some(None),
    };
    let events = http
        .edit_server_profile(&secret(), UserId(42), ServerId(10), &edit)
        .await
        .unwrap();
    let first = requests.recv().await.unwrap();
    assert!(first.starts_with("PATCH /api/v10/guilds/10/members/@me HTTP/1.1\r\n"));
    assert_eq!(body(&first)["nick"], "Lab");
    // A moving picture goes as the GIF it is.
    assert!(body(&first)["avatar"]
        .as_str()
        .unwrap()
        .starts_with("data:image/gif;base64,R0lGODlh"));
    let second = requests.recv().await.unwrap();
    assert!(second.starts_with("PATCH /api/v10/guilds/10/profile/@me HTTP/1.1\r\n"));
    assert_eq!(
        body(&second)["banner"],
        serde_json::Value::Null,
        "taken away"
    );
    let third = requests.recv().await.unwrap();
    assert!(third.contains("&guild_id=10"));
    let [Event::MemberChanged(saved), Event::ProfileChanged(profile)] = &events[..] else {
        panic!("{events:?}");
    };
    assert_eq!(saved.nick.as_deref(), Some("Lab"));
    assert!(saved
        .avatar
        .as_ref()
        .is_some_and(|a| a.url.contains("guilds/10/users/42/avatars")));
    assert_eq!(profile.server, Some(ServerId(10)));
    assert_eq!(profile.server_about.as_deref(), Some("Aquí"));
    assert_eq!(profile.server_banner, ProfileBanner::Absent);
}

#[tokio::test]
async fn a_challenge_is_reported_and_a_change_not_read_back_is_not_shown_as_confirmed() {
    let challenge = r#"{"captcha_key":["captcha-required"],"captcha_sitekey":"a5f74b19-9e45-40e0-b45d-47ff91b7a6c2","captcha_service":"hcaptcha"}"#;
    let (http, _requests) = fixtures(vec![response(400, challenge)]).await;
    let edit = ProfileEdit {
        about: Some(Some("Hola".into())),
        ..Default::default()
    };
    let error = http
        .edit_profile(&secret(), &me(), &edit)
        .await
        .unwrap_err();
    assert_eq!(
        error.reason.to_string(),
        "Discord requiere una verificación para completar esta acción."
    );
    assert!(
        error.challenge.is_some(),
        "for the person to solve, elsewhere"
    );

    let user = r#"{"id":"42","username":"rune-test","global_name":"Otro","bot":false}"#;
    let (http, _requests) = fixtures(vec![response(200, user), response(500, "{}")]).await;
    let edit = ProfileEdit {
        display_name: Some(Some("Otro".into())),
        ..Default::default()
    };
    let events = http.edit_profile(&secret(), &me(), &edit).await.unwrap();
    assert!(
        matches!(&events[0], Event::UserChanged(user) if user.display_name.as_deref() == Some("Otro"))
    );
    assert!(matches!(
        &events[1],
        Event::Failed {
            failure: Failure::Profile,
            ambiguous: true,
            ..
        }
    ));
    // Too long never leaves.
    let long = ProfileEdit {
        about: Some(Some("x".repeat(191))),
        ..Default::default()
    };
    assert!(http.edit_profile(&secret(), &me(), &long).await.is_err());
}

#[test]
fn a_profile_changes_only_where_the_account_is_and_is_not_restricted() {
    let mut snapshot = Snapshot {
        me: Some(me()),
        members: vec![Member {
            user: UserId(42),
            server: ServerId(10),
            ..Default::default()
        }],
        ..Default::default()
    };
    let edit = |server| Command::EditServerProfile {
        server: ServerId(server),
        edit: ServerProfileEdit::default(),
    };
    assert!(crate::profiles::edit_allowed(&snapshot, &edit(10)).is_ok());
    assert!(crate::profiles::edit_allowed(&snapshot, &edit(11)).is_err());
    snapshot.members[0].restrictions = MemberRestrictions {
        timeout_until: Some(Timestamp(i64::MAX)),
        ..Default::default()
    };
    assert!(crate::profiles::edit_allowed(&snapshot, &edit(10)).is_err());
    assert!(crate::profiles::edit_allowed(
        &snapshot,
        &Command::EditProfile(ProfileEdit::default())
    )
    .is_ok());
}
