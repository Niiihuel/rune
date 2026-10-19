use std::sync::{Arc, Mutex};
use std::time::Duration;

use rune_model::UserId;
use rune_platform::secrets::{Entry, Saved, SecretError, SecretFuture, SecretStore};
use rune_provider::{
    AuthenticationState, BoxFuture, Command, ConnectionState, Envelope, Event, EventSink, Provider,
    ProviderError, ProviderKind,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use zeroize::Zeroizing;

use crate::{
    gateway::Gateway, http::Http, Challenge, ChallengeAnswer, DiscordSessionProvider, LoginError,
    SessionLogin, SessionSecret,
};

const SECRET: &str = "SYNTHETIC_SESSION_MARKER";
const USER: &str = r#"{"id":"42","username":"rune-test","global_name":"Rune Test","bot":false,"accent_color":66051}"#;

fn secret() -> SessionSecret {
    SessionSecret::try_from_local(SECRET.into()).unwrap()
}

fn response(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn request(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        assert!(bytes.len() < 8192, "fixture header limit");
        bytes.push(socket.read_u8().await.unwrap());
    }
    let headers = String::from_utf8(bytes).unwrap();
    let length: usize = headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap_or(0);
    assert!(length <= 16384, "fixture body limit");
    let mut body = vec![0; length];
    socket.read_exact(&mut body).await.unwrap();
    headers + &String::from_utf8(body).unwrap()
}

async fn fixture(reply: String) -> (Http, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (sent, received) = oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let headers = request(&mut socket).await;
        let _ = sent.send(headers);
        let _ = socket.write_all(reply.as_bytes()).await;
    });
    (Http::local(&base), received)
}

#[tokio::test]
async fn identity_uses_one_raw_session_header_and_the_browser_identity() {
    let (http, request) = fixture(response(200, USER)).await;
    let user = http.verify(&secret()).await.unwrap();
    assert_eq!(user.id, UserId(42));
    assert_eq!(user.name, "rune-test");
    assert_eq!(user.display_name.as_deref(), Some("Rune Test"));
    assert!(!user.bot);
    assert!(user.about.is_none());
    let request = request.await.unwrap();
    assert!(request.starts_with("GET /api/v10/users/@me HTTP/1.1\r\n"));
    let lower = request.to_ascii_lowercase();
    assert_eq!(lower.matches("\r\nauthorization:").count(), 1);
    assert!(request.contains(&format!("authorization: {SECRET}\r\n")));
    // Browser identity (2026-10-19): the web client in WebKit; a read names
    // no `Origin`, as a browser's would not.
    crate::browser::assert_presented(&request, SECRET);
    assert!(!lower.contains("\r\norigin:"));
}

/// windows-login: once a page in the windows' engine (WebView2) told what
/// it reads, REST, the Gateway and the windows present that one engine.
/// Presenting is process-wide, so it runs in a child of this test binary.
#[test]
fn a_learnt_engine_is_what_rest_the_gateway_and_the_windows_present() {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "tests::learnt_engine_child",
            "--ignored",
            "--test-threads=1",
        ])
        .env("RUNE_TEST_LEARNT_ENGINE", "1")
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

#[tokio::test]
#[ignore = "child of a_learnt_engine_is_what_rest_the_gateway_and_the_windows_present"]
async fn learnt_engine_child() {
    if std::env::var_os("RUNE_TEST_LEARNT_ENGINE").is_none() {
        return;
    }
    struct Probe;
    impl SessionLogin for Probe {
        fn acquire(&self) -> BoxFuture<'_, Result<SessionSecret, LoginError>> {
            Box::pin(async { Err(LoginError::new("unused")) })
        }

        fn engine(&self) -> BoxFuture<'_, Result<Option<crate::Engine>, LoginError>> {
            Box::pin(async {
                Ok(Some(crate::browser::tests::webview2(&[
                    "Chromium",
                    "Microsoft Edge",
                    "Not?A_Brand",
                ])))
            })
        }
    }
    let engine = crate::browser::tests::webview2(&[]);
    crate::browser::learn(&Probe).await.unwrap();
    // What rune-login hands its WebView2 windows.
    assert_eq!(crate::user_agent(), engine.user_agent);
    let (http, request) = fixture(response(200, USER)).await;
    http.verify(&secret()).await.unwrap();
    let request = request.await.unwrap();
    crate::browser::assert_presented(&request, SECRET);
    let lower = request.to_ascii_lowercase();
    assert!(lower.contains("\r\nsec-ch-ua-platform: \"windows\"\r\n"));
    assert!(lower.contains("\r\nx-discord-timezone: america/argentina/buenos_aires\r\n"));
    // The Gateway's handshake and Identify.
    let mut handshake = tokio_tungstenite::tungstenite::http::Request::new(());
    crate::browser::socket_headers(&mut handshake);
    assert_eq!(
        handshake.headers()["user-agent"],
        engine.user_agent.as_str()
    );
    let identify = serde_json::to_value(crate::browser::identify()).unwrap();
    assert_eq!(identify["browser_user_agent"], engine.user_agent);
    assert_eq!(identify["os"], "Windows");
    assert_eq!(identify["browser"], "Edge");
    assert_eq!(identify["browser_version"], "141.0.0.0");
}

#[tokio::test]
async fn malformed_identity_and_bot_accounts_are_rejected() {
    for body in [
        r#"{"id":"42","username":"bot","bot":true}"#.to_string(),
        r#"{"id":"0","username":"zero"}"#.into(),
        r#"{"id":"0042","username":"noncanonical"}"#.into(),
        r#"{"id":"not-an-id","username":"bad"}"#.into(),
        r#"{"id":"42","username":""}"#.into(),
        r#"{"id":"42"}"#.into(),
        r#"{"id":"42","username":"ok","global_name":17}"#.into(),
        format!(r#"{{"id":"42","username":"{}"}}"#, "x".repeat(513)),
    ] {
        let (http, _) = fixture(response(200, &body)).await;
        assert!(http.verify(&secret()).await.is_err());
    }
}

#[tokio::test]
async fn failures_do_not_reflect_remote_bodies_or_credentials() {
    for status in [401, 403, 429, 500] {
        let (http, _) = fixture(response(status, SECRET)).await;
        let error = http.verify(&secret()).await.unwrap_err();
        assert!(!format!("{error:?} {error}").contains(SECRET));
    }
    let (http, _) = fixture(response(200, SECRET)).await;
    let error = http.verify(&secret()).await.unwrap_err();
    assert!(!format!("{error:?} {error}").contains(SECRET));
}

#[tokio::test]
async fn redirects_never_forward_a_session_to_another_origin() {
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = format!("http://{}/trap", trap.local_addr().unwrap());
    let (http, _) = fixture(format!("HTTP/1.1 307 Redirect\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")).await;
    assert!(http.verify(&secret()).await.is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), trap.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn declared_and_streaming_oversized_responses_are_rejected() {
    for reply in [
        "HTTP/1.1 200 OK\r\nContent-Length: 1048577\r\nConnection: close\r\n\r\n".to_string(),
        format!(
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}",
            "x".repeat(1024 * 1024 + 1)
        ),
    ] {
        let (http, _) = fixture(reply).await;
        assert!(http.verify(&secret()).await.is_err());
    }
}

/// One entry of the fake keyring: service, account, shown name, avatar, secret.
type FakeEntry = (String, String, String, String, String);

/// An in-memory keyring, so no test reaches the person's own. Entries are
/// kept the most recently saved first, as the real one lists them.
#[derive(Default)]
struct Keyring {
    entries: Mutex<Vec<FakeEntry>>,
    unavailable: bool,
}

impl Keyring {
    /// `saved`: (account, secret), the most recently used first.
    fn with_all(saved: &[(&str, &str)]) -> Arc<Self> {
        let keyring = Self::default();
        keyring
            .entries
            .lock()
            .unwrap()
            .extend(saved.iter().map(|(account, secret)| {
                (
                    "discord-session".into(),
                    (*account).into(),
                    format!("Cuenta {account}"),
                    String::new(),
                    (*secret).into(),
                )
            }));
        Arc::new(keyring)
    }

    fn with(account: &str, secret: &str) -> Arc<Self> {
        Self::with_all(&[(account, secret)])
    }

    /// The accounts saved, the most recently used first.
    fn accounts(&self) -> Vec<String> {
        let entries = self.entries.lock().unwrap();
        entries
            .iter()
            .map(|(_, account, ..)| account.clone())
            .collect()
    }

    fn name(&self, account: &str) -> Option<String> {
        let entries = self.entries.lock().unwrap();
        entries
            .iter()
            .find(|(_, saved, ..)| saved == account)
            .map(|(_, _, name, ..)| name.clone())
    }

    fn avatar(&self, account: &str) -> Option<String> {
        let entries = self.entries.lock().unwrap();
        entries
            .iter()
            .find(|(_, saved, ..)| saved == account)
            .map(|(_, _, _, avatar, _)| avatar.clone())
    }

    fn check(&self) -> Result<(), SecretError> {
        if self.unavailable {
            Err(SecretError::UNAVAILABLE)
        } else {
            Ok(())
        }
    }
}

impl SecretStore for Keyring {
    fn list<'a>(&'a self, service: &'a str) -> SecretFuture<'a, Vec<Entry>> {
        Box::pin(async move {
            self.check()?;
            let entries = self.entries.lock().unwrap();
            Ok(entries
                .iter()
                .filter(|(owner, ..)| owner == service)
                .map(|(_, account, name, avatar, _)| Entry {
                    account: account.clone(),
                    name: name.clone(),
                    avatar: avatar.clone(),
                })
                .collect())
        })
    }

    fn load<'a>(&'a self, service: &'a str, account: &'a str) -> SecretFuture<'a, Option<Saved>> {
        Box::pin(async move {
            self.check()?;
            let entries = self.entries.lock().unwrap();
            Ok(entries
                .iter()
                .find(|(owner, saved, ..)| owner == service && saved == account)
                .map(|(_, account, _, _, secret)| Saved {
                    account: account.clone(),
                    secret: Zeroizing::new(secret.clone()),
                }))
        })
    }

    fn save<'a>(
        &'a self,
        service: &'a str,
        account: &'a str,
        name: &'a str,
        avatar: &'a str,
        secret: &'a str,
    ) -> SecretFuture<'a, ()> {
        Box::pin(async move {
            self.check()?;
            let mut entries = self.entries.lock().unwrap();
            entries.retain(|(owner, saved, ..)| owner != service || saved != account);
            entries.insert(
                0,
                (
                    service.into(),
                    account.into(),
                    name.into(),
                    avatar.into(),
                    secret.into(),
                ),
            );
            Ok(())
        })
    }

    fn delete<'a>(&'a self, service: &'a str, account: &'a str) -> SecretFuture<'a, ()> {
        Box::pin(async move {
            self.check()?;
            let mut entries = self.entries.lock().unwrap();
            entries.retain(|(owner, saved, ..)| owner != service || saved != account);
            Ok(())
        })
    }
}

type LoginReply = oneshot::Sender<Result<SessionSecret, LoginError>>;
/// A challenge shown in the fake window and how the person answers it.
type VerifyReply = (
    Challenge,
    oneshot::Sender<Result<ChallengeAnswer, LoginError>>,
);

struct ControlledLogin(
    mpsc::UnboundedSender<LoginReply>,
    mpsc::UnboundedSender<VerifyReply>,
);

impl SessionLogin for ControlledLogin {
    fn acquire(&self) -> BoxFuture<'_, Result<SessionSecret, LoginError>> {
        Box::pin(async move {
            let (sent, received) = oneshot::channel();
            self.0.send(sent).unwrap();
            received
                .await
                .map_err(|_| LoginError::new("Login cancelado."))?
        })
    }

    fn verify(&self, challenge: Challenge) -> BoxFuture<'_, Result<ChallengeAnswer, LoginError>> {
        Box::pin(async move {
            let (sent, received) = oneshot::channel();
            let closed = || LoginError::new("Verificación cancelada.");
            self.1.send((challenge, sent)).map_err(|_| closed())?;
            received.await.map_err(|_| closed())?
        })
    }
}

fn login() -> (Arc<dyn SessionLogin>, mpsc::UnboundedReceiver<LoginReply>) {
    let (login, attempts, _) = controlled();
    (login, attempts)
}

fn controlled() -> (
    Arc<dyn SessionLogin>,
    mpsc::UnboundedReceiver<LoginReply>,
    mpsc::UnboundedReceiver<VerifyReply>,
) {
    let (sent, received) = mpsc::unbounded_channel();
    let (shown, verifications) = mpsc::unbounded_channel();
    (
        Arc::new(ControlledLogin(sent, shown)),
        received,
        verifications,
    )
}

struct Running {
    commands: mpsc::Sender<Command>,
    events: mpsc::Receiver<Envelope>,
    task: tokio::task::JoinHandle<Result<(), ProviderError>>,
}

fn run(login: Arc<dyn SessionLogin>, http: Http) -> Running {
    run_with_gateway(login, http, Gateway::local("ws://127.0.0.1:9"))
}

fn run_with_gateway(login: Arc<dyn SessionLogin>, http: Http, gateway: Gateway) -> Running {
    run_with_store(login, http, gateway, Arc::new(Keyring::default()))
}

fn run_with_store(
    login: Arc<dyn SessionLogin>,
    http: Http,
    gateway: Gateway,
    secrets: Arc<dyn SecretStore>,
) -> Running {
    let (voice, voice_link) = rune_provider::voice::voice_link();
    let (video, video_link) = rune_provider::video::video_link();
    let provider = DiscordSessionProvider {
        login,
        secrets,
        http,
        gateway,
        voice,
        voice_link: std::sync::Mutex::new(Some(voice_link)),
        video,
        video_link: std::sync::Mutex::new(Some(video_link)),
        voice_local: None,
        read_versions: Default::default(),
    };
    let (commands, mut receive) = mpsc::channel(16);
    let (sent, events) = mpsc::channel(16);
    let sink = EventSink::new(sent, Arc::new(Mutex::default()), Arc::new(|| {}));
    let task = tokio::spawn(async move { provider.connect(&mut receive, &sink).await });
    Running {
        commands,
        events,
        task,
    }
}

impl Running {
    /// The next event, other than the keyring's status, the account's
    /// upload limits and what the account keeps with Discord, read after
    /// each READY (see `next_any`).
    async fn next(&mut self) -> Event {
        loop {
            let event = self.next_any().await;
            if !matches!(
                event,
                Event::SessionRemembered(_)
                    | Event::RememberedAccounts(_)
                    // The local store's, checked in `keyring`.
                    | Event::StorageIdentity(_)
                    | Event::ForgetStorage(_)
                    | Event::UploadLimit { .. }
                    | Event::ServerFolders(_)
                    | Event::MessagingPrivacy(_)
                    | Event::ActivitySharing(_)
                    | Event::NotificationSettings { .. }
                    | Event::PresenceChanged {
                        user: UserId(42),
                        ..
                    }
            ) {
                return event;
            }
        }
    }

    async fn next_any(&mut self) -> Event {
        // Generous: on Windows a refused loopback connection takes ~2 s.
        tokio::time::timeout(Duration::from_secs(10), self.events.recv())
            .await
            .expect("expected provider event")
            .unwrap()
            .event
    }

    async fn stop(self) {
        drop(self.commands);
        self.task.await.unwrap().unwrap();
    }
}

#[test]
fn provider_declares_only_implemented_communication_capabilities() {
    let (login, _) = login();
    let info = DiscordSessionProvider::new(login).unwrap().info();
    assert_eq!(info.kind, ProviderKind::Unofficial);
    assert!(info.capabilities.authentication);
    assert!(info.capabilities.account_servers && info.capabilities.direct_messages);
    assert!(info.capabilities.history && info.capabilities.send_messages);
    assert!(info.capabilities.message_context);
    assert!(info.capabilities.edit_messages && info.capabilities.delete_messages);
    // Each one has its own fixture path; none was tried against the service.
    assert!(info.capabilities.reactions && info.capabilities.typing && info.capabilities.pins);
    // Threads and posts by Serein's routes: fixtures only, never tried live.
    assert!(info.capabilities.thread_replies && info.capabilities.create_threads);
    assert!(info.capabilities.thread_pages && info.capabilities.manage_threads);
    assert!(info.capabilities.follow_threads);
    // Server voice with DAVE: fixtures only, never tried against the service.
    assert_eq!(info.capabilities.voice, rune_provider::VoiceSupport::Real);
    assert!(info.capabilities.video, "the camera is sent");
    // The screen as Go Live: fixtures only, never tried against the service.
    assert!(info.capabilities.share_screen);
    // Relationships and conversations outside servers, by Serein's routes:
    // fixtures only, never tried against the service.
    let capabilities = info.capabilities;
    assert!(capabilities.relationships && capabilities.friend_requests);
    assert!(capabilities.block_users && capabilities.friend_nicknames);
    assert!(capabilities.open_direct && capabilities.close_direct);
    assert!(capabilities.manage_groups && capabilities.friend_presence);
    // Making groups, their people and owner by the official web client's
    // routes (REA): fixtures only.
    assert!(capabilities.create_groups && capabilities.group_members);
    assert!(capabilities.group_owner);
    // Full profile and member list: fixtures only, never tried against the service.
    assert!(info.capabilities.profiles && info.capabilities.member_list);
    // The account's own status, custom status and synced settings (P1):
    // fixtures only.
    assert!(info.capabilities.presence && info.capabilities.custom_status);
    assert!(capabilities.synced_notifications && capabilities.server_folders);
    assert!(capabilities.messaging_privacy && capabilities.notes);
    // Apps' parts, modals and commands (P3), as Serein and the official web
    // client send them: fixtures only.
    assert!(capabilities.interactions && capabilities.app_commands);
    // The account's profile, its pictures and its profile in each server, by
    // the routes Serein and the official web client use: fixtures only.
    assert!(capabilities.edit_profile && capabilities.profile_images);
    assert!(capabilities.server_profiles);
    // Files by the route Serein uses: fixtures only, never tried for real.
    assert!(info.capabilities.attachments);
    // Server administration by Serein's and Discord's documented routes:
    // fixtures only, never tried against the service.
    assert!(capabilities.manage_servers && capabilities.manage_channels);
    assert!(capabilities.moderation && capabilities.timeouts && capabilities.audit_log);
    assert!(capabilities.manage_invites && capabilities.create_invites);
    assert_eq!(capabilities.invite_links, Some("https://discord.gg/"));
    assert!(capabilities.manage_expressions && capabilities.upload_expressions);
    assert!(capabilities.member_pages && capabilities.prune);
    assert!(capabilities.webhooks && capabilities.integrations);
    assert!(capabilities.synced_gif_favorites);
    // Server administration, rest (P7): the same, fixtures only.
    assert!(capabilities.server_identity && capabilities.server_safety);
    assert!(capabilities.pause_invites && capabilities.reorder_channels);
    assert!(capabilities.forum_settings && capabilities.role_styles);
    assert!(capabilities.role_counts && capabilities.member_queries);
}

#[tokio::test]
async fn explicit_login_verifies_identity_and_logout_only_clears_local_session() {
    let (login, mut attempts) = login();
    let (http, request) = fixture(response(200, USER)).await;
    let mut running = run(login, http);
    assert_eq!(
        running.next().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    assert!(attempts.try_recv().is_err());
    running.commands.send(Command::BeginSignIn).await.unwrap();
    assert_eq!(
        running.next().await,
        Event::Authentication(AuthenticationState::AwaitingExternal)
    );
    attempts.recv().await.unwrap().send(Ok(secret())).unwrap();
    assert_eq!(
        running.next().await,
        Event::Authentication(AuthenticationState::Exchanging)
    );
    let Event::Ready(snapshot) = running.next().await else {
        panic!("identity snapshot");
    };
    assert_eq!(snapshot.me.as_ref().unwrap().id, UserId(42));
    assert_eq!(snapshot.users, vec![snapshot.me.clone().unwrap()]);
    assert!(
        snapshot.servers.is_empty()
            && snapshot.channels.is_empty()
            && snapshot.presences.is_empty()
    );
    assert_eq!(
        running.next().await,
        Event::Authentication(AuthenticationState::SignedIn { scopes: vec![] })
    );
    assert_eq!(
        running.next().await,
        Event::Connection(ConnectionState::Connecting)
    );
    request.await.unwrap();
    running.commands.send(Command::SignOut).await.unwrap();
    assert_eq!(
        running.next().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    let Event::Connection(ConnectionState::Disconnected { reason }) = running.next().await else {
        panic!("local logout");
    };
    assert!(reason.contains("equipo"));
    assert!(!reason.contains("revocada"));
    running.stop().await;
}

#[tokio::test]
async fn replacing_and_cancelling_login_discards_old_credentials() {
    let (login, mut attempts) = login();
    let mut running = run(login, Http::local("http://127.0.0.1:9"));
    running.next().await;
    running.commands.send(Command::BeginSignIn).await.unwrap();
    running.next().await;
    let old = attempts.recv().await.unwrap();
    running.commands.send(Command::BeginSignIn).await.unwrap();
    assert_eq!(
        running.next().await,
        Event::Authentication(AuthenticationState::AwaitingExternal)
    );
    let fresh = attempts.recv().await.unwrap();
    assert!(old.is_closed());
    running.commands.send(Command::CancelSignIn).await.unwrap();
    assert_eq!(
        running.next().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    assert!(fresh.is_closed());
    running.stop().await;
}

#[tokio::test]
async fn cancellation_wins_when_login_result_and_command_are_both_ready() {
    let (login, mut attempts) = login();
    let mut running = run(login, Http::local("http://127.0.0.1:9"));
    running.next().await;
    running.commands.send(Command::BeginSignIn).await.unwrap();
    running.next().await;
    let reply = attempts.recv().await.unwrap();
    reply.send(Ok(secret())).unwrap();
    running.commands.try_send(Command::CancelSignIn).unwrap();
    assert_eq!(
        running.next().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    assert!(running.events.try_recv().is_err());
    running.stop().await;
}

#[tokio::test]
async fn logout_during_http_verification_discards_a_late_identity() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http = Http::local(&format!("http://{}", listener.local_addr().unwrap()));
    let (arrived, waiting) = oneshot::channel();
    let (release, delayed) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        let _ = arrived.send(());
        delayed.await.unwrap();
        let _ = socket.write_all(response(200, USER).as_bytes()).await;
    });
    let (login, mut attempts) = login();
    let mut running = run(login, http);
    running.next().await;
    running.commands.send(Command::BeginSignIn).await.unwrap();
    running.next().await;
    attempts.recv().await.unwrap().send(Ok(secret())).unwrap();
    assert_eq!(
        running.next().await,
        Event::Authentication(AuthenticationState::Exchanging)
    );
    waiting.await.unwrap();
    running.commands.send(Command::SignOut).await.unwrap();
    assert_eq!(
        running.next().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    assert!(matches!(
        running.next().await,
        Event::Connection(ConnectionState::Disconnected { .. })
    ));
    release.send(()).unwrap();
    server.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), running.events.recv())
            .await
            .is_err()
    );
    running.stop().await;
}

#[tokio::test(start_paused = true)]
async fn login_has_a_deadline_and_drops_the_host_future_on_expiry() {
    let (login, mut attempts) = login();
    let mut running = run(login, Http::local("http://127.0.0.1:9"));
    running.next().await;
    running.commands.send(Command::BeginSignIn).await.unwrap();
    running.next().await;
    let pending = attempts.recv().await.unwrap();
    tokio::time::advance(Duration::from_secs(601)).await;
    let Event::Authentication(AuthenticationState::Failed { reason }) = running.next().await else {
        panic!("expired login");
    };
    assert!(reason.contains("venció"));
    assert!(pending.is_closed());
    assert!(attempts.try_recv().is_err());
    running.stop().await;
}

#[tokio::test]
async fn dropping_the_provider_connection_cancels_the_host_login() {
    let (login, mut attempts) = login();
    let mut running = run(login, Http::local("http://127.0.0.1:9"));
    running.next().await;
    running.commands.send(Command::BeginSignIn).await.unwrap();
    running.next().await;
    let pending = attempts.recv().await.unwrap();
    running.task.abort();
    assert!(running.task.await.unwrap_err().is_cancelled());
    assert!(pending.is_closed());
}

#[tokio::test]
async fn rejected_identity_stays_signed_out_without_automatic_retry() {
    let (login, mut attempts) = login();
    let (http, request) = fixture(response(401, SECRET)).await;
    let mut running = run(login, http);
    running.next().await;
    running.commands.send(Command::BeginSignIn).await.unwrap();
    running.next().await;
    attempts.recv().await.unwrap().send(Ok(secret())).unwrap();
    running.next().await;
    let Event::Authentication(AuthenticationState::Failed { reason }) = running.next().await else {
        panic!("identity rejection");
    };
    assert!(!reason.contains(SECRET));
    request.await.unwrap();
    assert!(attempts.try_recv().is_err());
    assert!(running.events.try_recv().is_err());
    running.stop().await;
}

#[tokio::test(start_paused = true)]
async fn identity_http_response_has_a_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http = Http::local(&format!("http://{}", listener.local_addr().unwrap()));
    let verifying = tokio::spawn(async move { http.verify(&secret()).await });
    let (mut socket, _) = listener.accept().await.unwrap();
    request(&mut socket).await;
    tokio::time::advance(Duration::from_secs(16)).await;
    let error = verifying.await.unwrap().unwrap_err();
    assert!(!format!("{error:?} {error}").contains(SECRET));
}

mod invitations;
mod keyring;
mod profile_edit;
mod rest;
mod session;
mod social;
mod stage;
mod threads;
