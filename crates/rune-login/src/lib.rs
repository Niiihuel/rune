//! Temporary, owner-operated authentication in an isolated helper process.
//!
//! This is an unofficial session handoff, not OAuth2. Only this newly opened
//! window is observed. The helper has no access to another browser profile.
//! A second helper shows one hCaptcha challenge of an explicit action on
//! RUNE's own page, for the person to solve (owner-authorized 2026-10-16).
//! The engine is WebKitGTK on Linux and WebView2 on Windows (windows-login),
//! where a third helper reads what a page in it sees of its browser, so the
//! adapter presents that same browser.

use std::sync::Mutex;
#[cfg(any(
    all(test, unix),
    all(feature = "webview", any(target_os = "linux", windows))
))]
use std::{process::Stdio, time::Duration};

use rune_discord_session::{
    Challenge, ChallengeAnswer, Engine, LoginError, SessionLogin, SessionSecret,
};
use rune_i18n::{es, Language};
use rune_provider::BoxFuture;
#[cfg(any(
    all(test, unix),
    all(feature = "webview", any(target_os = "linux", windows))
))]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(any(
    all(test, unix),
    all(feature = "webview", any(target_os = "linux", windows))
))]
use zeroize::Zeroizing;

// Built without the window too: `cancelled` compares its static labels.
#[cfg_attr(
    not(all(feature = "webview", any(target_os = "linux", windows))),
    allow(dead_code)
)]
mod protocol;
#[cfg(all(feature = "webview", any(target_os = "linux", windows)))]
mod surface;
#[cfg(all(feature = "webview", target_os = "linux"))]
mod webview;
#[cfg(all(feature = "webview", windows))]
mod windows;

/// Internal dispatch flag. The desktop checks it before initializing its UI.
pub const HELPER_ARGUMENT: &str = "--session-login-helper";
/// Internal dispatch flag of the verification helper, checked the same way.
pub const CAPTCHA_ARGUMENT: &str = "--session-captcha-helper";
/// Internal dispatch flag of the helper that reads the engine (Windows).
pub const ENGINE_ARGUMENT: &str = "--session-engine-helper";
#[cfg(all(feature = "webview", any(target_os = "linux", windows)))]
const LIFETIME: Duration = Duration::from_secs(600);
/// As long as the official client's challenge widget waits.
#[cfg(all(feature = "webview", any(target_os = "linux", windows)))]
const CAPTCHA_LIFETIME: Duration = Duration::from_secs(300);
/// A hidden page that only reads; WebView2's first start can take seconds.
#[cfg(all(feature = "webview", windows))]
const ENGINE_LIFETIME: Duration = Duration::from_secs(30);

/// Launches only the current RUNE executable, using private anonymous pipes.
#[derive(Default)]
pub struct HostedLogin;

impl HostedLogin {
    pub const fn new() -> Self {
        Self
    }

    /// Open the login window and wait for it, before RUNE's own window exists.
    pub fn acquire_blocking(&self) -> Result<SessionSecret, LoginError> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| LoginError::new(es("No se pudo abrir la ventana de inicio de sesión.")))?
            .block_on(self.acquire())
    }
}

/// Whether the person closed the login window without signing in.
pub fn cancelled(error: &LoginError) -> bool {
    *error == protocol::helper_error(Some(2))
}

/// A login made before RUNE's window opened, handed over at the first
/// sign-in; any later sign-in opens the login window again.
pub struct PrefetchedLogin {
    first: Mutex<Option<Result<SessionSecret, LoginError>>>,
}

impl PrefetchedLogin {
    pub fn new(first: Result<SessionSecret, LoginError>) -> Self {
        Self {
            first: Mutex::new(Some(first)),
        }
    }
}

impl SessionLogin for PrefetchedLogin {
    fn engine(&self) -> BoxFuture<'_, Result<Option<Engine>, LoginError>> {
        HostedLogin.engine()
    }

    fn acquire(&self) -> BoxFuture<'_, Result<SessionSecret, LoginError>> {
        let first = self.first.lock().ok().and_then(|mut first| first.take());
        Box::pin(async move {
            match first {
                Some(result) => result,
                None => HostedLogin.acquire().await,
            }
        })
    }

    fn verify(&self, challenge: Challenge) -> BoxFuture<'_, Result<ChallengeAnswer, LoginError>> {
        HostedLogin.verify(challenge)
    }
}

impl SessionLogin for HostedLogin {
    fn acquire(&self) -> BoxFuture<'_, Result<SessionSecret, LoginError>> {
        Box::pin(async {
            #[cfg(not(all(feature = "webview", any(target_os = "linux", windows))))]
            return Err(LoginError::new(es(
                "Esta compilación no incluye la ventana de inicio de sesión.",
            )));

            #[cfg(all(feature = "webview", any(target_os = "linux", windows)))]
            acquire_from(helper(HELPER_ARGUMENT)?, LIFETIME + Duration::from_secs(5)).await
        })
    }

    fn verify(&self, challenge: Challenge) -> BoxFuture<'_, Result<ChallengeAnswer, LoginError>> {
        #[cfg(not(all(feature = "webview", any(target_os = "linux", windows))))]
        {
            drop(challenge);
            Box::pin(async {
                Err(LoginError::new(es(
                    "Esta compilación no incluye la ventana de verificación.",
                )))
            })
        }

        #[cfg(all(feature = "webview", any(target_os = "linux", windows)))]
        Box::pin(async move {
            verify_from(
                helper(CAPTCHA_ARGUMENT)?,
                &challenge,
                CAPTCHA_LIFETIME + Duration::from_secs(5),
            )
            .await
        })
    }

    #[cfg(all(feature = "webview", windows))]
    fn engine(&self) -> BoxFuture<'_, Result<Option<Engine>, LoginError>> {
        Box::pin(async {
            let bytes = talk(
                helper(ENGINE_ARGUMENT)?,
                &[],
                protocol::MAX_ENGINE,
                ENGINE_LIFETIME + Duration::from_secs(5),
            )
            .await
            .map_err(|end| match end {
                Talk::Exit(code) => protocol::engine_error(code),
                _ => protocol::engine_error(None),
            })?;
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| protocol::engine_error(None))
        })
    }
}

/// This same executable as the helper `argument`, in RUNE's language; on
/// Windows also with the browser the adapter learnt it presents, if it did,
/// for the window to keep presenting that one. Before (the first login, or
/// the engine's own probe) a window presents WebView2's own, which is what
/// the adapter then learns.
#[cfg(all(feature = "webview", any(target_os = "linux", windows)))]
fn helper(argument: &str) -> Result<tokio::process::Command, LoginError> {
    let executable = std::env::current_exe()
        .map_err(|_| LoginError::new(es("No se pudo localizar el ejecutable de RUNE.")))?;
    let mut command = tokio::process::Command::new(executable);
    command
        .arg(argument)
        .env(LANGUAGE_VARIABLE, rune_i18n::current().code());
    #[cfg(windows)]
    if let Some(agent) = rune_discord_session::learnt_user_agent() {
        command.env(AGENT_VARIABLE, agent);
    }
    Ok(command)
}

/// How a helper exchange ended without a reply.
#[cfg(any(
    all(test, unix),
    all(feature = "webview", any(target_os = "linux", windows))
))]
enum Talk {
    Spawn,
    Input,
    Output,
    TooLarge,
    Exit(Option<i32>),
    Timeout,
}

/// Write `input` to a fresh helper and read its single bounded reply. The
/// child dies with this future: cancelling or timing out closes its window.
#[cfg(any(
    all(test, unix),
    all(feature = "webview", any(target_os = "linux", windows))
))]
async fn talk(
    mut command: tokio::process::Command,
    input: &[u8],
    max: usize,
    timeout: Duration,
) -> Result<Zeroizing<Vec<u8>>, Talk> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| Talk::Spawn)?;
    let exchange = async {
        let mut stdin = child.stdin.take().ok_or(Talk::Input)?;
        stdin.write_all(input).await.map_err(|_| Talk::Input)?;
        drop(stdin);
        let output = child.stdout.take().ok_or(Talk::Output)?;
        let mut bytes = Zeroizing::new(Vec::new());
        output
            .take((max + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| Talk::Output)?;
        if bytes.len() > max {
            return Err(Talk::TooLarge);
        }
        let status = child.wait().await.map_err(|_| Talk::Output)?;
        if !status.success() {
            return Err(Talk::Exit(status.code()));
        }
        Ok(bytes)
    };
    tokio::time::timeout(timeout, exchange)
        .await
        .map_err(|_| Talk::Timeout)?
}

#[cfg(any(
    all(test, unix),
    all(feature = "webview", any(target_os = "linux", windows))
))]
async fn acquire_from(
    command: tokio::process::Command,
    timeout: Duration,
) -> Result<SessionSecret, LoginError> {
    let capability = protocol::capability()?;
    let bytes = talk(command, capability.as_bytes(), protocol::MAX_REPLY, timeout)
        .await
        .map_err(|end| match end {
            Talk::Spawn => LoginError::new(es("No se pudo abrir la ventana de inicio de sesión.")),
            Talk::Input => LoginError::new(es("La ventana de inicio de sesión no respondió.")),
            Talk::Output => LoginError::new(es(
                "La ventana de inicio de sesión se cerró inesperadamente.",
            )),
            Talk::TooLarge => LoginError::new(es(
                "La respuesta de inicio de sesión superó el límite permitido.",
            )),
            Talk::Exit(code) => protocol::helper_error(code),
            Talk::Timeout => LoginError::new(es("Se agotó el tiempo de inicio de sesión.")),
        })?;
    protocol::decode_reply(&capability, &bytes)
}

#[cfg(any(
    all(test, unix),
    all(feature = "webview", any(target_os = "linux", windows))
))]
async fn verify_from(
    command: tokio::process::Command,
    challenge: &Challenge,
    timeout: Duration,
) -> Result<ChallengeAnswer, LoginError> {
    let capability = protocol::capability()?;
    let input = protocol::encode_challenge(&capability, challenge);
    let bytes = talk(
        command,
        input.as_bytes(),
        protocol::MAX_ANSWER_REPLY,
        timeout,
    )
    .await
    .map_err(|end| match end {
        Talk::Spawn => LoginError::new(es("No se pudo abrir la ventana de verificación.")),
        Talk::Input | Talk::Output | Talk::TooLarge => {
            LoginError::new(es("La ventana de verificación se cerró inesperadamente."))
        }
        Talk::Exit(code) => protocol::captcha_error(code),
        Talk::Timeout => protocol::captcha_error(Some(3)),
    })?;
    protocol::decode_answer(&capability, &bytes)
}

/// The language a helper's window is written in: its parent's.
const LANGUAGE_VARIABLE: &str = "RUNE_LANGUAGE";
/// The user agent the parent presents, for a WebView2 window to keep it
/// should its runtime have been updated since the engine was read.
#[cfg(windows)]
const AGENT_VARIABLE: &str = "RUNE_USER_AGENT";

/// A helper writes in the language of the RUNE that started it.
fn follow_parent_language() {
    if let Some(language) = std::env::var(LANGUAGE_VARIABLE)
        .ok()
        .as_deref()
        .and_then(Language::from_code)
    {
        rune_i18n::set(language);
    }
}

/// Run the temporary login helper on its own main thread, before eframe starts.
/// Its stdout is a private, bounded result pipe owned by `HostedLogin`.
pub fn run_helper() -> i32 {
    follow_parent_language();
    #[cfg(all(feature = "webview", target_os = "linux"))]
    {
        webview::run()
    }
    #[cfg(all(feature = "webview", windows))]
    {
        windows::run()
    }
    #[cfg(not(all(feature = "webview", any(target_os = "linux", windows))))]
    {
        5
    }
}

/// Run the temporary verification helper, as [`run_helper`] does the login.
pub fn run_captcha_helper() -> i32 {
    follow_parent_language();
    #[cfg(all(feature = "webview", target_os = "linux"))]
    {
        webview::run_captcha()
    }
    #[cfg(all(feature = "webview", windows))]
    {
        windows::run_captcha()
    }
    #[cfg(not(all(feature = "webview", any(target_os = "linux", windows))))]
    {
        5
    }
}

/// Run the hidden helper that reads WebView2's browser, as [`run_helper`].
pub fn run_engine_helper() -> i32 {
    #[cfg(all(feature = "webview", windows))]
    {
        windows::run_engine()
    }
    #[cfg(not(all(feature = "webview", windows)))]
    {
        5
    }
}

#[cfg(test)]
mod tests;
