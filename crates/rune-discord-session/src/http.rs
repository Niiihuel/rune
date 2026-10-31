use rune_i18n::es;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use reqwest::header::{HeaderValue, AUTHORIZATION, RETRY_AFTER};
use reqwest::{redirect::Policy, Client, Method, StatusCode};
use rune_model::{ChannelId, MessageId, MessagePatch, User};
use rune_provider::{Command, Event, InvitationJoinError};

mod admin;
// The account's synced settings (P1).
mod account;
mod challenges;
mod context;
// Commands and interactions (P3).
mod interactions;
mod invitations;
mod profile_edit;
mod reads;
mod rejections;
mod social;
mod stage;
mod threads;
mod uploads;
// Message actions and moderation (P4).
mod message_actions;
// Desktop, language and local voice (P6).
mod desktop;
// Presence, identity and expressions (P5).
mod identity;
// Private calls.
mod calls;
// Rules and onboarding (2026-10-18).
mod onboarding;
// Requests as the web client sends them (2026-10-19).
mod api;
// Scheduled events (eventos-programados).
mod events;
// AutoMod (automod).
mod automod;
pub(crate) use account::{presence, Known};
pub(crate) use challenges::{Answer, Solved};
pub(crate) use rejections::Issued;
use serde_json::{json, Value};
pub(crate) use threads::MAX_ACTIVE;
use tokio::time::Instant;
pub(crate) use uploads::Upload;

use crate::{voice::diag::note, wire, LoginError, SessionSecret};

// Streams (transmisiones).
/// The diagnostic lines of stream pictures: `rune: stream preview: …`.
pub(crate) const PREVIEW: &str = "stream preview";

/// The message flag that keeps a message from notifying anyone (P4).
pub(crate) const SUPPRESS_NOTIFICATIONS: u64 = 1 << 12;

#[derive(Clone)]
pub(crate) struct Http {
    client: Client,
    /// Attachment bytes to Discord's storage: no account header, and a
    /// deadline per stalled read instead of the 15 s of a whole request.
    storage: Client,
    base: String,
    cooldown: Arc<Mutex<Option<Instant>>>,
    /// The typing indicator's own hold: its route has a bucket of its own
    /// at Discord, so its 429 never holds a message (envios-y-subidas).
    typing_cooldown: Arc<Mutex<Option<Instant>>>,
    // Streams (transmisiones).
    /// Stream pictures' own hold, as typing's: a 429 there never holds a
    /// message, and says how long until the next picture.
    preview_cooldown: Arc<Mutex<Option<Instant>>>,
    /// The status of the latest stream picture request, for its diagnostic
    /// line; the session sends them one at a time.
    preview_status: Arc<Mutex<Option<StatusCode>>>,
    /// A person's answer to a challenge, for the one write it belongs to.
    answer: Arc<Mutex<Option<Answer>>>,
    /// What an administration request adds to itself and leaves behind.
    admin: admin::Extras,
    /// Where the invitation inspected last leads, for its acceptance's
    /// `X-Context-Properties` (2026-10-19).
    invitation: Arc<Mutex<Option<(String, HeaderValue)>>>,
}

#[derive(Debug)]
pub(crate) struct RestError {
    pub reason: LoginError,
    pub ambiguous: bool,
    pub ends_session: bool,
    pub invitation: Option<InvitationJoinError>,
    /// A challenge a person may solve to send this same request once more.
    pub challenge: Option<Box<Issued>>,
    /// Discord answered that it will not do this (400, 403 or 404), rather
    /// than the answer not coming, a limit or a failure of its own.
    pub refused: bool,
    // AutoMod (automod).
    /// A server's AutoMod refused the write, and what it tells whoever
    /// wrote it.
    pub blocked: Option<rejections::Blocked>,
}

impl RestError {
    pub(crate) fn rejected(reason: &'static str) -> Self {
        Self {
            reason: LoginError::new(reason),
            ambiguous: false,
            ends_session: false,
            invitation: None,
            challenge: None,
            refused: false,
            blocked: None,
        }
    }

    fn response(reason: LoginError, mutating: bool) -> Self {
        Self {
            reason,
            ambiguous: mutating,
            ends_session: false,
            invitation: None,
            challenge: None,
            refused: false,
            blocked: None,
        }
    }
}

// Production and fixtures share every transport policy; only the origin and
// proxy selection change, so redirect tests exercise the production builder.
fn client_builder() -> reqwest::ClientBuilder {
    Client::builder()
        // A non-idempotent action is one attempt, including protocol NACKs.
        .retry(reqwest::retry::never())
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
    // No default user agent: each request names the engine presented then
    // (`crate::browser`), which on Windows is learnt after this client exists.
}

/// The same policies, bounded per stalled read: a large file takes longer
/// than one request's 15 s, and the send's own deadline bounds it.
fn storage_builder() -> reqwest::ClientBuilder {
    Client::builder()
        .retry(reqwest::retry::never())
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
}

// Desktop session and games (escritorio-sesion-y-juegos).
/// `builder` with REST routed as `proxy` says: a proxy named has no way
/// around it, as Serein routes it. `None` for an address reqwest refuses.
fn proxied(
    builder: reqwest::ClientBuilder,
    proxy: &rune_provider::ApiProxy,
) -> Option<reqwest::ClientBuilder> {
    use rune_provider::ApiProxy;
    Some(match proxy {
        ApiProxy::Automatic => builder,
        ApiProxy::Direct => builder.no_proxy(),
        ApiProxy::Url(url) => builder.no_proxy().proxy(reqwest::Proxy::all(url).ok()?),
    })
}

impl Http {
    /// Requests going through the environment's proxies, if any.
    #[cfg(test)]
    pub fn new() -> Result<Self, LoginError> {
        Self::with_proxy(&rune_provider::ApiProxy::Automatic)
    }

    /// Requests and uploads going as `proxy` says.
    pub fn with_proxy(proxy: &rune_provider::ApiProxy) -> Result<Self, LoginError> {
        let failed = |_| LoginError::new(es("No se pudo iniciar el cliente HTTPS."));
        let routed = |builder| {
            proxied(builder, proxy)
                .ok_or_else(|| LoginError::new(es("La dirección del proxy no es válida.")))
        };
        Ok(Self {
            client: routed(client_builder())?.build().map_err(failed)?,
            storage: routed(storage_builder())?.build().map_err(failed)?,
            base: "https://discord.com/api/v10".into(),
            cooldown: Arc::default(),
            typing_cooldown: Arc::default(),
            preview_cooldown: Arc::default(),
            preview_status: Arc::default(),
            answer: Arc::default(),
            admin: Default::default(),
            invitation: Arc::default(),
        })
    }

    #[cfg(test)]
    pub fn local(base: &str) -> Self {
        let url = url::Url::parse(base).expect("local HTTP fixture URL");
        assert!(url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
        Self {
            client: client_builder().no_proxy().build().unwrap(),
            storage: storage_builder().no_proxy().build().unwrap(),
            base: format!("{base}/api/v10"),
            cooldown: Arc::default(),
            typing_cooldown: Arc::default(),
            preview_cooldown: Arc::default(),
            preview_status: Arc::default(),
            answer: Arc::default(),
            admin: Default::default(),
            invitation: Arc::default(),
        }
    }

    pub async fn verify(&self, secret: &SessionSecret) -> Result<User, LoginError> {
        self.identity(secret).await.map_err(|error| error.reason)
    }

    /// The same checks as [`Self::verify`], keeping whether the session was
    /// rejected (`ends_session`) apart from a transient failure.
    pub async fn identity(&self, secret: &SessionSecret) -> Result<User, RestError> {
        let value = self
            .request(secret, Method::GET, "/users/@me", None, 1024 * 1024)
            .await?;
        let user = wire::user(&value).map_err(|reason| RestError::response(reason, false))?;
        if user.bot {
            return Err(RestError::rejected(es(
                "Esta integración no admite cuentas de bot.",
            )));
        }
        Ok(user)
    }

    async fn request(
        &self,
        secret: &SessionSecret,
        method: Method,
        path: &str,
        body: Option<Value>,
        max_response: usize,
    ) -> Result<Value, RestError> {
        self.request_answer(secret, method, path, body, max_response, false)
            .await
    }

    /// As [`Self::request`]; with `either`, a success with or without a body
    /// (200, 201 or 204) is an answer, as the routes about people and
    /// threads give any of them.
    async fn request_answer(
        &self,
        secret: &SessionSecret,
        method: Method,
        path: &str,
        body: Option<Value>,
        max_response: usize,
        either: bool,
    ) -> Result<Value, RestError> {
        // Documented as 204; a 200 with a cooldown body is accepted too.
        let typing = method == Method::POST && path.ends_with("/typing");
        // Streams (transmisiones): a stream's picture, read or sent.
        let preview = path.starts_with("/streams/") && path.contains("/preview");
        if preview {
            self.set_preview_status(None);
        }
        let held = |hold: &Mutex<Option<Instant>>| {
            hold.lock()
                .map(|until| until.is_some_and(|until| until > Instant::now()))
                .map_err(|_| RestError::rejected(es("El transporte no está disponible.")))
        };
        if held(&self.cooldown)?
            || (typing && held(&self.typing_cooldown)?)
            || (preview && held(&self.preview_cooldown)?)
        {
            return Err(RestError::rejected(es(
                "Discord pidió esperar antes de otra solicitud.",
            )));
        }
        let mutating = method != Method::GET;
        let route = format!("{method} {path}");
        // A search answers 202, with a body, while its index is prepared.
        let search = method == Method::GET && path.contains("/messages/search?");
        let expected = if typing || matches!(method, Method::DELETE | Method::PUT) {
            StatusCode::NO_CONTENT
        } else {
            StatusCode::OK
        };
        let mut authorization = HeaderValue::from_str(&secret.0)
            .map_err(|_| RestError::rejected(es("La sesión local recibida no es válida.")))?;
        authorization.set_sensitive(true);
        let mut request = self
            .api(method, format!("{}{path}", self.base))
            .header(AUTHORIZATION, authorization);
        if let Some(solved) = self.answer_for(&route).filter(|_| mutating) {
            request = challenges::with_answer(request, &solved)
                .map_err(|()| RestError::rejected(es("La verificación recibida no es válida.")))?;
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        request = self.admin.apply(request);
        // A GET is sent once more, a second later, after a dead connection or
        // a timeout; a write is never sent twice.
        let again = (!mutating).then(|| request.try_clone()).flatten();
        let mut sent = request.send().await;
        let dead = matches!(&sent, Err(error) if error.is_connect() || error.is_timeout());
        if let (true, Some(again)) = (dead, again) {
            tokio::time::sleep(Duration::from_secs(1)).await;
            sent = again.send().await;
        }
        let response = sent.map_err(|error| {
            // Only the kind: the error's own text may name the route.
            let reason = if error.is_connect() {
                es("No se pudo conectar con Discord. Revisa la conexión a internet.")
            } else if error.is_timeout() {
                es("Discord no respondió a tiempo.")
            } else {
                es("No se pudo confirmar la respuesta de Discord.")
            };
            RestError::response(LoginError::new(reason), mutating)
        })?;
        let status = response.status();
        if preview {
            self.set_preview_status(Some(status));
        }
        let delay = if status == StatusCode::TOO_MANY_REQUESTS {
            response.headers().get(RETRY_AFTER)
        } else if response
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|v| v == "0")
        {
            response.headers().get("x-ratelimit-reset-after")
        } else {
            None
        };
        let header_delay = delay
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<f64>().ok());
        // Typing's own bucket, unless Discord says the limit is global.
        let headers = response.headers();
        let own = (typing || preview)
            && headers
                .get("x-ratelimit-global")
                .is_none_or(|value| value != "true")
            && headers
                .get("x-ratelimit-scope")
                .is_none_or(|value| value != "global");
        let bucket = match (own, preview) {
            (false, _) => Bucket::All,
            (true, false) => Bucket::Typing,
            (true, true) => Bucket::Preview,
        };
        if let Some(delay) = header_delay {
            self.defer(delay, bucket);
        }
        if status == StatusCode::TOO_MANY_REQUESTS && header_delay.is_none() {
            // A malformed error body still establishes a conservative hold.
            let body = read_bounded(response, 64 * 1024)
                .await
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .unwrap_or_default();
            let delay = body["retry_after"].as_f64().unwrap_or(60.0);
            let global = body["global"] == json!(true);
            self.defer(delay, if global { Bucket::All } else { bucket });
            return Err(status_error(status, mutating));
        }
        if !status.is_success() {
            // A note never written is empty, not a missing person, as Serein reads it.
            if !mutating && status == StatusCode::NOT_FOUND && path.starts_with("/users/@me/notes/")
            {
                return Ok(json!({ "note": "" }));
            }
            if matches!(status.as_u16(), 400 | 403 | 404) {
                if let Some(rejection) = rejections::inspect(response).await {
                    let mut error = rejection.error(true);
                    // Only a write may be sent again with a person's answer.
                    match error.challenge.as_mut() {
                        Some(issued) if mutating => issued.route = route,
                        _ => error.challenge = None,
                    }
                    return Err(error);
                }
            }
            // A known rejection remains such even if its body never arrives.
            // In particular, 401 must clear the session without reading it.
            return Err(status_error(status, mutating));
        }
        let optional_body = either
            && matches!(
                status,
                StatusCode::OK | StatusCode::CREATED | StatusCode::NO_CONTENT
            );
        let accepted = optional_body || search && status == StatusCode::ACCEPTED;
        if status != expected && !(typing && status == StatusCode::OK) && !accepted {
            return Err(RestError::response(wire::invalid(), mutating));
        }
        if status == StatusCode::NO_CONTENT || typing {
            return Ok(Value::Null);
        }
        let bytes = read_bounded(response, max_response)
            .await
            .map_err(|reason| RestError::response(reason, mutating))?;
        // Some writes confirm the change with an empty 200/201 instead of
        // 204. Only routes that explicitly allow no answer may omit JSON;
        // ordinary reads and a search's 202 still require their document.
        if optional_body && bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|_| RestError::response(wire::invalid(), mutating))
    }

    /// Hold the typing indicator or stream pictures alone, or
    /// conservatively every route; never repeat an ambiguous write.
    fn defer(&self, seconds: f64, bucket: Bucket) {
        let delay = Duration::try_from_secs_f64(seconds)
            .ok()
            .and_then(|duration| Instant::now().checked_add(duration))
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(365 * 24 * 60 * 60));
        let hold = match bucket {
            Bucket::Typing => &self.typing_cooldown,
            Bucket::Preview => &self.preview_cooldown,
            Bucket::All => &self.cooldown,
        };
        if let Ok(mut cooldown) = hold.lock() {
            *cooldown = Some(cooldown.map_or(delay, |old| old.max(delay)));
        }
    }

    /// Recover one exact baseline for local authorization, never a UI event.
    /// GET failures are definite; authenticated 401 still ends the session.
    pub async fn message_preflight(
        &self,
        secret: &SessionSecret,
        channel: ChannelId,
        message: MessageId,
    ) -> Result<Vec<Event>, RestError> {
        let value = self
            .request(
                secret,
                Method::GET,
                &format!("/channels/{channel}/messages/{message}"),
                None,
                1024 * 1024,
            )
            .await?;
        mapped_message(&value, channel, Some(message), None, false)
    }

    pub async fn execute(
        &self,
        secret: &SessionSecret,
        command: &Command,
        send_nonce: &str,
    ) -> Result<Vec<Event>, RestError> {
        match command {
            Command::LoadContext {
                request,
                channel,
                anchor,
                limit,
            } => {
                self.context(secret, *request, *channel, *anchor, *limit)
                    .await
            }
            Command::LoadHistory {
                request,
                channel,
                before,
                limit,
            } => {
                let limit = (*limit).clamp(1, 100);
                let mut path = format!("/channels/{channel}/messages?limit={limit}");
                if let Some(before) = before {
                    path.push_str(&format!("&before={before}"));
                }
                let values = self
                    .request(secret, Method::GET, &path, None, 8 * 1024 * 1024)
                    .await?;
                let values = values
                    .as_array()
                    .filter(|values| values.len() <= limit)
                    .ok_or_else(|| RestError::response(wire::invalid(), false))?;
                let mut users = Vec::new();
                let mut messages = Vec::new();
                for value in values {
                    let (message, authors) =
                        wire::message(value).map_err(|error| RestError::response(error, false))?;
                    if message.channel != *channel
                        || before.is_some_and(|before| message.id >= before)
                    {
                        return Err(RestError::response(wire::invalid(), false));
                    }
                    users.extend(authors.into_iter().map(Event::UserChanged));
                    messages.push(message);
                }
                messages.sort_by_key(|message| message.id);
                messages.dedup_by_key(|message| message.id);
                users.push(Event::HistoryLoaded {
                    request: *request,
                    channel: *channel,
                    before: *before,
                    reached_start: values.len() < limit,
                    messages,
                });
                Ok(users)
            }
            Command::SendMessage {
                channel,
                content,
                reply_to,
                nonce,
                mention_reply,
            } => {
                // `@silent` first: nobody is notified (P4), as Discord's flag 4096 says.
                let (content, silent) = match rune_provider::without_silent(content) {
                    Some(rest) => (rest, true),
                    None => (content.as_str(), false),
                };
                validate_content(content)?;
                let mut body = json!({"content":content,"nonce":send_nonce,
                    "allowed_mentions":{"parse":["users"],
                        "replied_user": *mention_reply && reply_to.is_some()}});
                if silent {
                    body["flags"] = json!(SUPPRESS_NOTIFICATIONS);
                }
                if let Some(message) = reply_to {
                    body["message_reference"] = json!({"message_id":message.to_string(),
                        "channel_id":channel.to_string(),"fail_if_not_exists":true});
                }
                let value = self
                    .request(
                        secret,
                        Method::POST,
                        &format!("/channels/{channel}/messages"),
                        Some(body),
                        1024 * 1024,
                    )
                    .await?;
                mapped_message(&value, *channel, None, Some(*nonce), true)
            }
            Command::SendSticker {
                channel,
                sticker,
                nonce,
            } => {
                let value = self
                    .request(
                        secret,
                        Method::POST,
                        &format!("/channels/{channel}/messages"),
                        Some(
                            json!({"sticker_ids":[sticker.to_string()], "nonce":send_nonce,
                        "allowed_mentions":{"parse":[],"replied_user":false}}),
                        ),
                        1024 * 1024,
                    )
                    .await?;
                mapped_message(&value, *channel, None, Some(*nonce), true)
            }
            Command::EditMessage {
                channel,
                message,
                content,
            } => {
                validate_content(content)?;
                let value = self
                    .request(
                        secret,
                        Method::PATCH,
                        &format!("/channels/{channel}/messages/{message}"),
                        Some(json!({"content":content,
                        "allowed_mentions":{"parse":["users"],"replied_user":false}})),
                        1024 * 1024,
                    )
                    .await?;
                mapped_message(&value, *channel, Some(*message), None, true)
            }
            Command::DeleteMessage { channel, message } => {
                self.request(
                    secret,
                    Method::DELETE,
                    &format!("/channels/{channel}/messages/{message}"),
                    None,
                    64 * 1024,
                )
                .await?;
                Ok(vec![Event::MessageDeleted {
                    channel: *channel,
                    message: *message,
                }])
            }
            Command::VotePoll {
                channel,
                message,
                answers,
            } => {
                // The vote replaces the account's earlier one; an empty list
                // takes it back. The Gateway's vote events confirm it.
                let ids: Vec<String> = answers.iter().map(u32::to_string).collect();
                self.request(
                    secret,
                    Method::PUT,
                    &format!("/channels/{channel}/polls/{message}/answers/@me"),
                    Some(json!({ "answer_ids": ids })),
                    64 * 1024,
                )
                .await?;
                Ok(Vec::new())
            }
            Command::StartTyping { channel } => {
                self.request(
                    secret,
                    Method::POST,
                    &format!("/channels/{channel}/typing"),
                    None,
                    64 * 1024,
                )
                .await?;
                Ok(Vec::new())
            }
            Command::PinMessage { channel, message }
            | Command::UnpinMessage { channel, message } => {
                let pinned = matches!(command, Command::PinMessage { .. });
                self.request(
                    secret,
                    if pinned { Method::PUT } else { Method::DELETE },
                    &format!("/channels/{channel}/messages/pins/{message}"),
                    None,
                    64 * 1024,
                )
                .await?;
                // The Gateway's MESSAGE_UPDATE carries the same field again.
                Ok(vec![Event::MessagePatched(MessagePatch {
                    channel: *channel,
                    id: *message,
                    pinned: Some(pinned),
                    ..Default::default()
                })])
            }
            Command::LoadPins { request, channel } => self.pins(secret, *request, *channel).await,
            Command::LoadStreamPreview {
                server,
                channel,
                owner,
            } => self.stream_preview(secret, *server, *channel, *owner).await,
            // Streams (transmisiones).
            Command::UploadStreamPreview {
                server,
                channel,
                owner,
                jpeg,
            } => {
                self.upload_stream_preview(secret, *server, *channel, *owner, jpeg)
                    .await
            }
            Command::SendFriendRequest { .. }
            | Command::AcceptFriendRequest { .. }
            | Command::RemoveRelationship { .. }
            | Command::BlockUser { .. }
            | Command::IgnoreUser { .. }
            | Command::UnignoreUser { .. }
            | Command::SetFriendNickname { .. }
            | Command::OpenDirect { .. }
            | Command::CreateGroup { .. }
            | Command::RenameGroup { .. }
            | Command::SetGroupIcon { .. }
            | Command::AddGroupMember { .. }
            | Command::RemoveGroupMember { .. }
            | Command::SetGroupOwner { .. }
            | Command::LeaveGroup { .. }
            | Command::CloseDirect { .. } => self.social(secret, command).await,
            command if crate::bootstrap::is_thread_command(command) => {
                self.threads(secret, command).await
            }
            // Private calls.
            Command::RingCall { .. } | Command::StopRinging { .. } => {
                self.ring(secret, command).await
            }
            // Calls and the voice panel (llamadas-y-panel-de-voz).
            Command::CheckCall { channel } => self.check_call(secret, *channel).await,
            // Rules and onboarding (2026-10-18).
            command if crate::onboarding::is_onboarding(command) => {
                self.onboarding(secret, command).await
            }
            // Scheduled events (eventos-programados).
            command if crate::events::is_events(command) => {
                self.scheduled_events(secret, command).await
            }
            // AutoMod (automod).
            command if crate::automod::is_automod(command) => self.automod(secret, command).await,
            _ => Err(RestError::rejected(es(
                "Esta acción todavía no está disponible.",
            ))),
        }
    }

    /// The latest picture of `owner`'s stream in `channel` of `server`
    /// (`None`: a conversation's call), as the official client asks it
    /// (REA): `{url}` under that stream's key, with `version` the time
    /// asked. A 429 holds stream pictures alone for its `retry_after`, which
    /// the answer carries; that, a missing preview or a malformed answer is
    /// no picture, never a failure the interface reports. Only a rejected
    /// session ends. Each answer leaves one diagnostic line: its status and
    /// why there is no picture, never the key or the address.
    pub async fn stream_preview(
        &self,
        secret: &SessionSecret,
        server: Option<rune_model::ServerId>,
        channel: ChannelId,
        owner: rune_model::UserId,
    ) -> Result<Vec<Event>, RestError> {
        let key = stream_key(server, channel, owner);
        let version = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis());
        let path = format!("/streams/{key}/preview?version={version}");
        let answer = self.request(secret, Method::GET, &path, None, 4096).await;
        let status = self.preview_answered();
        let (image, retry_after) = match answer {
            Ok(value) => match preview_url(&value, &key) {
                Ok(url) => {
                    note!(PREVIEW, "GET {status}: a picture on {}", preview_host(&url));
                    let kind = rune_model::ImageKind::Banner;
                    (Some(rune_model::RemoteImage { url, kind }), None)
                }
                Err(why) => {
                    let host = value["url"].as_str().map(preview_host);
                    let on = host.map(|host| format!(" ({host})")).unwrap_or_default();
                    note!(PREVIEW, "GET {status}: no picture, {why}{on}");
                    (None, None)
                }
            },
            Err(error) if error.ends_session => {
                note!(PREVIEW, "GET {status}: the session was rejected");
                return Err(error);
            }
            Err(_) => {
                let hold = self.preview_hold();
                let held = if hold.is_some() {
                    ", held for a 429"
                } else {
                    ""
                };
                note!(PREVIEW, "GET {status}: no picture{held}");
                (None, hold)
            }
        };
        Ok(vec![Event::StreamPreview {
            channel,
            owner,
            image,
            retry_after,
        }])
    }

    fn set_preview_status(&self, status: Option<StatusCode>) {
        if let Ok(mut seen) = self.preview_status.lock() {
            *seen = status;
        }
    }

    /// How the latest stream picture request was answered, for its line.
    fn preview_answered(&self) -> String {
        match self.preview_status.lock().ok().and_then(|seen| *seen) {
            Some(status) => format!("answered {}", status.as_u16()),
            None => "not answered".into(),
        }
    }

    /// Every route is held for a 429.
    #[cfg(test)]
    pub(crate) fn held_for_everything(&self) -> bool {
        self.cooldown
            .lock()
            .is_ok_and(|until| until.is_some_and(|until| until > Instant::now()))
    }

    /// How long stream pictures are held for a 429, if they are.
    pub(crate) fn preview_hold(&self) -> Option<Duration> {
        let until = (*self.preview_cooldown.lock().ok()?)?;
        Some(until.checked_duration_since(Instant::now())?).filter(|left| !left.is_zero())
    }

    /// The picture of the account's own stream, as the official client
    /// sends it (REA): `{thumbnail}`, a JPEG as a data URL. Its failure is
    /// nobody's to see; only a rejected session ends.
    pub async fn upload_stream_preview(
        &self,
        secret: &SessionSecret,
        server: Option<rune_model::ServerId>,
        channel: ChannelId,
        owner: rune_model::UserId,
        jpeg: &[u8],
    ) -> Result<Vec<Event>, RestError> {
        use base64::Engine as _;
        let key = stream_key(server, channel, owner);
        let thumbnail = format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(jpeg)
        );
        let body = json!({ "thumbnail": thumbnail });
        let path = format!("/streams/{key}/preview");
        let answer = self
            .request_answer(secret, Method::POST, &path, Some(body), 4096, true)
            .await;
        let status = self.preview_answered();
        let sent = if answer.is_ok() { "sent" } else { "not sent" };
        // shortcut: a picture not sent waits for the next one (5 min), not
        // the official client's 60 s; an answer to the interface would let
        // it try sooner.
        note!(
            PREVIEW,
            "POST {status}: own picture {sent} ({} KiB)",
            jpeg.len() / 1024
        );
        match answer {
            Err(error) if error.ends_session => Err(error),
            _ => Ok(Vec::new()),
        }
    }

    /// `user`'s full profile as seen in `server`; one GET, never repeated,
    /// and with `effect` a second one for the effect they wear.
    pub async fn profile(
        &self,
        secret: &SessionSecret,
        request: rune_provider::RequestId,
        user: rune_model::UserId,
        server: Option<rune_model::ServerId>,
        mutuals: bool,
        effect: bool,
    ) -> Result<Vec<Event>, RestError> {
        let path = crate::profiles::path(user, server, mutuals);
        let value = self
            .request(
                secret,
                Method::GET,
                &path,
                None,
                crate::profiles::MAX_RESPONSE,
            )
            .await?;
        let mut events = crate::profiles::profile(&value, request, user, server, mutuals)
            .map_err(|reason| RestError::response(reason, false))?;
        // The effect they wear, read from its product (P5); without it the
        // profile still shows. Hidden effects skip that read, as Serein's.
        if let Some(sku) = crate::identity::effect_sku(&value).filter(|_| effect) {
            let product = self
                .request(
                    secret,
                    Method::GET,
                    &format!("/collectibles-products/{sku}"),
                    None,
                    crate::profiles::MAX_RESPONSE,
                )
                .await;
            let effect = match product {
                Err(error) if error.ends_session => return Err(error),
                Err(_) => None,
                Ok(product) => crate::identity::effect(&product, sku),
            };
            if let Some(Event::ProfileLoaded { profile, .. }) = events.last_mut() {
                profile.effect = effect;
            }
        }
        Ok(events)
    }

    /// Add (`add`) or take back the account's Unicode `emoji` on a message.
    /// The answer is the change itself; the Gateway echoes it once more.
    pub async fn react(
        &self,
        secret: &SessionSecret,
        channel: ChannelId,
        message: MessageId,
        emoji: &str,
        add: bool,
    ) -> Result<Vec<Event>, RestError> {
        // The whole emoji is one path segment, every byte escaped.
        let encoded: String = emoji.bytes().map(|byte| format!("%{byte:02X}")).collect();
        self.request(
            secret,
            if add { Method::PUT } else { Method::DELETE },
            &format!("/channels/{channel}/messages/{message}/reactions/{encoded}/@me"),
            None,
            64 * 1024,
        )
        .await?;
        Ok(vec![Event::ReactionChanged {
            channel,
            message,
            emoji: emoji.to_owned(),
            me: true,
            added: add,
        }])
    }

    /// Every pinned message of `channel`, newest first, in bounded pages.
    /// A list longer than the service's documented maximum is refused, never cut.
    async fn pins(
        &self,
        secret: &SessionSecret,
        request: u64,
        channel: ChannelId,
    ) -> Result<Vec<Event>, RestError> {
        const PAGE: usize = 50;
        const PAGES: usize = 5;
        let bad = || RestError::response(wire::invalid(), false);
        let mut events = Vec::new();
        let mut messages: Vec<rune_model::Message> = Vec::new();
        let mut before: Option<String> = None;
        for _ in 0..PAGES {
            let mut path = format!("/channels/{channel}/messages/pins?limit={PAGE}");
            if let Some(before) = &before {
                let encoded: String = before.bytes().map(|byte| format!("%{byte:02X}")).collect();
                path.push_str(&format!("&before={encoded}"));
            }
            let page = self
                .request(secret, Method::GET, &path, None, 8 * 1024 * 1024)
                .await?;
            let items = page["items"]
                .as_array()
                .filter(|items| items.len() <= PAGE)
                .ok_or_else(bad)?;
            for item in items {
                let (mut message, users) = wire::message(&item["message"])
                    .map_err(|reason| RestError::response(reason, false))?;
                if message.channel != channel || messages.iter().any(|known| known.id == message.id)
                {
                    return Err(bad());
                }
                message.pinned = true;
                messages.push(message);
                events.extend(users.into_iter().map(Event::UserChanged));
            }
            if !page["has_more"].as_bool().ok_or_else(bad)? {
                events.push(Event::PinsLoaded {
                    request,
                    channel,
                    messages,
                });
                return Ok(events);
            }
            before = Some(
                items
                    .last()
                    .and_then(|item| item["pinned_at"].as_str())
                    .filter(|at| !at.is_empty() && at.len() <= 64)
                    .ok_or_else(bad)?
                    .to_owned(),
            );
        }
        Err(RestError::rejected(es(
            "Este canal tiene más mensajes fijados de los que RUNE puede leer.",
        )))
    }
}

/// A preview's URL, only on Discord's CDN, under the stream's `key` and
/// named by a hex hash, as Serein accepts it.
/// Which hold a 429 sets.
#[derive(Clone, Copy)]
pub(super) enum Bucket {
    All,
    Typing,
    Preview,
}

/// The largest picture of the account's own stream sent: a JPEG that fits
/// 512×288 is far smaller.
const MAX_PREVIEW_JPEG: usize = 512 * 1024;

/// A stream's key, as the official client names it:
/// `guild:{server}:{channel}:{owner}`, or `call:{channel}:{owner}` in a
/// conversation's call.
pub(crate) fn stream_key(
    server: Option<rune_model::ServerId>,
    channel: ChannelId,
    owner: rune_model::UserId,
) -> String {
    match server {
        Some(server) => format!("guild:{server}:{channel}:{owner}"),
        None => format!("call:{channel}:{owner}"),
    }
}

/// The picture of the account's own stream is a JPEG of a sensible size.
pub(crate) fn preview_fits(command: &Command) -> bool {
    matches!(command, Command::UploadStreamPreview { jpeg, .. }
        if jpeg.starts_with(&[0xFF, 0xD8, 0xFF]) && jpeg.len() <= MAX_PREVIEW_JPEG)
}

/// The hosts of Discord's media a stream's picture may be on: its CDN and
/// its media proxy, as Serein reads it (the official client shows the
/// address as it comes, REA).
const PREVIEW_HOSTS: [&str; 2] = ["cdn.discordapp.com", "media.discordapp.net"];

/// Where a stream's picture comes from, as a diagnostic line may say it:
/// the host alone, printable and short.
pub(crate) fn preview_host(raw: &str) -> String {
    url::Url::parse(raw)
        .ok()
        .and_then(|url| url.host_str().map(crate::voice::diag::shown_endpoint))
        .unwrap_or_else(|| "no host".into())
}

/// The address of the picture `value` names for the stream `key`, or why
/// it is not one to load: https on Discord's media hosts, without a user,
/// port or fragment, under `/streams/{key}/` (its colons as they are or
/// percent-encoded), the file below it named in plain characters. The
/// file's name and kind are not assumed: the service has not shown them.
fn preview_url(value: &Value, key: &str) -> Result<String, &'static str> {
    let raw = value["url"].as_str().ok_or("no url in the answer")?;
    if raw.len() > 2048 || raw.chars().any(|c| c.is_control() || c == '\\') {
        return Err("a malformed url");
    }
    let url = url::Url::parse(raw).map_err(|_| "a malformed url")?;
    if url.scheme() != "https" {
        return Err("not https");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("a user in the url");
    }
    if url.port().is_some() {
        return Err("a port in the url");
    }
    if url.fragment().is_some() {
        return Err("a fragment in the url");
    }
    if !url
        .host_str()
        .is_some_and(|host| PREVIEW_HOSTS.contains(&host))
    {
        return Err("a host not of Discord's media");
    }
    // Parsing resolved any `..`, plain or encoded: what is left is where it
    // leads.
    let (segment, file) = url
        .path()
        .strip_prefix("/streams/")
        .and_then(|rest| rest.split_once('/'))
        .ok_or("not under /streams/")?;
    if segment.replace("%3A", ":").replace("%3a", ":") != key {
        return Err("another stream's key");
    }
    let plain = !file.is_empty()
        && file.len() <= 256
        && file.split('/').all(|part| !part.is_empty())
        && file
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'));
    if !plain {
        return Err("no plain file name");
    }
    Ok(url.into())
}

fn status_error(status: StatusCode, mutating: bool) -> RestError {
    RestError {
        reason: LoginError::new(match status.as_u16() {
            401 => es("Discord rechazó la sesión; inicia sesión otra vez."),
            403 => es("No tienes permiso para completar esta acción."),
            404 => es("El canal o mensaje ya no está disponible."),
            429 => es("Discord pidió esperar. La acción no se reintentó."),
            400 => es("Discord rechazó los datos de esta acción."),
            _ => es("Discord no pudo confirmar esta acción."),
        }),
        ambiguous: mutating && (status.is_server_error() || status.is_redirection()),
        ends_session: status == StatusCode::UNAUTHORIZED,
        invitation: None,
        challenge: None,
        refused: matches!(status.as_u16(), 400 | 403 | 404),
        blocked: None,
    }
}

async fn read_bounded(
    mut response: reqwest::Response,
    maximum: usize,
) -> Result<Vec<u8>, LoginError> {
    let too_large = || LoginError::new(es("La respuesta de Discord excede el límite local."));
    if response
        .content_length()
        .is_some_and(|size| size > maximum as u64)
    {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| LoginError::new(es("La respuesta de Discord quedó incompleta.")))?
    {
        if bytes.len().saturating_add(chunk.len()) > maximum {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn validate_content(content: &str) -> Result<(), RestError> {
    if content.trim().is_empty() || content.encode_utf16().count() > 2000 {
        return Err(RestError::rejected(es(
            "El mensaje debe contener entre 1 y 2000 caracteres.",
        )));
    }
    Ok(())
}

fn mapped_message(
    value: &Value,
    channel: ChannelId,
    expected_id: Option<MessageId>,
    nonce: Option<u64>,
    mutating: bool,
) -> Result<Vec<Event>, RestError> {
    let (message, users) =
        wire::message(value).map_err(|error| RestError::response(error, mutating))?;
    if message.channel != channel || expected_id.is_some_and(|expected| expected != message.id) {
        return Err(RestError::response(wire::invalid(), mutating));
    }
    let mut events: Vec<_> = users.into_iter().map(Event::UserChanged).collect();
    events.push(if expected_id.is_some() {
        Event::MessageUpdated(message)
    } else {
        Event::MessageCreated { message, nonce }
    });
    Ok(events)
}

// Desktop session and games (escritorio-sesion-y-juegos).
#[cfg(test)]
mod proxy_tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    /// A proxy on loopback that hears the first line of what comes and
    /// refuses it: nothing reaches Discord.
    #[tokio::test]
    async fn a_named_proxy_carries_requests_and_uploads_with_no_way_around() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy =
            rune_provider::ApiProxy::url(&format!("http://{}", listener.local_addr().unwrap()))
                .unwrap();
        let http = Http::with_proxy(&proxy).unwrap();
        let heard = tokio::spawn(async move {
            let mut lines = Vec::new();
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut head = [0u8; 256];
                let read = socket.read(&mut head).await.unwrap();
                let text = String::from_utf8_lossy(&head[..read]).into_owned();
                lines.push(text.lines().next().unwrap_or_default().to_owned());
                let _ = socket
                    .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                    .await;
            }
            lines
        });
        assert!(http
            .client
            .get(format!("{}/gateway", http.base))
            .send()
            .await
            .is_err());
        assert!(http
            .storage
            .put("https://discord-attachments-uploads-prd.storage.googleapis.com/x")
            .send()
            .await
            .is_err());
        assert_eq!(
            heard.await.unwrap(),
            [
                "CONNECT discord.com:443 HTTP/1.1",
                "CONNECT discord-attachments-uploads-prd.storage.googleapis.com:443 HTTP/1.1"
            ]
        );
        // Straight, the environment is not asked; a bad address never builds.
        assert!(Http::with_proxy(&rune_provider::ApiProxy::Direct).is_ok());
        assert!(proxied(
            client_builder(),
            &rune_provider::ApiProxy::Url("not a url".into())
        )
        .is_none());
    }
}
