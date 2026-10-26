//! Bounded JSON Gateway transport for the explicitly unofficial session adapter.
//! Protocol opcodes follow Discord's published Gateway documentation. Normal
//! account Identify is not an official application integration or live-verified.
//! Identify presents the web client in a WebKit browser, as REST does
//! (owner decision of 2026-10-19, `crate::browser`); no capability flags.
//! The one undocumented message sent is the member list subscription (op 37)
//! Serein uses, owner-authorized on 2026-10-16 and paced below. Voice state
//! updates (op 4) join, move and leave the account's call; ops 20, 19 and 21
//! watch, let go of and ask again for a stream in it, op 18 creates the
//! account's own and op 22 pauses it (owner-authorized REA, 2026-10-16).
//! The account's status and custom status go in Identify and as presence
//! updates (op 3, documented), at most one every five seconds as Serein
//! paces them (P1, 2026-10-17).
//!
//! Op 8 looks members up by name, as Serein does for the composer
//! (`member_search`), paced far below the published budget.
//!
//! What this device says the person is doing (another program's Rich
//! Presence, a game) goes in the same presence update, beside the custom
//! status (P6, 2026-10-17).

use rune_i18n::es;
use std::{collections::VecDeque, sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    net::TcpStream,
    sync::{mpsc, watch, Notify},
    time::{sleep, sleep_until, timeout, Instant},
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        client::IntoClientRequest, protocol::WebSocketConfig, Error as WsError, Message,
    },
    MaybeTlsStream, WebSocketStream,
};
use zeroize::Zeroizing;

use crate::{
    member_search::{self, Lookup, Pacer},
    members::Watch,
    settings_proto::Own,
    voice::{diag::note, VoiceWant},
    LoginError, SessionSecret,
};

/// The scope of this transport's diagnostic lines (`voice::diag`): why it
/// reconnects, and what it tells the service about voice.
const GATEWAY: &str = "gateway";

const ENDPOINT: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
const MAX_PAYLOAD: usize = 8 * 1024 * 1024;
const MAX_DISPATCH: usize = 1024 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(10);
const READY_TIMEOUT: Duration = Duration::from_secs(60);
/// Discord's reconnect backoff: it starts at a second, grows by up to twice
/// itself each failure and never passes a minute. Transport failures never
/// stop it; only terminal closes and local limits do.
const BACKOFF_START_MS: u64 = 1000;
const BACKOFF_MAX_MS: u64 = 60_000;
/// From this many failures in a row on, the session is forgotten and the
/// next attempt identifies, as Discord's client does.
const FRESH_AFTER: u32 = 4;
/// How long an expedited heartbeat (asked after a network change) may wait
/// for its ACK before the connection is taken as dead.
const EXPEDITED_ACK: Duration = Duration::from_secs(5);
/// Discord allows one IDENTIFY every 5 s; asking to hurry never cuts it.
const IDENTIFY_GAP: Duration = Duration::from_secs(5);
/// Member list frames per minute, at most, and the least time between two
/// changes: far below the write budget, so scrolling never spends it.
const LIST_FRAMES: usize = 20;
const LIST_WINDOW: Duration = Duration::from_secs(60);
const LIST_GAP: Duration = Duration::from_millis(500);
/// How long a list opened waits for its first sync before it is sent again;
/// doubled for each try, up to eight times this.
const LIST_SYNC_WAIT: Duration = Duration::from_secs(15);
/// What the provider wants the Gateway to keep: the member list, the voice
/// state, the account's presence and the members to look up.
type Wishes<'a> = (
    &'a mut watch::Receiver<Option<Watch>>,
    &'a mut watch::Receiver<Option<VoiceWant>>,
    &'a mut watch::Receiver<Option<Own>>,
    &'a mut watch::Receiver<Option<Lookup>>,
    // Desktop (P6): the local activity to publish, already resolved.
    &'a mut watch::Receiver<Option<Value>>,
    // Private calls: the conversation whose call to ask about (op 13).
    &'a mut watch::Receiver<Option<(u64, u32)>>,
    // Voice protocol (voz-protocolo): the server shown (op 43).
    &'a mut watch::Receiver<Option<u64>>,
);
/// The least time between two presence updates: stricter than the
/// documented 5 per 20 seconds, as Serein sends them.
const PRESENCE_GAP: Duration = Duration::from_secs(5);
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Deliberately has no Debug/Serialize implementation: private wire data must
/// be mapped to domain entities before leaving this adapter.
pub(crate) enum GatewayEvent {
    /// The id of the session that starts, just before its `Ready`: apps'
    /// interactions go with it (P3). Never part of the snapshot.
    Session(Zeroizing<String>),
    Ready(Value),
    Dispatch {
        kind: DispatchKind,
        data: Value,
    },
    Reconnecting {
        attempt: u32,
        retry_in_ms: u64,
    },
    Resumed,
}

macro_rules! dispatch_kinds {
    ($($variant:ident => $wire:literal),+ $(,)?) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub(crate) enum DispatchKind { $($variant),+ }
        impl DispatchKind {
            pub(crate) fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $wire),+ }
            }
            fn parse(name: &str) -> Option<Self> {
                match name { $($wire => Some(Self::$variant)),+, _ => None }
            }
        }
    };
}
dispatch_kinds! {
    ReadySupplemental => "READY_SUPPLEMENTAL",
    MessageCreate => "MESSAGE_CREATE", MessageUpdate => "MESSAGE_UPDATE",
    MessageDelete => "MESSAGE_DELETE", MessageDeleteBulk => "MESSAGE_DELETE_BULK",
    MessageReactionAdd => "MESSAGE_REACTION_ADD", MessageReactionRemove => "MESSAGE_REACTION_REMOVE",
    MessageReactionRemoveAll => "MESSAGE_REACTION_REMOVE_ALL", MessageReactionRemoveEmoji => "MESSAGE_REACTION_REMOVE_EMOJI",
    MessagePollVoteAdd => "MESSAGE_POLL_VOTE_ADD", MessagePollVoteRemove => "MESSAGE_POLL_VOTE_REMOVE",
    ChannelCreate => "CHANNEL_CREATE", ChannelUpdate => "CHANNEL_UPDATE", ChannelDelete => "CHANNEL_DELETE",
    GuildCreate => "GUILD_CREATE", GuildUpdate => "GUILD_UPDATE", GuildDelete => "GUILD_DELETE",
    GuildRoleCreate => "GUILD_ROLE_CREATE", GuildRoleUpdate => "GUILD_ROLE_UPDATE", GuildRoleDelete => "GUILD_ROLE_DELETE",
    GuildEmojisUpdate => "GUILD_EMOJIS_UPDATE", GuildStickersUpdate => "GUILD_STICKERS_UPDATE",
    GuildBanAdd => "GUILD_BAN_ADD", GuildBanRemove => "GUILD_BAN_REMOVE",
    GuildMemberAdd => "GUILD_MEMBER_ADD", GuildMemberUpdate => "GUILD_MEMBER_UPDATE", GuildMemberRemove => "GUILD_MEMBER_REMOVE",
    PresenceUpdate => "PRESENCE_UPDATE", TypingStart => "TYPING_START", UserUpdate => "USER_UPDATE",
    ThreadCreate => "THREAD_CREATE", ThreadUpdate => "THREAD_UPDATE", ThreadDelete => "THREAD_DELETE",
    RelationshipAdd => "RELATIONSHIP_ADD", RelationshipUpdate => "RELATIONSHIP_UPDATE",
    RelationshipRemove => "RELATIONSHIP_REMOVE",
    ChannelRecipientAdd => "CHANNEL_RECIPIENT_ADD", ChannelRecipientRemove => "CHANNEL_RECIPIENT_REMOVE",
    ThreadListSync => "THREAD_LIST_SYNC", ThreadMemberUpdate => "THREAD_MEMBER_UPDATE",
    ThreadMembersUpdate => "THREAD_MEMBERS_UPDATE",
    GuildMemberListUpdate => "GUILD_MEMBER_LIST_UPDATE",
    VoiceStateUpdate => "VOICE_STATE_UPDATE", VoiceServerUpdate => "VOICE_SERVER_UPDATE",
    StreamCreate => "STREAM_CREATE", StreamServerUpdate => "STREAM_SERVER_UPDATE",
    StreamUpdate => "STREAM_UPDATE", StreamDelete => "STREAM_DELETE",
    // The account's synced settings (P1).
    UserGuildSettingsUpdate => "USER_GUILD_SETTINGS_UPDATE", UserNoteUpdate => "USER_NOTE_UPDATE",
    UserSettingsProtoUpdate => "USER_SETTINGS_PROTO_UPDATE", SessionsReplace => "SESSIONS_REPLACE",
    // Reads, search and remote members (P2).
    MessageAck => "MESSAGE_ACK", PassiveUpdateV1 => "PASSIVE_UPDATE_V1",
    PassiveUpdateV2 => "PASSIVE_UPDATE_V2", GuildMembersChunk => "GUILD_MEMBERS_CHUNK",
    // Commands and interactions (P3).
    InteractionCreate => "INTERACTION_CREATE", InteractionSuccess => "INTERACTION_SUCCESS",
    InteractionFailure => "INTERACTION_FAILURE", InteractionModalCreate => "INTERACTION_MODAL_CREATE",
    CommandSuggestions => "APPLICATION_COMMAND_AUTOCOMPLETE_RESPONSE",
    CommandIndexUpdate => "GUILD_APPLICATION_COMMAND_INDEX_UPDATE",
    // Private calls.
    CallCreate => "CALL_CREATE", CallUpdate => "CALL_UPDATE", CallDelete => "CALL_DELETE",
    // Rail and notifications.
    EmbeddedActivityUpdate => "EMBEDDED_ACTIVITY_UPDATE_V2",
    // Voice protocol (voz-protocolo).
    ChannelInfo => "CHANNEL_INFO", VoiceChannelStartTimeUpdate => "VOICE_CHANNEL_START_TIME_UPDATE",
    // Scheduled events (eventos-programados).
    ScheduledEventCreate => "GUILD_SCHEDULED_EVENT_CREATE",
    ScheduledEventUpdate => "GUILD_SCHEDULED_EVENT_UPDATE",
    ScheduledEventDelete => "GUILD_SCHEDULED_EVENT_DELETE",
    ScheduledEventUserAdd => "GUILD_SCHEDULED_EVENT_USER_ADD",
    ScheduledEventUserRemove => "GUILD_SCHEDULED_EVENT_USER_REMOVE",
    // AutoMod (automod).
    AutoModRuleCreate => "AUTO_MODERATION_RULE_CREATE", AutoModRuleUpdate => "AUTO_MODERATION_RULE_UPDATE",
    AutoModRuleDelete => "AUTO_MODERATION_RULE_DELETE",
    AutoModActionExecution => "AUTO_MODERATION_ACTION_EXECUTION",
}

#[derive(Clone)]
pub(crate) struct Gateway {
    #[cfg(test)]
    local: Option<String>,
    /// How long a test waits between attempts: 10 ms unless it says.
    #[cfg(test)]
    wait: Option<Duration>,
    /// The least time between two IDENTIFYs in a test: none unless it says.
    #[cfg(test)]
    gap: Duration,
}

#[derive(Default)]
struct Session {
    id: Option<Zeroizing<String>>,
    resume_url: Option<String>,
    sequence: Option<u64>,
    identified_at: Option<Instant>,
    ready_at: Option<Instant>,
    /// The member list subscription the service has from this session.
    members: Option<Watch>,
    /// When member list frames went out, in the last minute.
    list_frames: VecDeque<Instant>,
    /// When the list opened last should have synced by: past it, the
    /// subscription is sent again from nothing.
    list_sync: Option<Instant>,
    /// Times the list was sent again without a sync, for the next wait.
    list_retries: u32,
    /// The list those retries count for.
    list_target: Option<(rune_model::ServerId, String)>,
    /// The voice state the service was last told, in this session.
    voice: Option<VoiceWant>,
    // Private calls.
    /// The conversation's call last asked about in this connection (op 13).
    call_status: Option<(u64, u32)>,
    /// The presence the service was last told in this connection, and when
    /// the next may go.
    presence: Option<Own>,
    presence_at: Option<Instant>,
    /// The local activity told with that presence (P6).
    activity: Option<Value>,
    // Reads, search and remote members (P2).
    /// When the member lookups asked for go.
    lookup: Pacer,
    // Voice protocol (voz-protocolo).
    /// The servers asked in this connection when their voice channels went
    /// live (op 43).
    start_times: Vec<u64>,
}

impl Session {
    fn forget_resume(&mut self) {
        self.id = None;
        self.resume_url = None;
        self.sequence = None;
    }
}

enum Failure {
    Retry { fresh: bool },
    Stop(LoginError),
    Closed,
}

fn protocol_error() -> Failure {
    Failure::Stop(LoginError::new(es(
        "Discord envió un evento Gateway incompatible.",
    )))
}

fn socket_error(error: WsError) -> Failure {
    note!(
        GATEWAY,
        "socket failed: {}",
        crate::voice::diag::socket_error(&error)
    );
    match error {
        WsError::Capacity(_) => Failure::Stop(LoginError::new(es(
            "Un evento de Discord excedió el límite local.",
        ))),
        // A cut stream (no closing handshake, as after hours on a flaky
        // network) or a busy Gateway is worth the bounded retries.
        WsError::Protocol(_) => Failure::Retry { fresh: false },
        WsError::Http(response)
            if response.status().is_server_error() || response.status().as_u16() == 429 =>
        {
            Failure::Retry { fresh: false }
        }
        WsError::Http(_) | WsError::HttpFormat(_) | WsError::Utf8(_) => {
            // Includes rejected/redirected handshakes. Never expose the response
            // body, redirect target or websocket error through LoginError.
            Failure::Stop(LoginError::new(es("Discord rechazó la conexión Gateway.")))
        }
        _ => Failure::Retry { fresh: false },
    }
}

fn closed(code: Option<u16>) -> Failure {
    note!(
        GATEWAY,
        "closed by Discord: code {}",
        code.map_or_else(|| "none".into(), |code| code.to_string())
    );
    match code {
        Some(code @ 4004) => Failure::Stop(LoginError::gateway_close(
            es("Discord rechazó la sesión Gateway. Inicia sesión otra vez."),
            code,
        )),
        Some(code @ 4008) => Failure::Stop(LoginError::gateway_close(
            es("Discord limitó la conexión Gateway. Espera antes de volver a conectar."),
            code,
        )),
        Some(1000 | 1001 | 4007 | 4009) => Failure::Retry { fresh: true },
        Some(code @ (1002 | 1003 | 1007..=1009 | 4001..=4003 | 4005 | 4010..=4999)) => {
            Failure::Stop(LoginError::gateway_close(
                es("Discord cerró la conexión Gateway. El intento se detuvo."),
                code,
            ))
        }
        _ => Failure::Retry { fresh: false },
    }
}

/// Resume must stay on Discord's Gateway infrastructure. Query parameters are
/// replaced locally; userinfo, alternate ports, paths and fragments are rejected.
fn resume_url(value: &str) -> Result<String, Failure> {
    let mut url = url::Url::parse(value).map_err(|_| protocol_error())?;
    // The main host or a regional one (`gateway-us-east1-b.discord.gg`).
    let on_gateway = url.host_str().is_some_and(|host| {
        host == "gateway.discord.gg"
            || host.strip_suffix(".discord.gg").is_some_and(|label| {
                label.len() > 8
                    && label.starts_with("gateway-")
                    && label
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            })
    });
    let bare = url.scheme() == "wss"
        && url.path() == "/"
        && url.fragment().is_none()
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none();
    if !(on_gateway && bare) {
        return Err(protocol_error());
    }
    url.set_query(Some("v=10&encoding=json"));
    Ok(url.to_string())
}

/// Whether a member list update syncs rows of the watched server's list.
/// Its list may be named otherwise than computed: the server is enough.
fn synced(data: &Value, watch: Option<&Watch>) -> bool {
    watch.is_some_and(|watch| data["guild_id"].as_str() == Some(watch.server.to_string().as_str()))
        && data["ops"]
            .as_array()
            .is_some_and(|ops| ops.iter().any(|op| op["op"] == "SYNC"))
}

/// Room for `needed` member list frames at `now`, else when there will be.
fn list_room(sent: &mut VecDeque<Instant>, now: Instant, needed: usize) -> Result<(), Instant> {
    while sent
        .front()
        .is_some_and(|at| now.duration_since(*at) >= LIST_WINDOW)
    {
        sent.pop_front();
    }
    match sent.front() {
        Some(oldest) if sent.len() + needed > LIST_FRAMES => Err(*oldest + LIST_WINDOW),
        _ => Ok(()),
    }
}

fn jitter(max: u64) -> Result<u64, Failure> {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).map_err(|_| {
        Failure::Stop(LoginError::new(es(
            "No se pudo preparar el reintento de Gateway.",
        )))
    })?;
    Ok(u64::from_le_bytes(bytes) % max.max(1))
}

/// Discord's next backoff: `current` plus up to twice itself, capped.
fn next_backoff(current: u64) -> Result<u64, Failure> {
    Ok((current + jitter(2 * current)?).min(BACKOFF_MAX_MS))
}

/// A presence update (op 3, Discord's documented shape) for `own`: its
/// status, and its custom status as the activity of type 4 unless invisible.
/// The `local` activity of this device (P6) goes first, also unless invisible.
fn presence_update(own: &Own, local: Option<&Value>) -> Value {
    let mut activities = Vec::new();
    if let Some(local) = local.filter(|_| own.status != rune_model::Status::Invisible) {
        activities.push(local.clone());
    }
    if let Some(custom) = own
        .custom
        .as_ref()
        .filter(|_| own.status != rune_model::Status::Invisible)
    {
        let mut activity = serde_json::json!({"name": "Custom Status", "type": 4});
        if !custom.text.is_empty() {
            activity["state"] = custom.text.clone().into();
        }
        if let Some((id, name)) = &custom.server_emoji {
            activity["emoji"] = serde_json::json!({ "id": id.to_string(), "name": name });
        } else if let Some(emoji) = &custom.emoji {
            activity["emoji"] = serde_json::json!({ "name": emoji });
        }
        activities.push(activity);
    }
    serde_json::json!({
        "since": null,
        "activities": activities,
        "status": crate::settings_proto::status_name(own.status),
        "afk": false,
    })
}

fn emit(output: &mpsc::Sender<GatewayEvent>, event: GatewayEvent) -> Result<(), Failure> {
    // Never wait on the UI and starve heartbeat acknowledgements. Overload is
    // explicit and terminal, rather than silently losing ordered events.
    if output.max_capacity().saturating_sub(output.capacity()) >= 32 {
        return Err(Failure::Stop(LoginError::new(es(
            "La cola de eventos de Discord se llenó. Vuelve a conectar.",
        ))));
    }
    output.try_send(event).map_err(|error| match error {
        mpsc::error::TrySendError::Closed(_) => Failure::Closed,
        mpsc::error::TrySendError::Full(_) => Failure::Stop(LoginError::new(es(
            "La cola de eventos de Discord se llenó. Vuelve a conectar.",
        ))),
    })
}

#[derive(Deserialize)]
struct Packet {
    op: u8,
    #[serde(default)]
    d: Value,
    s: Option<u64>,
    t: Option<String>,
}

#[derive(Default)]
struct WriteBudget(VecDeque<Instant>);

impl WriteBudget {
    fn record(&mut self) -> Result<(), Failure> {
        let now = Instant::now();
        while self
            .0
            .front()
            .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(60))
        {
            self.0.pop_front();
        }
        // Leave headroom below the published 120 Gateway events / 60 seconds.
        // A peer heartbeat flood must not turn this client into a send loop.
        if self.0.len() >= 100 {
            return Err(Failure::Stop(LoginError::new(es(
                "Gateway solicitó demasiadas respuestas. La conexión se detuvo.",
            ))));
        }
        self.0.push_back(now);
        Ok(())
    }
}

fn packet(message: Message) -> Result<Option<(Packet, usize)>, Failure> {
    match message {
        Message::Text(text) => {
            let length = text.len();
            if length > MAX_PAYLOAD {
                return Err(protocol_error());
            }
            let value: Packet =
                serde_json::from_slice(text.as_bytes()).map_err(|_| protocol_error())?;
            if value.t.as_ref().is_some_and(|name| name.len() > 64) {
                return Err(protocol_error());
            }
            Ok(Some((value, length)))
        }
        Message::Close(frame) => Err(closed(frame.map(|frame| frame.code.into()))),
        Message::Ping(_) | Message::Pong(_) => Ok(None),
        // Compression is deliberately not negotiated. No unbounded inflater.
        _ => Err(protocol_error()),
    }
}

async fn send<T: Serialize>(
    socket: &mut Socket,
    budget: &mut WriteBudget,
    op: u8,
    data: T,
) -> Result<(), Failure> {
    #[derive(Serialize)]
    struct Outbound<T> {
        op: u8,
        d: T,
    }
    let encoded = Zeroizing::new(
        serde_json::to_string(&Outbound { op, d: data }).map_err(|_| protocol_error())?,
    );
    if encoded.len() > 4096 {
        return Err(protocol_error());
    }
    budget.record()?;
    // The websocket/TLS implementation owns a temporary transport copy. Our
    // serialization buffer is erased; no credential is retained in a JSON Value.
    timeout(
        IO_TIMEOUT,
        socket.send(Message::Text(encoded.to_string().into())),
    )
    .await
    .map_err(|_| Failure::Retry { fresh: false })?
    .map_err(socket_error)
}

impl Gateway {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(test)]
            local: None,
            #[cfg(test)]
            wait: None,
            #[cfg(test)]
            gap: Duration::ZERO,
        }
    }

    #[cfg(test)]
    pub(crate) fn local(value: &str) -> Self {
        let url = url::Url::parse(value).unwrap();
        assert_eq!(url.scheme(), "ws");
        assert!(matches!(url.host_str(), Some("127.0.0.1" | "[::1]")));
        assert!(url.username().is_empty() && url.password().is_none());
        Self {
            local: Some(url.to_string()),
            wait: None,
            gap: Duration::ZERO,
        }
    }

    /// The same, waiting `wait` between attempts as the service would.
    #[cfg(test)]
    pub(crate) fn waiting(self, wait: Duration) -> Self {
        Self {
            wait: Some(wait),
            ..self
        }
    }

    /// The same, keeping `gap` between two IDENTIFYs as the service asks.
    #[cfg(test)]
    pub(crate) fn spaced(self, gap: Duration) -> Self {
        Self { gap, ..self }
    }

    fn identify_gap(&self) -> Duration {
        #[cfg(test)]
        if self.test_mode() {
            return self.gap;
        }
        IDENTIFY_GAP
    }

    fn test_wait(&self) -> Duration {
        #[cfg(test)]
        {
            self.wait.unwrap_or(Duration::from_millis(10))
        }
        #[cfg(not(test))]
        {
            Duration::ZERO
        }
    }

    fn test_mode(&self) -> bool {
        #[cfg(test)]
        {
            self.local.is_some()
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    #[cfg(test)]
    pub(crate) async fn run(
        &self,
        secret: Arc<SessionSecret>,
        output: mpsc::Sender<GatewayEvent>,
    ) -> Result<(), LoginError> {
        self.run_hurried(secret, output, Arc::default()).await
    }

    /// [`Self::run`], told by `now` to stop waiting between attempts.
    #[cfg(test)]
    pub(crate) async fn run_hurried(
        &self,
        secret: Arc<SessionSecret>,
        output: mpsc::Sender<GatewayEvent>,
        now: Arc<Notify>,
    ) -> Result<(), LoginError> {
        self.run_watching(
            secret,
            output,
            watch::channel(None).1,
            watch::channel(None).1,
            watch::channel(None).1,
            watch::channel(None).1,
            watch::channel(None).1,
            watch::channel(None).1,
            watch::channel(None).1,
            now,
        )
        .await
    }

    /// Keep the member list subscription `members` asks for, the voice
    /// state `voice` asks for and the account's `presence` with its local
    /// `activity`, as they change; look up the members `lookups` names; ask
    /// about the conversation's call `calls` names (op 13), and when the
    /// voice rooms of the server `shown` went live (op 43). `now` cuts a
    /// wait between attempts short, when the person asks to try at once.
    pub(crate) async fn run_watching(
        &self,
        secret: Arc<SessionSecret>,
        output: mpsc::Sender<GatewayEvent>,
        mut members: watch::Receiver<Option<Watch>>,
        mut voice: watch::Receiver<Option<VoiceWant>>,
        mut presence: watch::Receiver<Option<Own>>,
        mut lookups: watch::Receiver<Option<Lookup>>,
        mut activity: watch::Receiver<Option<Value>>,
        mut calls: watch::Receiver<Option<(u64, u32)>>,
        mut shown: watch::Receiver<Option<u64>>,
        now: Arc<Notify>,
    ) -> Result<(), LoginError> {
        let mut state = Session::default();
        let mut failures = 0;
        let mut backoff_ms = BACKOFF_START_MS;
        // Whether the attempt under way was asked for by the person.
        let mut hurried = false;
        loop {
            if output.is_closed() {
                return Ok(());
            }
            state.ready_at = None;
            match self
                .connection(
                    &secret,
                    &output,
                    &mut state,
                    (
                        &mut members,
                        &mut voice,
                        &mut presence,
                        &mut lookups,
                        &mut activity,
                        &mut calls,
                        &mut shown,
                    ),
                    &now,
                )
                .await
            {
                Err(Failure::Closed) => return Ok(()),
                Err(Failure::Stop(error)) => {
                    note!(GATEWAY, "stopped: {error}");
                    return Err(error);
                }
                Err(Failure::Retry { fresh }) => {
                    note!(
                        GATEWAY,
                        "reconnecting ({})",
                        if fresh || state.id.is_none() {
                            "new session: a call there is asked for again"
                        } else {
                            "resume: a call there goes on"
                        }
                    );
                    if fresh {
                        state.forget_resume();
                    }
                    if state
                        .ready_at
                        .is_some_and(|at| at.elapsed() >= Duration::from_secs(60))
                    {
                        failures = 0;
                        backoff_ms = BACKOFF_START_MS;
                    }
                    // An attempt the person hurried that failed before READY
                    // leaves the backoff as it is: a few clicks on a dead
                    // network would only lengthen the wait.
                    let asked = std::mem::take(&mut hurried) && state.ready_at.is_none();
                    if !asked || failures == 0 {
                        failures += 1;
                        backoff_ms = next_backoff(backoff_ms)
                            .map_err(|_| LoginError::new(es("No se pudo preparar Gateway.")))?;
                    }
                    if failures > FRESH_AFTER {
                        state.forget_resume();
                    }
                    let backoff = if self.test_mode() {
                        self.test_wait()
                    } else {
                        Duration::from_millis(backoff_ms)
                    };
                    // Without a session to resume, the next attempt identifies.
                    let spacing = match state.identified_at {
                        Some(at) if state.id.is_none() => {
                            self.identify_gap().saturating_sub(at.elapsed())
                        }
                        _ => Duration::ZERO,
                    };
                    let earliest = Instant::now() + spacing;
                    let delay = backoff.max(spacing);
                    match emit(
                        &output,
                        GatewayEvent::Reconnecting {
                            attempt: failures,
                            retry_in_ms: delay.as_millis() as u64,
                        },
                    ) {
                        Ok(()) => {}
                        Err(Failure::Closed) => return Ok(()),
                        _ => {
                            return Err(LoginError::new(es(
                                "La cola de eventos de Discord se llenó. Vuelve a conectar.",
                            )))
                        }
                    }
                    // Only a wait under way is cut short, and only down to the
                    // spacing between IDENTIFYs: a click while connected
                    // leaves the next loss its full wait.
                    tokio::select! {
                        biased;
                        _ = output.closed() => return Ok(()),
                        _ = now.notified() => {
                            note!(GATEWAY, "trying again now, as asked");
                            hurried = true;
                            tokio::select! {
                                biased;
                                _ = output.closed() => return Ok(()),
                                _ = sleep_until(earliest) => {}
                            }
                        }
                        _ = sleep(delay) => {}
                    }
                }
                Ok(()) => return Ok(()),
            }
        }
    }

    async fn connection(
        &self,
        secret: &SessionSecret,
        output: &mpsc::Sender<GatewayEvent>,
        state: &mut Session,
        (members, voice, presence, lookups, activity, calls, shown): Wishes<'_>,
        now: &Notify,
    ) -> Result<(), Failure> {
        let address = state.resume_url.as_deref().unwrap_or(ENDPOINT);
        #[cfg(test)]
        let address = self.local.as_deref().unwrap_or(address);
        let mut request = address
            .into_client_request()
            .map_err(|_| protocol_error())?;
        crate::browser::socket_headers(&mut request);
        let config = WebSocketConfig::default()
            .read_buffer_size(64 * 1024)
            .write_buffer_size(0)
            .max_write_buffer_size(8192)
            .max_message_size(Some(MAX_PAYLOAD))
            .max_frame_size(Some(MAX_PAYLOAD));
        let (mut socket, _) = timeout(
            IO_TIMEOUT,
            connect_async_with_config(request, Some(config), true),
        )
        .await
        .map_err(|_| Failure::Retry { fresh: false })?
        .map_err(socket_error)?;
        let hello_deadline = Instant::now() + IO_TIMEOUT;
        let hello = loop {
            let frame = tokio::select! {
                biased; _ = output.closed() => return Err(Failure::Closed),
                _ = sleep_until(hello_deadline) => {
                    note!(GATEWAY, "no HELLO in {} s", IO_TIMEOUT.as_secs());
                    return Err(Failure::Retry { fresh: false });
                }
                frame = socket.next() => frame.ok_or(Failure::Retry { fresh: false })?.map_err(socket_error)?,
            };
            if let Some((packet, _)) = packet(frame)? {
                break packet;
            }
            timeout(IO_TIMEOUT, socket.flush())
                .await
                .map_err(|_| Failure::Retry { fresh: false })?
                .map_err(socket_error)?;
        };
        let interval_ms = hello
            .d
            .get("heartbeat_interval")
            .and_then(Value::as_u64)
            .ok_or_else(protocol_error)?;
        let minimum = if self.test_mode() { 10 } else { 1000 };
        if hello.op != 10 || !(minimum..=300_000).contains(&interval_ms) {
            return Err(protocol_error());
        }
        let interval = Duration::from_millis(interval_ms);
        let initial = if self.test_mode() {
            0
        } else {
            jitter(interval_ms)?
        };
        let mut next_heartbeat = Instant::now() + Duration::from_millis(initial);
        let mut awaiting_ack: Option<Instant> = None;
        // Set by an expedited heartbeat: past it, an ACK still missing ends the connection.
        let mut expedited: Option<Instant> = None;
        let ready_deadline = Instant::now() + READY_TIMEOUT;
        let mut writes = WriteBudget::default();
        let resuming = state.id.is_some() && state.sequence.is_some();
        let mut watching = true;
        let mut voicing = true;
        let mut presencing = true;
        let mut activitying = true;
        let mut calling = true;
        let mut showing = true;
        // A new connection is told the presence again, once it is ready.
        state.presence = None;
        state.activity = None;
        // And asked about the conversation's call read last, as the official
        // client asks about the one open (op 13).
        state.call_status = None;
        // And when the voice channels shown went live (op 43), as it asks
        // again on each connection.
        state.start_times.clear();
        let mut list_at = Instant::now();
        let list_gap = if self.test_mode() {
            Duration::ZERO
        } else {
            LIST_GAP
        };
        let list_wait = if self.test_mode() {
            Duration::from_millis(300)
        } else {
            LIST_SYNC_WAIT
        };
        // A lookup not answered on the last connection goes again.
        let mut looking = true;
        state.lookup.restart();
        let (settle, gap) = if self.test_mode() {
            (Duration::ZERO, Duration::ZERO)
        } else {
            (member_search::SETTLE, member_search::GAP)
        };
        if let (Some(id), Some(seq)) = (state.id.as_ref(), state.sequence) {
            #[derive(Serialize)]
            struct Resume<'a> {
                token: &'a str,
                session_id: &'a str,
                seq: u64,
            }
            send(
                &mut socket,
                &mut writes,
                6,
                Resume {
                    token: &secret.0,
                    session_id: id,
                    seq,
                },
            )
            .await?;
        } else {
            #[derive(Serialize)]
            struct Identify<'a> {
                token: &'a str,
                properties: crate::browser::Identify,
                compress: bool,
                /// The status the account starts with, so an invisible one
                /// never shows online; the custom status follows the READY.
                #[serde(skip_serializing_if = "Option::is_none")]
                presence: Option<Value>,
            }
            state.identified_at = Some(Instant::now());
            let starting_own = presence.borrow().clone().map(|mut own| {
                own.custom = None;
                own
            });
            let starting = starting_own.as_ref().map(|own| presence_update(own, None));
            send(
                &mut socket,
                &mut writes,
                2,
                Identify {
                    token: &secret.0,
                    properties: crate::browser::identify(),
                    compress: false,
                    presence: starting,
                },
            )
            .await?;
            state.presence = starting_own;
        }
        loop {
            // The provider polls this future and its event receiver in the
            // same select. Buffered socket frames can all be immediately
            // ready, so explicitly let that receiver drain between frames.
            // This retains the hard queue bound for genuinely slow consumers.
            tokio::task::yield_now().await;
            tokio::select! {
                biased;
                _ = output.closed() => return Err(Failure::Closed),
                _ = sleep_until(ready_deadline), if state.ready_at.is_none() => {
                    note!(GATEWAY, "no READY or RESUMED in {} s", READY_TIMEOUT.as_secs());
                    return Err(Failure::Retry { fresh: false });
                }
                _ = sleep_until(next_heartbeat) => {
                    if awaiting_ack.is_some_and(|at| at.elapsed() >= interval) {
                        note!(GATEWAY, "heartbeat not acknowledged in {} ms", interval.as_millis());
                        return Err(Failure::Retry { fresh: false });
                    }
                    send(&mut socket, &mut writes, 1, state.sequence).await?;
                    awaiting_ack.get_or_insert(Instant::now());
                    next_heartbeat = Instant::now() + interval;
                }
                // Asked after a network change (or by the person) once READY:
                // a heartbeat now, as Discord's client does. Before READY the
                // ask waits for the next wait between attempts instead.
                _ = now.notified(), if state.ready_at.is_some() => {
                    note!(GATEWAY, "heartbeat now, as asked");
                    send(&mut socket, &mut writes, 1, state.sequence).await?;
                    awaiting_ack.get_or_insert(Instant::now());
                    expedited = Some(Instant::now() + EXPEDITED_ACK);
                }
                _ = sleep_until(expedited.unwrap_or_else(Instant::now)), if expedited.is_some() => {
                    expedited = None;
                    if awaiting_ack.is_some() {
                        note!(GATEWAY, "expedited heartbeat not acknowledged");
                        return Err(Failure::Retry { fresh: false });
                    }
                }
                changed = members.changed(), if watching => watching = changed.is_ok(),
                changed = voice.changed(), if voicing => voicing = changed.is_ok(),
                changed = presence.changed(), if presencing => presencing = changed.is_ok(),
                changed = activity.changed(), if activitying => activitying = changed.is_ok(),
                changed = calls.changed(), if calling => calling = changed.is_ok(),
                changed = shown.changed(), if showing => showing = changed.is_ok(),
                // The latest presence only, paced: a burst of changes sends one.
                _ = sleep_until(state.presence_at.unwrap_or_else(Instant::now)), if state.ready_at.is_some()
                    && presence.borrow().is_some()
                    && (*presence.borrow() != state.presence || *activity.borrow() != state.activity) => {
                    let want = presence.borrow().clone();
                    let local = activity.borrow().clone();
                    if let Some(own) = &want {
                        send(&mut socket, &mut writes, 3, presence_update(own, local.as_ref())).await?;
                    }
                    state.presence = want;
                    state.activity = local;
                    state.presence_at = Some(Instant::now() + if self.test_mode() { Duration::from_millis(50) } else { PRESENCE_GAP });
                }
                // Member lookups (op 8): the newest wish, once typing settles.
                changed = lookups.changed(), if looking => {
                    looking = changed.is_ok();
                    state.lookup.changed(Instant::now());
                }
                _ = sleep_until(state.lookup.due(Instant::now(), settle, gap)),
                    if state.ready_at.is_some() && state.lookup.wants(lookups.borrow().as_ref()) => {
                    let want = lookups.borrow().clone();
                    if let Some(want) = want {
                        let frame = state.lookup.send(&want, Instant::now());
                        send(&mut socket, &mut writes, 8, frame).await?;
                    }
                }
                // Private calls: what the service knows of a conversation's
                // call, asked before joining it (so before op 4).
                _ = std::future::ready(()), if state.ready_at.is_some() && *calls.borrow() != state.call_status => {
                    let want = *calls.borrow();
                    if let Some((channel, _)) = want {
                        #[derive(Serialize)]
                        struct CallConnect {
                            channel_id: String,
                        }
                        note!(GATEWAY, "op 13 sent: ask what the service knows of a conversation's call");
                        send(&mut socket, &mut writes, 13, CallConnect { channel_id: channel.to_string() }).await?;
                    }
                    state.call_status = want;
                }
                // Voice protocol (voz-protocolo): the server shown, as the
                // official client asks for each server whose channels show.
                _ = std::future::ready(()), if state.ready_at.is_some()
                    && shown.borrow().is_some_and(|guild| state.start_times_wanted(guild)) => {
                    let guild = *shown.borrow();
                    if let Some(guild) = guild {
                        ask_start_times(&mut socket, &mut writes, state, guild).await?;
                    }
                }
                // The latest wish only: a burst of clicks sends one update.
                _ = std::future::ready(()), if state.ready_at.is_some() && *voice.borrow() != state.voice => {
                    let want = *voice.borrow();
                    if let Some(want) = want.filter(|want| Some(want.state()) != state.voice.map(|told| told.state())) {
                        #[derive(Serialize)]
                        struct VoiceUpdate {
                            /// Null for a conversation's call.
                            guild_id: Option<String>,
                            channel_id: Option<String>,
                            self_mute: bool,
                            self_deaf: bool,
                            self_video: bool,
                        }
                        note!(
                            GATEWAY,
                            "op 4 sent: {} (self_mute {}, self_deaf {}, self_video {})",
                            if want.channel.is_some() { "join or stay in a voice channel" } else { "leave voice" },
                            want.muted,
                            want.deafened,
                            want.camera
                        );
                        send(&mut socket, &mut writes, 4, VoiceUpdate {
                            guild_id: want.guild.map(|guild| guild.to_string()),
                            channel_id: want.channel.map(|id| id.to_string()),
                            self_mute: want.muted,
                            self_deaf: want.deafened,
                            self_video: want.camera,
                        }).await?;
                        // Voice protocol (voz-protocolo): the call's server, as Serein asks.
                        if let (Some(guild), Some(_)) = (want.guild, want.channel) {
                            ask_start_times(&mut socket, &mut writes, state, guild).await?;
                        }
                    }
                    // Then the streams, once the call is asked for: those
                    // watched, and the account's own. Letting go comes first.
                    #[derive(Serialize)]
                    struct Stream<'a> {
                        stream_key: &'a str,
                    }
                    #[derive(Serialize)]
                    struct CreateStream {
                        r#type: &'static str,
                        guild_id: Option<String>,
                        channel_id: Option<String>,
                        preferred_region: Option<String>,
                    }
                    let keys = |want: Option<VoiceWant>| -> Vec<(String, u32, bool)> {
                        let Some(want) = want else { return Vec::new() };
                        let mut keys: Vec<_> = want.stream_keys().into_iter().map(|(key, request)| (key, request, false)).collect();
                        keys.extend(want.share_key().map(|key| (key, want.share_request, true)));
                        keys
                    };
                    let (told, wanted) = (keys(state.voice), keys(want));
                    for (old, ..) in told.iter().filter(|(old, ..)| wanted.iter().all(|(new, ..)| new != old)) {
                        note!(GATEWAY, "op 19 sent: let go of a stream");
                        send(&mut socket, &mut writes, 19, Stream { stream_key: old }).await?;
                    }
                    for (new, request, own) in &wanted {
                        match (told.iter().find(|(old, ..)| old == new), want) {
                            (Some((_, before, _)), _) if before == request => {}
                            (Some(_), _) => {
                                note!(GATEWAY, "op 21 sent: ask for a stream again");
                                send(&mut socket, &mut writes, 21, Stream { stream_key: new }).await?
                            }
                            (None, Some(want)) if *own => {
                                note!(GATEWAY, "op 18 sent: create the account's stream");
                                send(&mut socket, &mut writes, 18, CreateStream {
                                    // A conversation's call streams as a call.
                                    r#type: if want.guild.is_some() { "guild" } else { "call" },
                                    guild_id: want.guild.map(|guild| guild.to_string()),
                                    channel_id: want.channel.map(|id| id.to_string()),
                                    preferred_region: None,
                                }).await?;
                            }
                            (None, _) => {
                                note!(GATEWAY, "op 20 sent: watch a stream");
                                send(&mut socket, &mut writes, 20, Stream { stream_key: new }).await?
                            }
                        }
                    }
                    // The account's stream paused or resumed (op 22), once created.
                    if let Some((key, paused)) = want.and_then(|want| Some((want.share_key()?, want.share_paused))) {
                        #[derive(Serialize)]
                        struct Pause<'a> {
                            stream_key: &'a str,
                            paused: bool,
                        }
                        let before = state.voice.filter(|told| told.share_key().as_deref() == Some(key.as_str()));
                        if before.is_some_and(|told| told.share_paused) != paused {
                            note!(GATEWAY, "op 22 sent: stream paused {paused}");
                            send(&mut socket, &mut writes, 22, Pause { stream_key: &key, paused }).await?;
                        }
                    }
                    state.voice = want;
                }
                // A list that never synced is sent again from nothing, waiting
                // longer each time, as Serein does (15, 30, 60, then 120 s).
                _ = sleep_until(state.list_sync.unwrap_or(list_at)), if state.ready_at.is_some() && state.list_sync.is_some() => {
                    state.list_sync = None;
                    state.list_retries = state.list_retries.saturating_add(1);
                    state.members = None;
                }
                _ = sleep_until(list_at), if state.ready_at.is_some() && *members.borrow() != state.members => {
                    let want = members.borrow().clone();
                    let frames = crate::members::frames(state.members.as_ref(), want.as_ref());
                    let now = Instant::now();
                    match list_room(&mut state.list_frames, now, frames.len()) {
                        Err(room) => list_at = room,
                        Ok(()) => {
                            for frame in frames {
                                send(&mut socket, &mut writes, 37, frame).await?;
                                state.list_frames.push_back(now);
                            }
                            match &want {
                                None => state.list_sync = None,
                                Some(want) if state.members.as_ref().is_none_or(|sent| sent.server != want.server || sent.list != want.list) => {
                                    let target = (want.server, want.list.clone());
                                    if state.list_target.as_ref() != Some(&target) {
                                        state.list_retries = 0;
                                        state.list_target = Some(target);
                                    }
                                    state.list_sync = Some(now + list_wait.saturating_mul(1 << state.list_retries.min(3)));
                                }
                                Some(_) => {}
                            }
                            state.members = want;
                            list_at = now + list_gap;
                        }
                    }
                }
                frame = socket.next() => {
                    let frame = frame.ok_or(Failure::Retry { fresh: false })?.map_err(socket_error)?;
                    let Some((packet, length)) = packet(frame)? else {
                        timeout(IO_TIMEOUT, socket.flush()).await.map_err(|_| Failure::Retry { fresh: false })?.map_err(socket_error)?;
                        continue;
                    };
                    match packet.op {
                        0 => {
                            let sequence = packet.s.ok_or_else(protocol_error)?;
                            if state.sequence.is_some_and(|previous| sequence <= previous) { continue; }
                            let name = packet.t.as_deref().ok_or_else(protocol_error)?;
                            if name == "READY" {
                                let mut data = packet.d;
                                let object = data.as_object_mut().ok_or_else(protocol_error)?;
                                let Some(Value::String(id)) = object.remove("session_id") else { return Err(protocol_error()); };
                                if id.is_empty() || id.len() > 512 || !id.bytes().all(|byte| byte.is_ascii_graphic()) { return Err(protocol_error()); }
                                let Some(Value::String(url)) = object.remove("resume_gateway_url") else { return Err(protocol_error()); };
                                let url = resume_url(&url)?;
                                // These are never domain bootstrap data, even if a
                                // future server includes one in its READY payload.
                                object.remove("token");
                                let id = Zeroizing::new(id);
                                emit(output, GatewayEvent::Session(id.clone()))?;
                                state.id = Some(id);
                                state.resume_url = Some(url);
                                // A new session has no subscription yet. Its call is
                                // asked for again only if the provider wants it
                                // (`Calls::ready`): it may have gone to another device.
                                state.members = None;
                                state.list_sync = None;
                                state.list_retries = 0;
                                note!(GATEWAY, "READY: new session");
                                emit(output, GatewayEvent::Ready(data))?;
                                state.ready_at = Some(Instant::now());
                            } else if name == "RESUMED" {
                                if !resuming { return Err(protocol_error()); }
                                note!(GATEWAY, "RESUMED");
                                emit(output, GatewayEvent::Resumed)?;
                                state.ready_at = Some(Instant::now());
                            } else {
                                if state.id.is_none() { return Err(protocol_error()); }
                                if length > MAX_DISPATCH { return Err(Failure::Stop(LoginError::new(es("Un evento de Discord excedió el límite local.")))); }
                                if name == "GUILD_MEMBER_LIST_UPDATE" && synced(&packet.d, state.members.as_ref()) {
                                    state.list_sync = None;
                                    state.list_retries = 0;
                                }
                                if let Some(kind) = DispatchKind::parse(name) {
                                    emit(output, GatewayEvent::Dispatch { kind, data: packet.d })?;
                                }
                            }
                            state.sequence = Some(sequence);
                        }
                        1 => {
                            send(&mut socket, &mut writes, 1, state.sequence).await?;
                            awaiting_ack.get_or_insert(Instant::now());
                        }
                        7 => {
                            note!(GATEWAY, "Discord asked to reconnect (op 7)");
                            return Err(Failure::Retry { fresh: false });
                        }
                        9 => {
                            let resumable = packet.d.as_bool().ok_or_else(protocol_error)?;
                            note!(GATEWAY, "invalid session (op 9), resumable {resumable}");
                            return Err(Failure::Retry { fresh: !resumable });
                        }
                        11 => awaiting_ack = None,
                        _ => return Err(protocol_error()),
                    }
                }
            }
        }
    }
}

// Voice protocol (voz-protocolo).
impl Session {
    /// Whether `guild` was not asked yet in this connection when its voice
    /// channels went live; bounded by the servers an account can be in,
    /// and more.
    fn start_times_wanted(&self, guild: u64) -> bool {
        !self.start_times.contains(&guild) && self.start_times.len() < 512
    }
}

/// Ask, once a connection, when `guild`'s voice channels went live (op 43,
/// `REQUEST_CHANNEL_INFO` in the official client's bundle, which asks for
/// each server shown; Serein for its call's). `CHANNEL_INFO` answers.
async fn ask_start_times(
    socket: &mut Socket,
    writes: &mut WriteBudget,
    state: &mut Session,
    guild: u64,
) -> Result<(), Failure> {
    if !state.start_times_wanted(guild) {
        return Ok(());
    }
    state.start_times.push(guild);
    #[derive(Serialize)]
    struct ChannelInfo {
        guild_id: String,
        fields: [&'static str; 1],
    }
    note!(
        GATEWAY,
        "op 43 sent: when the server's voice channels went live"
    );
    send(
        socket,
        writes,
        43,
        ChannelInfo {
            guild_id: guild.to_string(),
            fields: ["voice_start_time"],
        },
    )
    .await
}

#[cfg(test)]
#[path = "gateway_tests.rs"]
mod tests;
