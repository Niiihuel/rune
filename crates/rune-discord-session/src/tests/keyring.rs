//! Remembering sessions in a keyring, always the in-memory fake.

use rune_provider::{Failure, RememberedAccount};

use super::*;

const OTHER_SECRET: &str = "SYNTHETIC_OTHER_SESSION_MARKER";
const OTHER_USER: &str = r#"{"id":"7","username":"otra","global_name":"Otra Cuenta","bot":false}"#;

fn start(keyring: Arc<Keyring>, http: Http) -> (Running, mpsc::UnboundedReceiver<LoginReply>) {
    let (login, attempts) = login();
    let running = run_with_store(login, http, Gateway::local("ws://127.0.0.1:9"), keyring);
    (running, attempts)
}

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

fn listed(accounts: &[(u64, &str)]) -> Event {
    Event::RememberedAccounts(
        accounts
            .iter()
            .map(|(id, name)| RememberedAccount {
                id: UserId(*id),
                name: (*name).into(),
                // None of their own: Discord's default for a new-style name.
                avatar: Some(crate::wire::default_avatar(
                    *id,
                    &serde_json::Value::Null,
                    false,
                )),
            })
            .collect(),
    )
}

/// Every event until `last`, which must come; checked for both credentials.
async fn until(running: &mut Running, last: &Event) -> Vec<Event> {
    let mut seen = Vec::new();
    loop {
        let event = running.next_any().await;
        let shown = format!("{event:?}");
        assert!(!shown.contains(SECRET) && !shown.contains(OTHER_SECRET));
        let done = &event == last;
        seen.push(event);
        if done {
            return seen;
        }
    }
}

/// `first` comes before `then` in `seen`.
fn before(seen: &[Event], first: &Event, then: &Event) -> bool {
    let at = |wanted: &Event| seen.iter().position(|event| event == wanted);
    matches!((at(first), at(then)), (Some(first), Some(then)) if first < then)
}

async fn sign_in(running: &mut Running, attempts: &mut mpsc::UnboundedReceiver<LoginReply>) {
    running.commands.send(Command::BeginSignIn).await.unwrap();
    attempts.recv().await.unwrap().send(Ok(secret())).unwrap();
    until(running, &Event::Connection(ConnectionState::Connecting)).await;
}

fn signed_in() -> Event {
    Event::Authentication(AuthenticationState::SignedIn { scopes: vec![] })
}

/// The account the local store may keep, as the provider names it.
fn stored(account: u64) -> rune_provider::StorageIdentity {
    rune_provider::StorageIdentity {
        provider: crate::provider::STORAGE,
        account: UserId(account),
    }
}

#[tokio::test]
async fn a_saved_session_is_verified_and_restored_without_the_login_window() {
    let keyring = Keyring::with("42", SECRET);
    let (http, request) = fixture(response(200, USER)).await;
    let (mut running, attempts) = start(keyring.clone(), http);
    // The list, then never SignedOut first: the interface would open the login window.
    assert_eq!(running.next_any().await, listed(&[(42, "Cuenta 42")]));
    assert_eq!(
        running.next_any().await,
        Event::Authentication(AuthenticationState::Exchanging)
    );
    let seen = until(&mut running, &Event::SessionRemembered(true)).await;
    assert!(seen.contains(&signed_in()));
    assert!(!seen.contains(&Event::Authentication(AuthenticationState::SignedOut)));
    // Verified, it is named for the local store, and no bare snapshot
    // replaces what this device kept of it before READY.
    assert!(before(
        &seen,
        &Event::StorageIdentity(stored(42)),
        &signed_in()
    ));
    assert!(!seen.iter().any(|event| matches!(event, Event::Ready(_))));
    assert!(request
        .await
        .unwrap()
        .contains(&format!("authorization: {SECRET}\r\n")));
    // Saved again: the entry now carries the verified name.
    assert_eq!(keyring.name("42").as_deref(), Some("Rune Test"));
    let mut attempts = attempts;
    assert!(attempts.try_recv().is_err());
    // Signing out deletes the entry before announcing it, and forgets what
    // the local store keeps of the account.
    running.commands.send(Command::SignOut).await.unwrap();
    let seen = until(
        &mut running,
        &Event::Authentication(AuthenticationState::SignedOut),
    )
    .await;
    assert!(seen.contains(&listed(&[])));
    assert!(seen.contains(&Event::ForgetStorage(stored(42))));
    assert!(keyring.accounts().is_empty());
    running.stop().await;
}

#[tokio::test]
async fn a_new_sign_in_names_its_account_for_the_store_after_its_first_snapshot() {
    let keyring = Arc::new(Keyring::default());
    let (http, _) = fixture(response(200, USER)).await;
    let (mut running, mut attempts) = start(keyring.clone(), http);
    running.commands.send(Command::BeginSignIn).await.unwrap();
    attempts.recv().await.unwrap().send(Ok(secret())).unwrap();
    let seen = until(
        &mut running,
        &Event::Connection(ConnectionState::Connecting),
    )
    .await;
    let ready = seen
        .iter()
        .find(|event| matches!(event, Event::Ready(snapshot) if snapshot.me.as_ref().is_some_and(|me| me.id == UserId(42))))
        .cloned()
        .expect("the account's own snapshot first");
    assert!(before(&seen, &ready, &Event::StorageIdentity(stored(42))));
    assert!(before(
        &seen,
        &Event::StorageIdentity(stored(42)),
        &signed_in()
    ));
    running.stop().await;
}

#[tokio::test]
async fn several_saved_accounts_restore_only_the_last_used() {
    let keyring = Keyring::with_all(&[("42", SECRET), ("7", OTHER_SECRET)]);
    let (http, mut requests) = fixtures(vec![response(200, USER)]).await;
    let (mut running, _attempts) = start(keyring.clone(), http);
    assert_eq!(
        running.next_any().await,
        listed(&[(42, "Cuenta 42"), (7, "Cuenta 7")])
    );
    until(&mut running, &Event::SessionRemembered(true)).await;
    // One identity request, with the last used account's credential.
    let only = requests.recv().await.unwrap();
    assert!(only.contains(&format!("authorization: {SECRET}\r\n")));
    assert!(requests.try_recv().is_err());
    assert_eq!(keyring.accounts(), ["42", "7"]);
    running.stop().await;
}

#[tokio::test]
async fn switching_ends_the_session_keeps_both_and_restores_the_other() {
    let keyring = Keyring::with_all(&[("42", SECRET), ("7", OTHER_SECRET)]);
    let (http, mut requests) = fixtures(vec![response(200, USER), response(200, OTHER_USER)]).await;
    let (mut running, mut attempts) = start(keyring.clone(), http);
    until(&mut running, &Event::SessionRemembered(true)).await;
    running
        .commands
        .send(Command::UseAccount(Some(UserId(7))))
        .await
        .unwrap();
    let seen = until(&mut running, &Event::SessionRemembered(true)).await;
    // The same account boundary as a sign-out, then the chosen one.
    let signed_out = Event::Authentication(AuthenticationState::SignedOut);
    assert!(before(
        &seen,
        &signed_out,
        &Event::Authentication(AuthenticationState::Exchanging)
    ));
    assert!(before(&seen, &signed_out, &signed_in()));
    // The other is named for the local store; switching forgets nothing.
    assert!(before(
        &seen,
        &signed_out,
        &Event::StorageIdentity(stored(7))
    ));
    assert!(!seen
        .iter()
        .any(|event| matches!(event, Event::ForgetStorage(_))));
    requests.recv().await.unwrap();
    assert!(requests
        .recv()
        .await
        .unwrap()
        .contains(&format!("authorization: {OTHER_SECRET}\r\n")));
    assert!(attempts.try_recv().is_err());
    // Both stay remembered; the other is now the last used.
    assert_eq!(keyring.accounts(), ["7", "42"]);
    running.stop().await;
}

#[tokio::test]
async fn adding_an_account_keeps_the_open_one_and_opens_the_login() {
    let keyring = Keyring::with("42", SECRET);
    let (http, _) = fixture(response(200, USER)).await;
    let (mut running, mut attempts) = start(keyring.clone(), http);
    until(&mut running, &Event::SessionRemembered(true)).await;
    running
        .commands
        .send(Command::UseAccount(None))
        .await
        .unwrap();
    let seen = until(
        &mut running,
        &Event::Authentication(AuthenticationState::AwaitingExternal),
    )
    .await;
    assert!(seen.contains(&Event::Authentication(AuthenticationState::SignedOut)));
    assert!(attempts.recv().await.is_some());
    assert_eq!(keyring.accounts(), ["42"]);
    running.stop().await;
}

#[tokio::test]
async fn an_account_whose_entry_is_gone_says_so_and_stays_signed_out() {
    let keyring = Keyring::with("42", SECRET);
    let (http, _) = fixture(response(200, USER)).await;
    let (mut running, mut attempts) = start(keyring.clone(), http);
    until(&mut running, &Event::SessionRemembered(true)).await;
    running
        .commands
        .send(Command::UseAccount(Some(UserId(9))))
        .await
        .unwrap();
    let seen = until(
        &mut running,
        &Event::Authentication(AuthenticationState::SignedOut),
    )
    .await;
    assert!(!seen.contains(&Event::Authentication(AuthenticationState::Exchanging)));
    let reason = loop {
        if let Event::Connection(ConnectionState::Disconnected { reason }) =
            running.next_any().await
        {
            if reason.contains("No se encontró") {
                break reason;
            }
        }
    };
    assert!(reason.contains("Usar otra cuenta"));
    assert!(attempts.try_recv().is_err());
    assert_eq!(keyring.accounts(), ["42"]);
    running.stop().await;
}

#[tokio::test]
async fn forgetting_one_account_keeps_the_others_and_the_open_session() {
    let keyring = Keyring::with_all(&[("42", SECRET), ("7", OTHER_SECRET)]);
    let (http, _) = fixture(response(200, USER)).await;
    let (mut running, _attempts) = start(keyring.clone(), http);
    until(&mut running, &Event::SessionRemembered(true)).await;
    running
        .commands
        .send(Command::ForgetAccount(UserId(7)))
        .await
        .unwrap();
    let seen = until(&mut running, &listed(&[(42, "Rune Test")])).await;
    assert!(!seen.contains(&Event::SessionRemembered(false)));
    // What the local store keeps of it goes with it.
    assert!(seen.contains(&Event::ForgetStorage(stored(7))));
    assert_eq!(keyring.accounts(), ["42"]);
    // The open one: it stays open, no longer remembered, its data in memory.
    running
        .commands
        .send(Command::ForgetAccount(UserId(42)))
        .await
        .unwrap();
    let seen = until(&mut running, &listed(&[])).await;
    assert!(seen.contains(&Event::SessionRemembered(false)));
    assert!(seen.contains(&Event::ForgetStorage(stored(42))));
    assert!(!seen.contains(&Event::Authentication(AuthenticationState::SignedOut)));
    assert!(keyring.accounts().is_empty());
    running.stop().await;
}

#[tokio::test]
async fn a_revoked_saved_session_is_deleted_and_falls_back_to_the_rest() {
    let keyring = Keyring::with_all(&[("42", SECRET), ("7", OTHER_SECRET)]);
    let (http, mut requests) = fixtures(vec![response(401, SECRET)]).await;
    let (mut running, mut attempts) = start(keyring.clone(), http);
    running.next_any().await;
    assert_eq!(
        running.next_any().await,
        Event::Authentication(AuthenticationState::Exchanging)
    );
    // What is left comes before SignedOut, so the interface lists it.
    assert_eq!(running.next_any().await, listed(&[(7, "Cuenta 7")]));
    assert_eq!(
        running.next_any().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    let Event::Connection(ConnectionState::Disconnected { reason }) = running.next_any().await
    else {
        panic!("why the saved session ended");
    };
    assert!(reason.contains("guardada") && !reason.contains(SECRET));
    requests.recv().await.unwrap();
    assert_eq!(keyring.accounts(), ["7"]);
    // The other account is never checked on its own.
    assert!(requests.try_recv().is_err());
    assert!(attempts.try_recv().is_err());
    assert!(running.events.try_recv().is_err());
    running.stop().await;
}

#[tokio::test]
async fn a_revoked_only_session_is_deleted_and_falls_back_to_sign_in() {
    let keyring = Keyring::with("42", SECRET);
    let (http, request) = fixture(response(401, SECRET)).await;
    let (mut running, mut attempts) = start(keyring.clone(), http);
    let seen = until(
        &mut running,
        &Event::Authentication(AuthenticationState::SignedOut),
    )
    .await;
    assert!(seen.contains(&listed(&[])));
    request.await.unwrap();
    assert!(keyring.accounts().is_empty());
    // No automatic retry; the interface opens the login once on SignedOut.
    assert!(attempts.try_recv().is_err());
    running.stop().await;
}

#[tokio::test]
async fn a_saved_session_that_cannot_be_checked_now_is_kept_and_retried_on_request() {
    let keyring = Keyring::with("42", SECRET);
    let (http, _) = fixture(response(500, "")).await;
    let (mut running, mut attempts) = start(keyring.clone(), http);
    running.next_any().await;
    running.next_any().await;
    assert!(matches!(
        running.next_any().await,
        Event::Authentication(AuthenticationState::Failed { .. })
    ));
    assert_eq!(keyring.accounts(), ["42"]);
    // "Volver a intentar" checks the saved session again, not the login window.
    running.commands.send(Command::BeginSignIn).await.unwrap();
    assert_eq!(
        running.next_any().await,
        Event::Authentication(AuthenticationState::Exchanging)
    );
    assert!(matches!(
        running.next_any().await,
        Event::Authentication(AuthenticationState::Failed { .. })
    ));
    assert!(attempts.try_recv().is_err());
    assert_eq!(keyring.accounts(), ["42"]);
    running.stop().await;
}

#[tokio::test]
async fn a_new_login_is_saved_forgotten_on_request_and_deleted_on_sign_out() {
    let keyring = Arc::new(Keyring::default());
    let (http, _) = fixture(response(200, USER)).await;
    let (mut running, mut attempts) = start(keyring.clone(), http);
    assert_eq!(running.next_any().await, listed(&[]));
    assert_eq!(
        running.next_any().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    sign_in(&mut running, &mut attempts).await;
    until(&mut running, &Event::SessionRemembered(true)).await;
    // The list follows the status, from the same keyring operation.
    assert_eq!(running.next_any().await, listed(&[(42, "Rune Test")]));
    assert_eq!(keyring.accounts(), ["42"]);
    // "Olvidar esta sesión": the session stays open.
    running.commands.send(Command::ForgetSession).await.unwrap();
    until(&mut running, &Event::SessionRemembered(false)).await;
    assert!(keyring.accounts().is_empty());
    running.stop().await;

    let keyring = Arc::new(Keyring::default());
    let (http, _) = fixture(response(200, USER)).await;
    let (mut running, mut attempts) = start(keyring.clone(), http);
    running.next_any().await;
    running.next_any().await;
    sign_in(&mut running, &mut attempts).await;
    // Signing out right away still waits for the save, then deletes it.
    running.commands.send(Command::SignOut).await.unwrap();
    let seen = until(
        &mut running,
        &Event::Authentication(AuthenticationState::SignedOut),
    )
    .await;
    assert!(seen
        .iter()
        .all(|event| !matches!(event, Event::Failed { .. })));
    assert!(keyring.accounts().is_empty());
    running.stop().await;
}

#[tokio::test]
async fn without_a_keyring_the_session_stays_in_memory_with_a_notice() {
    let keyring = Arc::new(Keyring {
        unavailable: true,
        ..Keyring::default()
    });
    let (http, _) = fixture(response(200, USER)).await;
    let (mut running, mut attempts) = start(keyring, http);
    // No list without a keyring.
    assert_eq!(
        running.next_any().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    sign_in(&mut running, &mut attempts).await;
    // Gateway reconnection events from the closed fixture port may interleave.
    until(&mut running, &Event::SessionRemembered(false)).await;
    let reason = loop {
        if let Event::Failed {
            failure: Failure::Other,
            reason,
            ambiguous: false,
        } = running.next_any().await
        {
            break reason;
        }
    };
    assert!(reason.contains("llavero") && !reason.contains(SECRET));
    // The session is still open; signing out has nothing to delete.
    running.commands.send(Command::SignOut).await.unwrap();
    until(
        &mut running,
        &Event::Authentication(AuthenticationState::SignedOut),
    )
    .await;
    assert!(matches!(
        running.next_any().await,
        Event::Connection(ConnectionState::Disconnected { .. })
    ));
    assert!(running.events.try_recv().is_err());
    running.stop().await;
}

#[tokio::test]
async fn the_picture_is_kept_beside_the_name_and_listed_with_the_account() {
    let hash = "0123456789abcdef0123456789abcdef";
    let user = format!(
        r#"{{"id":"42","username":"rune-test","global_name":"Rune Test","bot":false,"avatar":"{hash}"}}"#
    );
    let keyring = Keyring::with("42", SECRET);
    let (http, _request) = fixture(response(200, &user)).await;
    let (mut running, _attempts) = start(keyring.clone(), http);
    let listed = Event::RememberedAccounts(vec![RememberedAccount {
        id: UserId(42),
        name: "Rune Test".into(),
        avatar: Some(rune_model::RemoteImage {
            url: format!("https://cdn.discordapp.com/avatars/42/{hash}.png?size=128"),
            kind: rune_model::ImageKind::Artwork,
        }),
    }]);
    until(&mut running, &listed).await;
    // Only the public key of the picture: never the session, never a URL.
    assert_eq!(keyring.avatar("42").as_deref(), Some(hash));
    running.stop().await;
}

/// An HTTP server that takes the identity check and never answers it: the
/// saved session stays being checked.
async fn silent() -> (Http, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http = Http::local(&format!("http://{}", listener.local_addr().unwrap()));
    let held = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    (http, held)
}

#[tokio::test]
async fn signing_out_while_the_saved_session_is_checked_forgets_it() {
    let keyring = Keyring::with_all(&[("42", SECRET), ("7", OTHER_SECRET)]);
    let (http, held) = silent().await;
    let (mut running, mut attempts) = start(keyring.clone(), http);
    until(
        &mut running,
        &Event::Authentication(AuthenticationState::Exchanging),
    )
    .await;
    running.commands.send(Command::SignOut).await.unwrap();
    let seen = until(
        &mut running,
        &Event::Authentication(AuthenticationState::SignedOut),
    )
    .await;
    // Deleted before SignedOut; the other account stays remembered.
    assert!(before(
        &seen,
        &listed(&[(7, "Cuenta 7")]),
        &Event::Authentication(AuthenticationState::SignedOut)
    ));
    assert_eq!(keyring.accounts(), ["7"]);
    // Its local data goes with it.
    assert!(seen.contains(&Event::ForgetStorage(stored(42))));
    assert!(attempts.try_recv().is_err());
    held.abort();
    running.stop().await;
}

#[tokio::test]
async fn switching_while_the_saved_session_is_checked_keeps_it() {
    let keyring = Keyring::with("42", SECRET);
    let (http, held) = silent().await;
    let (mut running, mut attempts) = start(keyring.clone(), http);
    until(
        &mut running,
        &Event::Authentication(AuthenticationState::Exchanging),
    )
    .await;
    running
        .commands
        .send(Command::UseAccount(None))
        .await
        .unwrap();
    until(
        &mut running,
        &Event::Authentication(AuthenticationState::AwaitingExternal),
    )
    .await;
    assert!(attempts.recv().await.is_some());
    assert_eq!(keyring.accounts(), ["42"]);
    // Neither closing that login nor signing out then forgets it: nothing
    // of that account is being checked any more.
    running.commands.send(Command::CancelSignIn).await.unwrap();
    until(
        &mut running,
        &Event::Authentication(AuthenticationState::SignedOut),
    )
    .await;
    running.commands.send(Command::SignOut).await.unwrap();
    until(
        &mut running,
        &Event::Authentication(AuthenticationState::SignedOut),
    )
    .await;
    assert_eq!(keyring.accounts(), ["42"]);
    held.abort();
    running.stop().await;
}

#[tokio::test]
async fn signing_out_after_the_saved_session_could_not_be_checked_forgets_it() {
    let keyring = Keyring::with("42", SECRET);
    let (http, _) = fixture(response(500, "")).await;
    let (mut running, _attempts) = start(keyring.clone(), http);
    while !matches!(
        running.next_any().await,
        Event::Authentication(AuthenticationState::Failed { .. })
    ) {}
    assert_eq!(keyring.accounts(), ["42"]);
    running.commands.send(Command::SignOut).await.unwrap();
    let seen = until(
        &mut running,
        &Event::Authentication(AuthenticationState::SignedOut),
    )
    .await;
    assert!(seen.contains(&listed(&[])));
    assert!(keyring.accounts().is_empty());
    assert!(seen.contains(&Event::ForgetStorage(stored(42))));
    running.stop().await;
}
