use rune_i18n::{es, t};
use std::future::pending;
use std::time::Duration;

use rune_provider::{
    AuthenticationState, BoxFuture, Capabilities, Command, ConnectionState, Event, EventSink,
    Failure, Provider, ProviderError, ProviderInfo, ProviderKind,
};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::time::Instant;
use url::Url;

use crate::{
    auth::Attempt,
    http::{snowflake, Http, Session},
    AuthError, DEFAULT_REDIRECT_URI,
};

/// An invalid public application ID or unavailable HTTPS configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigError(&'static str);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(t(self.0))
    }
}
impl std::error::Error for ConfigError {}

/// Read-only OAuth2 identity and basic guild listing; credentials stay in RAM.
pub struct DiscordProvider {
    pub(crate) client_id: String,
    pub(crate) http: Http,
    pub(crate) redirect: Url,
}

enum Step {
    Code(Attempt, String),
    SignedIn(Session),
    Revoked(bool),
}
type Operation = BoxFuture<'static, Result<Step, AuthError>>;

impl DiscordProvider {
    pub fn new(client_id: String) -> Result<Self, ConfigError> {
        Self::with_proxy(client_id, &rune_provider::ApiProxy::Automatic)
    }

    /// As [`Self::new`], its requests going as `proxy` says
    /// (escritorio-sesion-y-juegos). The browser's authorization does not.
    pub fn with_proxy(
        client_id: String,
        proxy: &rune_provider::ApiProxy,
    ) -> Result<Self, ConfigError> {
        let id = snowflake(&client_id).map_err(|_| {
            ConfigError(es(
                "El ID público de la aplicación debe ser un entero decimal distinto de cero.",
            ))
        })?;
        if id.to_string() != client_id {
            return Err(ConfigError(es(
                "El ID público de la aplicación no debe tener ceros iniciales.",
            )));
        }
        Ok(Self {
            client_id,
            http: Http::new(proxy).map_err(|e| ConfigError(e.0))?,
            redirect: Url::parse(DEFAULT_REDIRECT_URI).expect("fixed loopback redirect"),
        })
    }

    async fn begin(&self) -> Result<(String, Operation), AuthError> {
        let listener = TcpListener::bind(("127.0.0.1", self.redirect.port_or_known_default().unwrap_or(80))).await
            .map_err(|_| AuthError(es("El puerto local de autorización está ocupado. Cerrá el otro intento y volvé a intentar.")))?;
        let attempt = Attempt::new(self.redirect.clone())?;
        let url = attempt.authorize_url(&self.http.authorize, &self.client_id);
        Ok((
            url,
            Box::pin(async move {
                let (attempt, code) = attempt.receive(listener).await?;
                Ok(Step::Code(attempt, code))
            }),
        ))
    }

    async fn run(
        &self,
        commands: &mut mpsc::Receiver<Command>,
        events: &EventSink,
    ) -> Result<(), ProviderError> {
        emit(
            events,
            Event::Authentication(AuthenticationState::SignedOut),
        )
        .await?;
        let mut operation: Option<Operation> = None;
        let mut session: Option<Session> = None;
        loop {
            let expires = session
                .as_ref()
                .map(|s| s.expires)
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(366 * 86400));
            tokio::select! {
                biased;
                command = commands.recv() => {
                    let Some(command) = command else { return Ok(()); };
                    match command {
                        Command::BeginSignIn if session.is_none() => {
                            // Dropping the previous future closes its listener and
                            // discards any late exchange result before starting again.
                            operation = None;
                            match self.begin().await {
                                Ok((url, next)) => {
                                    operation = Some(next);
                                    emit(events, Event::Authentication(AuthenticationState::AwaitingBrowser { url })).await?;
                                }
                                Err(error) => emit(events, Event::Authentication(AuthenticationState::Failed { reason: error.to_string() })).await?,
                            }
                        }
                        Command::BeginSignIn => {}
                        Command::CancelSignIn => {
                            operation = None;
                            if session.is_none() { emit(events, Event::Authentication(AuthenticationState::SignedOut)).await?; }
                        }
                        Command::SignOut => {
                            operation = None;
                            let old = session.take();
                            clear(events, t("Sesión cerrada en este equipo.")).await?;
                            if let Some(old) = old {
                                let http = self.http.clone();
                                let id = self.client_id.clone();
                                operation = Some(Box::pin(async move { Ok(Step::Revoked(http.revoke(&id, old.access_token).await)) }));
                            }
                        }
                        other => if let Some(failure) = Failure::of(&other) {
                            emit(events, Event::Failed { failure, reason: t("La integración OAuth2 solo permite identidad y un listado básico de servidores; esta función no está habilitada.").into(), ambiguous: false }).await?;
                        },
                    }
                }
                result = async { match operation.as_mut() { Some(next) => next.await, None => pending().await } } => {
                    operation = None;
                    match result {
                        Ok(Step::Code(attempt, code)) => {
                            emit(events, Event::Authentication(AuthenticationState::Exchanging)).await?;
                            let http = self.http.clone();
                            let id = self.client_id.clone();
                            operation = Some(Box::pin(async move { http.exchange(&id, attempt, code).await.map(Step::SignedIn) }));
                        }
                        Ok(Step::SignedIn(mut signed_in)) => {
                            let snapshot = std::mem::take(&mut signed_in.snapshot);
                            emit(events, Event::Ready(snapshot)).await?;
                            emit(events, Event::Connection(ConnectionState::Connected)).await?;
                            emit(events, Event::Authentication(AuthenticationState::SignedIn { scopes: signed_in.scopes.clone() })).await?;
                            session = Some(signed_in);
                        }
                        Ok(Step::Revoked(false)) => emit(events, Event::Failed { failure: Failure::Other, reason: t("La sesión local ya está cerrada. No se confirmó la revocación remota; podés quitar RUNE en Discord → Ajustes → Aplicaciones autorizadas.").into(), ambiguous: false }).await?,
                        Ok(Step::Revoked(true)) => {}
                        Err(error) => emit(events, Event::Authentication(AuthenticationState::Failed { reason: error.to_string() })).await?,
                    }
                }
                _ = tokio::time::sleep_until(expires), if session.is_some() => {
                    session = None;
                    clear(events, t("La autorización caducó. Volvé a iniciar sesión.")).await?;
                }
            }
        }
    }
}

async fn emit(events: &EventSink, event: Event) -> Result<(), ProviderError> {
    events
        .send(event)
        .await
        .map_err(|_| ProviderError::Fatal("La interfaz se cerró.".into()))
}

async fn clear(events: &EventSink, reason: &str) -> Result<(), ProviderError> {
    emit(
        events,
        Event::Authentication(AuthenticationState::SignedOut),
    )
    .await?;
    emit(
        events,
        Event::Connection(ConnectionState::Disconnected {
            reason: reason.into(),
        }),
    )
    .await
}

impl Provider for DiscordProvider {
    fn image_origins(&self) -> &'static [&'static str] {
        &["https://cdn.discordapp.com"]
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: es("Discord · OAuth2 experimental").into(), kind: ProviderKind::Official,
            capabilities: Capabilities { authentication: true, account_servers: true, ..Capabilities::default() },
            notice: es("OAuth2 experimental: identidad y listado básico de servidores. Sin mensajes ni voz. Canje público pendiente de validación con tu aplicación.").into(),
        }
    }

    fn connect<'a>(
        &'a self,
        commands: &'a mut mpsc::Receiver<Command>,
        events: &'a EventSink,
    ) -> BoxFuture<'a, Result<(), ProviderError>> {
        Box::pin(self.run(commands, events))
    }
}
