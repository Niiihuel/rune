use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rune_provider::{
    AuthenticationState, Command, Envelope, Event, EventSink, Provider, ProviderKind,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use url::Url;

use crate::{
    auth::{challenge, Attempt, Callback},
    http::Http,
    DiscordProvider,
};

fn attempt() -> Attempt {
    Attempt {
        verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into(),
        state: "nonce-from-this-attempt".into(),
        redirect: Url::parse("http://127.0.0.1:48731/oauth/callback").unwrap(),
    }
}

fn callback(query: &str) -> String {
    format!("GET /oauth/callback?{query} HTTP/1.1\r\nHost: 127.0.0.1:48731\r\n\r\n")
}

#[test]
fn pkce_matches_rfc7636_and_authorization_has_only_requested_scopes() {
    let attempt = attempt();
    assert_eq!(
        challenge(&attempt.verifier),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    let url =
        Url::parse(&attempt.authorize_url("https://discord.com/oauth2/authorize", "42")).unwrap();
    let pairs: HashMap<_, _> = url.query_pairs().collect();
    assert_eq!(pairs["scope"], "identify guilds");
    assert_eq!(pairs["code_challenge_method"], "S256");
    assert!(!url.as_str().contains(&attempt.verifier));
    assert!(!pairs.contains_key("client_secret"));
}

#[test]
fn attempts_use_independent_os_random_verifiers_and_states() {
    let first = Attempt::new(attempt().redirect).unwrap();
    let second = Attempt::new(attempt().redirect).unwrap();
    assert_eq!(first.verifier.len(), 43);
    assert_eq!(first.state.len(), 43);
    assert_ne!(first.verifier, second.verifier);
    assert_ne!(first.state, second.state);
    assert_ne!(first.state, first.verifier);
}

#[test]
fn callback_rejects_wrong_state_duplicates_host_path_issuer_and_ambiguous_outcomes() {
    let attempt = attempt();
    for request in [
        callback("state=another-attempt&code=secret-code"),
        callback("state=nonce-from-this-attempt&state=nonce-from-this-attempt&code=x"),
        callback("state=nonce-from-this-attempt&code=x&code=y"),
        callback("state=nonce-from-this-attempt&code=x&error=denied"),
        callback("state=nonce-from-this-attempt&code=x&iss=https%3A%2F%2Fevil.invalid"),
        callback("state=nonce-from-this-attempt&code=x")
            .replace("Host: 127.0.0.1:48731", "Host: attacker.invalid"),
        callback("state=nonce-from-this-attempt&code=x").replace("/oauth/callback?", "/unrelated?"),
        callback("state=nonce-from-this-attempt&code=x").replace("GET ", "POST "),
        callback("state=nonce-from-this-attempt&code=x")
            .replace("/oauth/callback?", "/\\evil.invalid/oauth/callback?"),
        callback("state=nonce-from-this-attempt&code=x")
            .replace("/oauth/callback?", "http://evil.invalid/oauth/callback?"),
    ] {
        assert!(matches!(attempt.parse(&request), Callback::Invalid));
    }
    assert!(
        matches!(attempt.parse(&callback("state=nonce-from-this-attempt&code=secret-code")), Callback::Code(code) if code == "secret-code")
    );
    assert_eq!(
        attempt.parse(&callback(
            "state=nonce-from-this-attempt&error=access_denied"
        )),
        Callback::Denied
    );
}

#[test]
fn configuration_accepts_only_canonical_public_ids_and_advertises_no_chat() {
    for bad in [
        "",
        "0",
        " 42",
        "042",
        "-2",
        "1e3",
        "18446744073709551616",
        "personal.secret.token",
    ] {
        assert!(DiscordProvider::new(bad.into()).is_err());
    }
    let info = DiscordProvider::new("42".into()).unwrap().info();
    assert_eq!(info.kind, ProviderKind::Official);
    assert!(info.capabilities.authentication);
    assert!(
        !info.capabilities.send_messages
            && !info.capabilities.history
            && !info.capabilities.presence
    );
    assert!(!info.capabilities.audit_log && !info.capabilities.manage_invites);
    assert!(!info.capabilities.manage_expressions && !info.capabilities.moderation);
    assert!(info.notice.contains("experimental"));
}

struct Fixture {
    http: Http,
    requests: mpsc::Receiver<String>,
    task: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new(responses: Vec<(u16, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (tx, requests) = mpsc::channel(32);
        let task = tokio::spawn(async move {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0; 2048];
                loop {
                    let read = stream.read(&mut chunk).await.unwrap();
                    if read == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&chunk[..read]);
                    if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&bytes[..end]);
                        let length = head
                            .lines()
                            .filter_map(|line| line.split_once(':'))
                            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                            .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                    assert!(bytes.len() < 64 * 1024);
                }
                tx.send(String::from_utf8(bytes).unwrap()).await.unwrap();
                let response = format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        Self {
            http: Http::local(&base),
            requests,
            task,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn token(expires: u64) -> String {
    serde_json::json!({"access_token":"fixture-access-secret", "refresh_token":"fixture-refresh-secret", "token_type":"Bearer", "expires_in":expires, "scope":"identify guilds"}).to_string()
}

fn authorization(application: &str, scopes: &[&str]) -> String {
    serde_json::json!({
        "application":{"id":application}, "scopes":scopes,
        "expires":(chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
        "user":{"id":"7", "username":"nihuel", "global_name":"Nihuel", "bot":false, "avatar":"0123456789abcdef0123456789abcdef", "banner":"a_0123456789abcdef0123456789abcdef"}
    })
    .to_string()
}

fn successful(expires: u64) -> Vec<(u16, String)> {
    vec![
        (200, token(expires)),
        (200, authorization("42", &["identify", "guilds"])),
        (
            200,
            r#"[{"id":"13","name":"Servidor autorizado","owner":true}]"#.into(),
        ),
    ]
}

#[tokio::test]
async fn direct_exchange_validates_identity_and_maps_only_available_fields() {
    let mut fixture = Fixture::new(successful(3600)).await;
    let session = fixture
        .http
        .exchange("42", attempt(), "fixture-code".into())
        .await
        .unwrap();
    let snapshot = session.snapshot;
    assert_eq!(snapshot.me.as_ref().unwrap().id.0, 7);
    let me = snapshot.me.as_ref().unwrap();
    assert_eq!(
        me.avatar.as_ref().unwrap().kind,
        rune_model::ImageKind::Artwork
    );
    assert_eq!(
        me.banner.image().unwrap().kind,
        rune_model::ImageKind::Banner
    );
    assert_eq!(
        me.banner.image().unwrap().url,
        "https://cdn.discordapp.com/banners/7/a_0123456789abcdef0123456789abcdef.png?size=512"
    );
    assert_eq!(
        DiscordProvider::new("42".into()).unwrap().image_origins(),
        &["https://cdn.discordapp.com"]
    );
    assert_eq!(snapshot.users.first(), snapshot.me.as_ref());
    assert_eq!(snapshot.servers[0].id.0, 13);
    assert!(
        snapshot.channels.is_empty()
            && snapshot.members.is_empty()
            && snapshot.presences.is_empty()
    );
    assert!(snapshot.servers[0].roles.is_empty());
    let request = fixture.requests.recv().await.unwrap();
    let (_, body) = request.split_once("\r\n\r\n").unwrap();
    let pairs: HashMap<_, _> = url::form_urlencoded::parse(body.as_bytes()).collect();
    assert_eq!(pairs["client_id"], "42");
    assert_eq!(pairs["code_verifier"], attempt().verifier);
    assert!(!pairs.contains_key("client_secret"));
    assert!(!request.contains("fixture-refresh-secret"));
}

#[tokio::test]
async fn exchange_rejects_wrong_application_or_missing_scopes() {
    for (app, scopes) in [
        ("43", vec!["identify", "guilds"]),
        ("42", vec!["identify"]),
        ("42", vec!["identify", "guilds", "bot"]),
    ] {
        let fixture =
            Fixture::new(vec![(200, token(3600)), (200, authorization(app, &scopes))]).await;
        assert!(fixture
            .http
            .exchange("42", attempt(), "fixture-code".into())
            .await
            .is_err());
    }
}

#[tokio::test]
async fn errors_never_echo_remote_body_code_or_token() {
    for (status, body) in [
        (400, "fixture-access-secret"),
        (200, "fixture-refresh-secret"),
        (302, "fixture-code"),
    ] {
        let fixture = Fixture::new(vec![(status, body.into())]).await;
        let error = fixture
            .http
            .exchange("42", attempt(), "fixture-code".into())
            .await
            .err()
            .unwrap();
        let text = format!("{error:?} {error}");
        assert!(!text.contains("fixture-") && !text.contains(&attempt().verifier));
    }
}

#[tokio::test]
async fn rejects_expired_token_and_oversized_response() {
    for response in [(200, token(0)), (200, "x".repeat(1024 * 1024 + 1))] {
        let fixture = Fixture::new(vec![response]).await;
        assert!(fixture
            .http
            .exchange("42", attempt(), "fixture-code".into())
            .await
            .is_err());
    }
}

#[tokio::test]
async fn guild_pagination_rejects_repeated_cursors() {
    let page = serde_json::Value::Array(
        (1..=200)
            .map(|id| serde_json::json!({"id":id.to_string(),"name":"Guild"}))
            .collect(),
    )
    .to_string();
    let fixture = Fixture::new(vec![
        (200, token(3600)),
        (200, authorization("42", &["identify", "guilds"])),
        (200, page.clone()),
        (200, page),
    ])
    .await;
    let error = fixture
        .http
        .exchange("42", attempt(), "fixture-code".into())
        .await
        .err()
        .unwrap();
    assert!(error.0.contains("paginación"));
}

#[tokio::test]
async fn guild_pagination_advances_after_a_full_page_and_stops_on_short_page() {
    let page = serde_json::Value::Array(
        (1..=200)
            .map(|id| serde_json::json!({"id":id.to_string(),"name":"Guild"}))
            .collect(),
    )
    .to_string();
    let mut fixture = Fixture::new(vec![
        (200, token(3600)),
        (200, authorization("42", &["identify", "guilds"])),
        (200, page),
        (200, r#"[{"id":"201","name":"Last"}]"#.into()),
    ])
    .await;
    let session = fixture
        .http
        .exchange("42", attempt(), "fixture-code".into())
        .await
        .unwrap();
    assert_eq!(session.snapshot.servers.len(), 201);
    for _ in 0..3 {
        fixture.requests.recv().await.unwrap();
    }
    let last = fixture.requests.recv().await.unwrap();
    assert!(last.starts_with("GET /guilds?limit=200&after=200 "));
}

async fn event(rx: &mut mpsc::Receiver<Envelope>) -> Event {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap()
        .event
}

struct Running {
    commands: mpsc::Sender<Command>,
    events: mpsc::Receiver<Envelope>,
    task: tokio::task::JoinHandle<()>,
}

impl Running {
    async fn start(http: Http) -> Self {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let provider = DiscordProvider {
            client_id: "42".into(),
            http,
            redirect: Url::parse(&format!("http://127.0.0.1:{port}/oauth/callback")).unwrap(),
        };
        let (commands, mut commands_rx) = mpsc::channel(16);
        let (tx, events) = mpsc::channel(64);
        let sink = EventSink::new(tx, Arc::new(Mutex::new(HashMap::new())), Arc::new(|| {}));
        let task = tokio::spawn(async move {
            provider.connect(&mut commands_rx, &sink).await.unwrap();
        });
        Self {
            commands,
            events,
            task,
        }
    }

    async fn begin(&mut self) -> Url {
        self.commands.send(Command::BeginSignIn).await.unwrap();
        match event(&mut self.events).await {
            Event::Authentication(AuthenticationState::AwaitingBrowser { url }) => {
                Url::parse(&url).unwrap()
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn deliver(url: &Url, override_state: Option<&str>) -> String {
    let pairs: HashMap<_, _> = url.query_pairs().collect();
    let mut redirect = Url::parse(&pairs["redirect_uri"]).unwrap();
    redirect
        .query_pairs_mut()
        .append_pair("state", override_state.unwrap_or(&pairs["state"]))
        .append_pair("code", "fixture-code");
    let mut stream = TcpStream::connect(("127.0.0.1", redirect.port().unwrap()))
        .await
        .unwrap();
    let request = format!(
        "GET {}?{} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
        redirect.path(),
        redirect.query().unwrap(),
        redirect.port().unwrap()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[tokio::test]
async fn full_local_sign_in_waits_for_action_then_signs_out_before_remote_revoke_failure() {
    let mut responses = successful(3600);
    responses.push((401, "fixture-access-secret".into()));
    let mut fixture = Fixture::new(responses).await;
    let mut running = Running::start(fixture.http.clone()).await;
    assert_eq!(
        event(&mut running.events).await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(20), running.events.recv())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(20), fixture.requests.recv())
            .await
            .is_err()
    );
    let url = running.begin().await;
    assert!(deliver(&url, Some("wrong-state"))
        .await
        .starts_with("HTTP/1.1 400"));
    assert!(deliver(&url, None).await.starts_with("HTTP/1.1 200"));
    assert_eq!(
        event(&mut running.events).await,
        Event::Authentication(AuthenticationState::Exchanging)
    );
    assert!(
        matches!(event(&mut running.events).await, Event::Ready(snapshot) if snapshot.me.is_some())
    );
    assert!(matches!(
        event(&mut running.events).await,
        Event::Connection(_)
    ));
    assert!(matches!(
        event(&mut running.events).await,
        Event::Authentication(AuthenticationState::SignedIn { .. })
    ));
    running.commands.send(Command::SignOut).await.unwrap();
    assert_eq!(
        event(&mut running.events).await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    assert!(matches!(
        event(&mut running.events).await,
        Event::Connection(_)
    ));
    assert!(
        matches!(event(&mut running.events).await, Event::Failed { reason, .. } if reason.contains("No se confirmó") && !reason.contains("fixture-"))
    );
}

#[tokio::test]
async fn cancelled_attempt_cannot_authenticate_the_next_attempt() {
    let fixture = Fixture::new(successful(3600)).await;
    let mut running = Running::start(fixture.http.clone()).await;
    event(&mut running.events).await;
    let old = running.begin().await;
    running.commands.send(Command::CancelSignIn).await.unwrap();
    assert_eq!(
        event(&mut running.events).await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    let current = running.begin().await;
    let old_pairs: HashMap<_, _> = old.query_pairs().collect();
    assert!(deliver(&current, Some(&old_pairs["state"]))
        .await
        .starts_with("HTTP/1.1 400"));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), running.events.recv())
            .await
            .is_err()
    );
    deliver(&current, None).await;
    assert_eq!(
        event(&mut running.events).await,
        Event::Authentication(AuthenticationState::Exchanging)
    );
}

#[tokio::test(start_paused = true)]
async fn slow_callback_does_not_hold_listener_beyond_authorization_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mut attempt = attempt();
    attempt.redirect.set_port(Some(addr.port())).unwrap();
    let task = tokio::spawn(attempt.receive(listener));
    let mut slow = TcpStream::connect(addr).await.unwrap();
    slow.write_all(b"GET /oauth/callback").await.unwrap();
    tokio::time::advance(Duration::from_secs(301)).await;
    let result = task.await.unwrap();
    assert!(result.err().unwrap().0.contains("agotó"));
}

#[tokio::test]
async fn authorization_rejects_past_expiry_and_bot_identity() {
    for bot in [false, true] {
        let mut auth: serde_json::Value =
            serde_json::from_str(&authorization("42", &["identify", "guilds"])).unwrap();
        if bot {
            auth["user"]["bot"] = true.into();
        } else {
            auth["expires"] = "2000-01-01T00:00:00Z".into();
        }
        let fixture = Fixture::new(vec![(200, token(3600)), (200, auth.to_string())]).await;
        assert!(fixture
            .http
            .exchange("42", attempt(), "fixture-code".into())
            .await
            .is_err());
    }
}

#[tokio::test]
async fn token_endpoint_redirect_never_reaches_another_server() {
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http = Http::local(&format!("http://{}", origin.local_addr().unwrap()));
    let location = format!("http://{}/stolen", trap.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut stream, _) = origin.accept().await.unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).await.unwrap() > 0);
        let response = format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    assert!(http
        .exchange("42", attempt(), "fixture-code".into())
        .await
        .is_err());
    task.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), trap.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cancellation_during_http_exchange_discards_late_result_and_releases_callback() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http = Http::local(&format!("http://{}", listener.local_addr().unwrap()));
    let (received, request_received) = tokio::sync::oneshot::channel();
    let (release, release_request) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).await.unwrap() > 0);
        received.send(()).unwrap();
        release_request.await.unwrap();
        let body = token(3600);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
    });
    let mut running = Running::start(http).await;
    event(&mut running.events).await;
    let url = running.begin().await;
    deliver(&url, None).await;
    assert_eq!(
        event(&mut running.events).await,
        Event::Authentication(AuthenticationState::Exchanging)
    );
    request_received.await.unwrap();
    running.commands.send(Command::CancelSignIn).await.unwrap();
    assert_eq!(
        event(&mut running.events).await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    release.send(()).unwrap();
    task.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), running.events.recv())
            .await
            .is_err()
    );
    let next = running.begin().await;
    let before: HashMap<_, _> = url.query_pairs().collect();
    let after: HashMap<_, _> = next.query_pairs().collect();
    assert_eq!(before["redirect_uri"], after["redirect_uri"]);
    assert_ne!(before["state"], after["state"]);
}

#[tokio::test]
async fn expired_session_forgets_identity_and_requests_sign_in_again() {
    let fixture = Fixture::new(successful(1)).await;
    let mut running = Running::start(fixture.http.clone()).await;
    event(&mut running.events).await;
    let url = running.begin().await;
    deliver(&url, None).await;
    for _ in 0..4 {
        event(&mut running.events).await;
    }
    assert_eq!(
        event(&mut running.events).await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    assert!(
        matches!(event(&mut running.events).await, Event::Connection(rune_provider::ConnectionState::Disconnected { reason }) if reason.contains("caducó"))
    );
}
