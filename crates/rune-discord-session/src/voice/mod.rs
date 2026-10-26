//! Voice: who is in which channel, and the account's own call.
//!
//! The main Gateway asks to join, move or leave (op 4) and answers with the
//! account's voice state (its voice session) and the voice server (token and
//! endpoint). With both, [`connection::run`] reaches the voice server. The
//! routes and messages are the ones Serein uses (owner-authorized
//! 2026-10-16). A direct or group conversation's call (2026-10-17) goes the
//! same way, without a server: see [`private`].
//!
//! Watching someone's screen in the call goes through the main Gateway too:
//! op 20 asks for the stream (op 19 lets go, op 21 asks again after it
//! dropped), `STREAM_CREATE` names its own voice server and
//! `STREAM_SERVER_UPDATE` its token and endpoint; a [`connection::run`] of
//! its own then carries its frames and sound. `STREAM_DELETE` ends it, or
//! means reconnecting when `unavailable`. Up to [`MAX_STREAMS`] are watched
//! at once, each on its connection; one more lets go of the oldest.
//!
//! Sharing the account's screen goes the same way: op 18 creates its stream
//! in the call's channel, the same `STREAM_CREATE` and
//! `STREAM_SERVER_UPDATE` (under its own key) name its server, and a
//! [`connection::run`] that sends carries it; op 22 pauses and resumes it,
//! op 19 lets go of it.
//!
//! The others' cameras come over the call's own connection, and so does the
//! account's: op 4 says it is on (`self_video`), and the connection
//! announces it (op 12) and sends it. The host's
//! [`VideoRequest`]s arrive here and go to the connection they are about:
//! key frames of the screen to the stream's, those of cameras and which
//! cameras are on screen (at most [`MAX_CAMERAS`]) to the call's.

pub(crate) mod connection;
mod dave;
pub(crate) mod diag;
mod h264;
mod nack;
// Private calls.
pub(crate) mod private;
mod rtp;
mod send;
mod sps;
#[cfg(test)]
mod tests;

use rune_i18n::{es, t};
use std::time::Duration;

use rune_model::{
    CallPhase, CallQuality, ChannelId, ServerId, StreamPhase, StreamQuality, Timestamp, UserId,
    VideoKind, VoiceState,
};
use rune_provider::{
    video::{VideoPort, VideoRequest, CAMERA, MAX_CAMERAS, MAX_STREAMS, REQUESTS},
    voice::VoicePort,
    BoxFuture, Event, Snapshot,
};
use serde_json::Value;
use tokio::{
    sync::{mpsc, oneshot, watch, Mutex, OwnedMutexGuard},
    time::{sleep_until, Instant},
};
use zeroize::Zeroizing;

use crate::{wire, LoginError};
use connection::{Codes, Controls, Credentials, Outgoing, Video};
use diag::note;

/// The scope of the call's diagnostic lines ([`diag`]).
const CALL: &str = "voice call";
/// Of the streams': watched, and the account's own.
const STREAM: &str = "voice stream";
const SHARE: &str = "voice share";

/// Waiting for the voice server after asking to join, or for a stream's.
const JOIN_TIMEOUT: Duration = Duration::from_secs(15);
/// Waits before asking the service for the call again once its connection
/// could not be resumed, as Discord's client does. Then the call ends.
const REJOIN_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(3),
    Duration::from_secs(10),
];
/// No voice server came in time: the Gateway may be away, so it is waited for.
pub(crate) const NO_SERVER: &str = es("Discord no asignó un servidor de voz a tiempo.");
/// Connected this long, a later loss starts from the first attempt again.
const REJOIN_STABLE: Duration = Duration::from_secs(30);
/// The account's voice went on with another session: another device.
pub(crate) const TAKEN_OVER: &str = es("Tu llamada siguió en otro dispositivo.");
/// Voice states kept for the whole account.
pub(crate) const MAX_STATES: usize = 4096;
/// Channels of one server whose start time is read (a server has at most 500).
const MAX_START_TIMES: usize = 500;

/// What the main Gateway should tell the service (op 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VoiceWant {
    /// `None` for a direct or group conversation's call.
    pub guild: Option<u64>,
    /// `None` leaves.
    pub channel: Option<u64>,
    pub muted: bool,
    pub deafened: bool,
    /// Changes to ask again for the same call: a rejoin.
    pub request: u32,
    /// Whose streams to watch in that channel (op 20), each with what
    /// changes to ask for it again after it dropped (op 21).
    pub streams: [Option<(u64, u32)>; MAX_STREAMS],
    /// The account, while it shares its screen in that channel (op 18).
    pub share: Option<u64>,
    /// Changes to ask again for the account's stream after it dropped.
    pub share_request: u32,
    /// The account's camera is on in that channel (op 4 `self_video`).
    pub camera: bool,
    /// The account paused its stream (op 22).
    pub share_paused: bool,
}

impl VoiceWant {
    /// What op 4 says; the stream goes in its own messages.
    pub(crate) fn state(&self) -> (Option<u64>, Option<u64>, bool, bool, bool, u32) {
        (
            self.guild,
            self.channel,
            self.muted,
            self.deafened,
            self.camera,
            self.request,
        )
    }

    /// The keys of the streams to watch, `guild:{server}:{channel}:{owner}`
    /// (`call:{channel}:{owner}` in a conversation's call), each with what
    /// changes to ask for it again.
    pub(crate) fn stream_keys(&self) -> Vec<(String, u32)> {
        self.streams
            .iter()
            .flatten()
            .filter_map(|(owner, request)| Some((self.key(*owner)?, *request)))
            .collect()
    }

    /// The key of the account's own stream, while it shares its screen.
    pub(crate) fn share_key(&self) -> Option<String> {
        self.key(self.share?)
    }

    /// The key of `owner`'s stream in this call, as the official client
    /// names it.
    fn key(&self, owner: u64) -> Option<String> {
        let channel = self.channel?;
        Some(match self.guild {
            Some(guild) => format!("guild:{guild}:{channel}:{owner}"),
            None => format!("call:{channel}:{owner}"),
        })
    }
}

/// Why Discord ended a stream (the account's own, `own`), as the interface
/// says it; `None` when its owner stopped it or the account let go.
fn stream_ended(reason: Option<&str>, own: bool) -> Option<String> {
    Some(
        match reason? {
            "user_requested" => return None,
            "stream_full" => t("La transmisión está llena."),
            "unauthorized" if own => t("Discord no te dejó transmitir en este canal."),
            "unauthorized" => t("No tienes permiso para ver esta transmisión."),
            "safety_guild_rate_limited" => t("Discord limitó las transmisiones de este servidor."),
            _ => t("La transmisión terminó."),
        }
        .into(),
    )
}

/// One person's voice state as the service sends it, or `None` (with the
/// user) when they are in no channel. Malformed states are skipped: they
/// are decoration, never a reason to disconnect.
fn state(data: &Value) -> Option<(UserId, Option<VoiceState>)> {
    let user = UserId(wire::id(&data["user_id"]).ok()?);
    let flag = |key: &str| data[key].as_bool().unwrap_or(false);
    let Some(channel) = data["channel_id"]
        .as_str()
        .filter(|_| !data["channel_id"].is_null())
    else {
        return Some((user, None));
    };
    let channel = ChannelId(channel.parse().ok().filter(|id| *id != 0)?);
    let deafened = flag("self_deaf");
    // A stage's hand raised or invitation; a malformed one reads as none.
    let speak_request = crate::wire::timestamp(&data["request_to_speak_timestamp"]).ok();
    Some((
        user,
        Some(VoiceState {
            user,
            channel,
            muted: flag("self_mute") || deafened,
            deafened,
            speaking: false,
            streaming: flag("self_stream"),
            // `suppress`: not allowed to speak there (an AFK room, a stage's
            // audience); invited to a stage, not until they accept, as the
            // official client holds the microphone then.
            server_muted: flag("mute") || flag("suppress") || speak_request.is_some(),
            server_deafened: flag("deaf"),
            camera: flag("self_video"),
            suppressed: flag("suppress"),
            speak_request,
        }),
    ))
}

/// The voice session of a voice state, if it is a plausible one.
fn session(data: &Value) -> Option<&str> {
    data["session_id"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 256 && id.bytes().all(|b| b.is_ascii_graphic()))
}

// Voice protocol (voz-protocolo).
/// Whether the voice states `rows` show the account `me` in a channel with
/// another voice session than `ours`, nor the main Gateway's `gateway` (this
/// device's, asked for since): its voice went on with another device.
fn elsewhere<'a>(
    rows: impl Iterator<Item = &'a Value>,
    me: UserId,
    ours: &str,
    gateway: Option<&str>,
) -> bool {
    rows.take(MAX_STATES).any(|row| {
        wire::id(&row["user_id"]).ok() == Some(me.0)
            && !row["channel_id"].is_null()
            && session(row).is_some_and(|theirs| theirs != ours && Some(theirs) != gateway)
    })
}

/// Since when voice channels went live: the answer to op 43 for a whole
/// server (`CHANNEL_INFO`), or one channel's change. The time is in seconds
/// since 1970, null when nobody is in it; a malformed row is skipped.
fn start_times(kind: &str, data: &Value) -> Vec<Event> {
    let Ok(server) = wire::id(&data["guild_id"]) else {
        return Vec::new();
    };
    let time = |row: &Value| {
        let channel = ChannelId(wire::id(&row["id"]).ok()?);
        // Null for nobody; anything but a time from Discord's epoch (2015)
        // to 2100 is malformed, and leaves what was known as it was.
        let since = match &row["voice_start_time"] {
            Value::Null => None,
            time => Some(
                time.as_u64()
                    .filter(|seconds| (1_420_070_400..4_102_444_800).contains(seconds))
                    .map(|seconds| Timestamp(seconds as i64 * 1000))?,
            ),
        };
        Some((channel, since))
    };
    let (times, whole) = match kind {
        "CHANNEL_INFO" => (
            data["channels"]
                .as_array()
                .into_iter()
                .flatten()
                .take(MAX_START_TIMES)
                .filter_map(time)
                .collect(),
            true,
        ),
        _ => (time(data).into_iter().collect(), false),
    };
    vec![Event::VoiceStartTimes {
        server: ServerId(server),
        times,
        whole,
    }]
}

/// The voice states a guild arrives with.
pub(crate) fn guild_states(snapshot: &mut Snapshot, data: &Value) {
    let Some(rows) = data["voice_states"].as_array() else {
        return;
    };
    for row in rows.iter().take(MAX_STATES) {
        if let Some((user, Some(state))) = state(row) {
            snapshot.voice.retain(|known| known.user != user);
            if snapshot.voice.len() < MAX_STATES {
                snapshot.voice.push(state);
            }
        }
    }
}

/// Someone's stream the account watches in its call, or its own.
struct Watch {
    owner: u64,
    /// How the account's own is sent; `None` for someone else's.
    quality: Option<StreamQuality>,
    /// Bumped to ask for the stream again (op 21).
    request: u32,
    /// Its own voice server: `rtc_server_id`, and the channel its
    /// end-to-end group is for.
    rtc: Option<(u64, u64)>,
    server: Option<(Zeroizing<String>, String)>,
    connection: Option<BoxFuture<'static, Result<(), &'static str>>>,
    phase: Option<watch::Receiver<CallPhase>>,
    /// Its connection's key frame requests.
    keys: Option<mpsc::Sender<u64>>,
    /// Until its voice server is known.
    deadline: Option<Instant>,
    /// Paused, as the service says.
    paused: bool,
    /// The account paused its own.
    pause: bool,
}

impl Watch {
    fn new(owner: u64) -> Self {
        Self {
            owner,
            quality: None,
            request: 0,
            rtc: None,
            server: None,
            connection: None,
            phase: None,
            keys: None,
            deadline: Some(Instant::now() + JOIN_TIMEOUT),
            paused: false,
            pause: false,
        }
    }

    /// Its connection and keys go; the service hands out new ones.
    fn forget(&mut self) {
        self.rtc = None;
        self.server = None;
        self.connection = None;
        self.phase = None;
        self.deadline = Some(Instant::now() + JOIN_TIMEOUT);
    }
}

/// What the connection of the stream in `slot` has to say next; pending
/// without one.
async fn progress(slot: &mut Option<Watch>) -> Watched {
    match slot.as_mut() {
        Some(watch) => next_of(watch).await,
        None => std::future::pending().await,
    }
}

/// What the connection of one of `watches` has to say next, and whose
/// stream it is; pending without any.
async fn progress_any(watches: &mut [Watch]) -> (u64, Watched) {
    if watches.is_empty() {
        return std::future::pending().await;
    }
    let next = watches
        .iter_mut()
        .map(|watch| Box::pin(async move { (watch.owner, next_of(watch).await) }));
    futures_util::future::select_all(next).await.0
}

/// What the connection of `watch` has to say next.
async fn next_of(watch: &mut Watch) -> Watched {
    tokio::select! {
        _ = async {
            match watch.deadline { Some(deadline) => sleep_until(deadline).await, None => std::future::pending().await }
        } => Watched::Late,
        changed = async {
            match watch.phase.as_mut() { Some(phase) => phase.changed().await.map(|()| phase.borrow_and_update().clone()).ok(), None => std::future::pending().await }
        } => Watched::Phase(changed),
        result = async {
            match watch.connection.as_mut() { Some(connection) => connection.await, None => std::future::pending().await }
        } => Watched::Ended(result),
    }
}

/// What a watched stream's connection had to say.
enum Watched {
    /// Its voice server never came.
    Late,
    /// A new phase; `None` when its connection went.
    Phase(Option<CallPhase>),
    Ended(Result<(), &'static str>),
}

struct Call {
    /// `None` for a direct or group conversation's call.
    guild: Option<u64>,
    channel: ChannelId,
    /// The streams watched in it, the oldest first, and the account's own.
    watches: Vec<Watch>,
    share: Option<Watch>,
    controls: watch::Sender<Controls>,
    /// Whose cameras the host shows, at most [`MAX_CAMERAS`].
    cameras: watch::Sender<Vec<u64>>,
    /// The connection's key frame requests, for cameras.
    keys: Option<mpsc::Sender<u64>>,
    session: Option<Zeroizing<String>>,
    /// The voice token and validated address, once the server answered.
    server: Option<(Zeroizing<String>, String)>,
    connection: Option<BoxFuture<'static, Result<(), &'static str>>>,
    phase: Option<watch::Receiver<CallPhase>>,
    /// Until the voice server is known.
    deadline: Option<Instant>,
    /// It was connected at some point: losing it rejoins.
    joined: bool,
    /// Since when it is connected, without a loss after.
    connected_at: Option<Instant>,
    /// Rejoins spent since it was last connected for a while.
    rejoins: u32,
    /// When to ask the service for the call again.
    rejoin_at: Option<Instant>,
    /// Waiting for the main Gateway to come back before asking again.
    awaiting_gateway: bool,
    // Desktop, language and local voice (P6).
    /// The connection's privacy code, as it changes.
    privacy: Option<watch::Receiver<Option<String>>>,
    /// Asks the connection for someone's verification code.
    verify: Option<mpsc::Sender<(u64, oneshot::Sender<Option<String>>)>>,
    /// The one asked for last, until it answers.
    verifying: Option<(UserId, oneshot::Receiver<Option<String>>)>,
    /// This device's voice server accepted `session` (its connection got
    /// past Identify). Until then a newer session from the service is a
    /// better candidate, and states of another channel or none are stale
    /// (Serein's rule: Discord may first describe another client); after,
    /// another session is another device's.
    confirmed: bool,
    // Voice protocol (voz-protocolo).
    /// What the connection measures: round trip and packets lost.
    quality: Option<watch::Receiver<Option<CallQuality>>>,
    /// After a new Gateway session whose READY had the call's server
    /// unavailable: op 4 waits for its `GUILD_CREATE` until then, which may
    /// show the account's voice on another device (the session accepted
    /// before, to compare).
    guild_wait: Option<(Instant, Option<Zeroizing<String>>)>,
}

impl Call {
    /// The account's own stream, or the one of `owner` watched.
    fn slot(&mut self, own: bool, owner: u64) -> Option<&mut Watch> {
        if own {
            self.share.as_mut()
        } else {
            self.watches.iter_mut().find(|watch| watch.owner == owner)
        }
    }

    /// The connection and its keys go; the session and server stay.
    fn drop_connection(&mut self) {
        self.connection = None;
        self.phase = None;
        self.deadline = None;
        self.privacy = None;
        self.verify = None;
        self.verifying = None;
        self.quality = None;
    }

    /// Forget the voice session, server and connection (and its keys): the
    /// service hands out new ones when asked again.
    fn forget(&mut self) {
        self.session = None;
        self.server = None;
        self.connection = None;
        self.phase = None;
        self.deadline = None;
        self.privacy = None;
        self.verify = None;
        self.verifying = None;
        self.confirmed = false;
        self.quality = None;
    }
}

/// The account's call, at most one.
pub(crate) struct Calls {
    port: Option<VoicePort>,
    /// Where watched streams' and cameras' frames go; without it none are.
    pub(crate) video: Option<VideoPort>,
    /// The host's requests, taken with the first call.
    requests: Option<OwnedMutexGuard<mpsc::Receiver<VideoRequest>>>,
    want: watch::Sender<Option<VoiceWant>>,
    call: Option<Call>,
    me: Option<UserId>,
    /// Bumped to ask the service for the same call again.
    request: u32,
    /// Fixtures reach a voice server on this machine.
    #[cfg(test)]
    pub(crate) local: Option<String>,
    /// The main Gateway's session (READY). The account's voice states with
    /// it are this device's, with another one another device's: the
    /// official client tells them apart so.
    gateway_session: Option<Zeroizing<String>>,
    /// The main Gateway is connected (not reconnecting). While it is not, a
    /// voice server that never answers is waited for, not counted.
    gateway_up: bool,
    // Private calls.
    /// The conversations whose call this Gateway session told, and whom
    /// each rings, the oldest first.
    known: Vec<(u64, Vec<UserId>)>,
    /// A ring held until the service has its call.
    ring: Option<rune_provider::Command>,
    /// What the Gateway asks of a conversation's call (op 13), the
    /// conversations asked about in this session, and what changes to ask
    /// again.
    status: watch::Sender<Option<(u64, u32)>>,
    asked: Vec<u64>,
    status_request: u32,
}

impl Calls {
    pub(crate) fn new(port: Option<VoicePort>) -> Self {
        Self {
            port,
            video: None,
            requests: None,
            want: watch::channel(None).0,
            call: None,
            me: None,
            request: 0,
            #[cfg(test)]
            local: None,
            gateway_session: None,
            gateway_up: true,
            known: Vec::new(),
            ring: None,
            status: watch::channel(None).0,
            asked: Vec::new(),
            status_request: 0,
        }
    }

    /// A new Gateway connection: what it should keep telling the service.
    /// Any call of a previous one is over.
    pub(crate) fn gateway(&mut self) -> watch::Receiver<Option<VoiceWant>> {
        self.call = None;
        self.gateway_session = None;
        // Its calls are told again, and asked about again (op 13).
        self.known.clear();
        self.ring = None;
        self.asked.clear();
        self.status = watch::channel(None).0;
        let (want, wanted) = watch::channel(None);
        self.want = want;
        wanted
    }

    /// The main Gateway's new session, told before its READY.
    pub(crate) fn gateway_session(&mut self, id: Zeroizing<String>) {
        self.gateway_session = Some(id);
    }

    /// The session ended: the call goes and its keys with it.
    pub(crate) fn end(&mut self) {
        self.call = None;
        self.want.send_replace(None);
    }

    pub(crate) fn join(
        &mut self,
        snapshot: &Snapshot,
        channel: ChannelId,
        muted: bool,
        deafened: bool,
    ) -> Result<Vec<Event>, LoginError> {
        let known = snapshot.channels.iter().find(|known| known.id == channel);
        // A server's voice channel, or a direct or group conversation's call.
        let guild = match known {
            Some(known) if known.kind.is_voice() && known.server.is_some() => {
                known.server.map(|server| server.0)
            }
            Some(known) if private::is_private(known) => None,
            _ => return Err(LoginError::new(es("Ese canal de voz no está disponible."))),
        };
        if self.port.is_none() {
            return Err(LoginError::new(es(
                "Esta ventana no puede reproducir llamadas.",
            )));
        }
        let mut events = self.leave_events(None);
        self.me = snapshot.me.as_ref().map(|me| me.id);
        if self.requests.is_none() {
            self.requests = self
                .video
                .as_ref()
                .and_then(|video| Mutex::try_lock_owned(video.requests.clone()).ok());
        }
        // Asked for another call, or none: the host asks again once connected.
        if let Some(requests) = self.requests.as_mut() {
            while requests.try_recv().is_ok() {}
        }
        if guild.is_none() {
            // What the service knows of that call, before joining it (op 13).
            self.ask_status(channel);
        }
        self.call = Some(Call {
            guild,
            channel,
            watches: Vec::new(),
            share: None,
            controls: watch::channel(Controls {
                muted,
                deafened,
                ..Controls::default()
            })
            .0,
            cameras: watch::channel(Vec::new()).0,
            keys: None,
            session: None,
            server: None,
            connection: None,
            phase: None,
            deadline: None,
            joined: false,
            connected_at: None,
            rejoins: 0,
            rejoin_at: None,
            awaiting_gateway: false,
            privacy: None,
            verify: None,
            verifying: None,
            confirmed: false,
            quality: None,
            guild_wait: None,
        });
        note!(CALL, "joining: asking the Gateway (op 4)");
        self.ask();
        events.push(Event::CallPhase {
            channel,
            phase: CallPhase::Joining,
            quality: None,
        });
        Ok(events)
    }

    pub(crate) fn leave(&mut self) -> Vec<Event> {
        self.leave_events(None)
    }

    /// End the call: tell the service, drop the connection and its keys.
    fn leave_events(&mut self, reason: Option<&str>) -> Vec<Event> {
        let Some(call) = self.call.take() else {
            return Vec::new();
        };
        self.ring = None;
        match reason {
            Some(reason) => note!(CALL, "ended: {reason} (leaving: op 4 without channel)"),
            None => note!(CALL, "left (op 4 without channel)"),
        }
        self.want.send_replace(Some(VoiceWant {
            guild: call.guild,
            channel: None,
            muted: false,
            deafened: false,
            request: self.request,
            streams: [None; MAX_STREAMS],
            share: None,
            share_request: 0,
            camera: false,
            share_paused: false,
        }));
        let mut events = Vec::new();
        if let Some(me) = self.me {
            events.push(Event::VoiceLeft {
                user: me,
                channel: call.channel,
            });
        }
        events.push(Event::CallEnded {
            channel: call.channel,
            reason: reason.map(|reason| t(reason).to_owned()),
        });
        events
    }

    pub(crate) fn set_self(&mut self, muted: bool, deafened: bool) {
        let Some(call) = self.call.as_mut() else {
            return;
        };
        call.controls.send_modify(|controls| {
            controls.muted = muted;
            controls.deafened = deafened;
        });
        self.tell();
    }

    /// Turn the account's camera on or off in its call, in `channel`: the
    /// service is told (op 4), and the call's connection announces it or
    /// takes it back (op 12) and sends what the host encodes meanwhile.
    pub(crate) fn set_camera(&mut self, channel: ChannelId, on: bool) -> Result<(), LoginError> {
        let Some(call) = self.call.as_mut().filter(|call| call.channel == channel) else {
            if !on {
                return Ok(());
            }
            return Err(LoginError::new(es(
                "Entra a la llamada para encender la cámara.",
            )));
        };
        if on && self.video.is_none() {
            return Err(LoginError::new(es(
                "Esta ventana no puede enviar la cámara.",
            )));
        }
        call.controls.send_if_modified(|controls| {
            let changed = controls.camera != on;
            controls.camera = on;
            changed
        });
        self.tell();
        Ok(())
    }

    /// Tell the service the call as it is now (op 4).
    fn tell(&mut self) {
        // Held for its server after READY (voz-protocolo): what changes
        // meanwhile goes with the op 4 that ends the wait.
        let Some(call) = self.call.as_ref().filter(|call| call.guild_wait.is_none()) else {
            return;
        };
        let Controls {
            muted,
            deafened,
            camera,
            ..
        } = *call.controls.borrow();
        self.want.send_replace(Some(VoiceWant {
            guild: call.guild,
            channel: Some(call.channel.0),
            muted: muted || deafened,
            deafened,
            request: self.request,
            // Asked for only once this session's voice state is known: a
            // new Gateway session watches nothing until the call is back.
            streams: std::array::from_fn(|at| {
                call.watches
                    .get(at)
                    .filter(|_| call.session.is_some())
                    .map(|watch| (watch.owner, watch.request))
            }),
            share: call
                .share
                .as_ref()
                .filter(|_| call.session.is_some())
                .map(|share| share.owner),
            share_request: call.share.as_ref().map_or(0, |share| share.request),
            camera,
            share_paused: call.share.as_ref().is_some_and(|share| share.pause),
        }));
    }

    /// Watch `owner`'s screen in the account's call, in `channel`, besides
    /// the others; past [`MAX_STREAMS`] the oldest goes.
    pub(crate) fn watch(
        &mut self,
        snapshot: &Snapshot,
        channel: ChannelId,
        owner: UserId,
    ) -> Result<Vec<Event>, LoginError> {
        let Some(call) = self.call.as_mut().filter(|call| call.channel == channel) else {
            return Err(LoginError::new(es(
                "Entra a la llamada para ver la transmisión.",
            )));
        };
        if self.video.is_none() {
            return Err(LoginError::new(es(
                "Esta ventana no puede mostrar transmisiones.",
            )));
        }
        let streaming = snapshot
            .voice
            .iter()
            .any(|state| state.user == owner && state.channel == channel && state.streaming);
        if !streaming || Some(owner) == self.me || owner.0 == 0 {
            return Err(LoginError::new(es(
                "Esa persona no está transmitiendo en tu llamada.",
            )));
        }
        call.watches.retain(|watch| watch.owner != owner.0);
        if call.watches.len() >= MAX_STREAMS {
            call.watches.remove(0);
        }
        call.watches.push(Watch::new(owner.0));
        self.tell();
        Ok(vec![Event::StreamPhase {
            channel,
            owner,
            phase: StreamPhase::Connecting,
        }])
    }

    /// Stop watching `owner`'s stream: the service is told, its connection
    /// and keys go.
    pub(crate) fn stop_watching(&mut self, channel: ChannelId, owner: UserId) {
        let Some(call) = self.call.as_mut().filter(|call| call.channel == channel) else {
            return;
        };
        let before = call.watches.len();
        call.watches.retain(|watch| watch.owner != owner.0);
        if call.watches.len() != before {
            self.tell();
        }
    }

    /// Share the account's screen in its call, in `channel`, at `quality`.
    pub(crate) fn share(
        &mut self,
        channel: ChannelId,
        quality: StreamQuality,
    ) -> Result<Vec<Event>, LoginError> {
        let (Some(call), Some(me)) = (
            self.call.as_mut().filter(|call| call.channel == channel),
            self.me,
        ) else {
            return Err(LoginError::new(es(
                "Entra a la llamada para compartir la pantalla.",
            )));
        };
        if self.video.is_none() {
            return Err(LoginError::new(es(
                "Esta ventana no puede compartir la pantalla.",
            )));
        }
        if !quality.valid() {
            return Err(LoginError::new(es("Esa calidad de transmisión no existe.")));
        }
        call.share = Some(Watch {
            quality: Some(quality),
            ..Watch::new(me.0)
        });
        // A new stream starts unpaused.
        call.controls
            .send_if_modified(|controls| std::mem::take(&mut controls.paused));
        self.tell();
        Ok(vec![Event::StreamPhase {
            channel,
            owner: me,
            phase: StreamPhase::Connecting,
        }])
    }

    /// Pause or resume the account's stream in `channel`: the service is
    /// told (op 22), and its connection sends nothing meanwhile.
    pub(crate) fn pause(&mut self, channel: ChannelId, paused: bool) -> Vec<Event> {
        let Some(call) = self.call.as_mut().filter(|call| call.channel == channel) else {
            return Vec::new();
        };
        let Some(share) = call.share.as_mut().filter(|share| share.pause != paused) else {
            return Vec::new();
        };
        share.pause = paused;
        let owner = UserId(share.owner);
        let phase = match share.phase.as_ref().map(|phase| phase.borrow().clone()) {
            _ if paused || share.paused => StreamPhase::Paused,
            Some(phase) if phase.is_connected() => StreamPhase::Live,
            Some(CallPhase::Reconnecting { .. }) => StreamPhase::Reconnecting,
            _ => StreamPhase::Connecting,
        };
        call.controls
            .send_modify(|controls| controls.paused = paused);
        self.tell();
        vec![Event::StreamPhase {
            channel,
            owner,
            phase,
        }]
    }

    /// Stop sharing: the service is told, the stream's connection goes.
    pub(crate) fn stop_share(&mut self, channel: ChannelId) {
        if let Some(call) = self.call.as_mut().filter(|call| call.channel == channel) {
            if call.share.take().is_some() {
                self.tell();
            }
        }
    }

    // Streams (transmisiones).
    /// The key of the account's own stream while it shares its screen:
    /// what its picture goes up under.
    pub(crate) fn own_stream_key(&self) -> Option<String> {
        let call = self.call.as_ref()?;
        let owner = call.share.as_ref()?.owner;
        Some(crate::http::stream_key(
            call.guild.map(rune_model::ServerId),
            call.channel,
            UserId(owner),
        ))
    }

    /// The stream of `owner` watched, or the account's own, is over: it
    /// goes, with why.
    fn watch_ended(&mut self, own: bool, owner: u64, reason: Option<String>) -> Vec<Event> {
        let Some(call) = self.call.as_mut() else {
            return Vec::new();
        };
        let watch = if own {
            call.share.take()
        } else {
            let at = call.watches.iter().position(|watch| watch.owner == owner);
            at.map(|at| call.watches.remove(at))
        };
        let Some(watch) = watch else {
            return Vec::new();
        };
        let channel = call.channel;
        self.tell();
        vec![Event::StreamPhase {
            channel,
            owner: UserId(watch.owner),
            phase: StreamPhase::Ended(reason),
        }]
    }

    /// Ask the service for the call, even if nothing changed: a voice
    /// server and token follow, in time or the call is lost.
    fn ask(&mut self) {
        let Some(call) = self.call.as_mut() else {
            return;
        };
        call.rejoin_at = None;
        call.awaiting_gateway = false;
        // Asking ends any wait for its server (voz-protocolo).
        call.guild_wait = None;
        call.deadline = Some(Instant::now() + JOIN_TIMEOUT);
        self.request = self.request.wrapping_add(1);
        self.tell();
    }

    /// The call's connection is gone: rejoin within the budget, else end.
    fn lost(&mut self, reason: &'static str) -> Vec<Event> {
        let Some(call) = self.call.as_mut() else {
            return Vec::new();
        };
        if call
            .connected_at
            .take()
            .is_some_and(|at| at.elapsed() >= REJOIN_STABLE)
        {
            call.rejoins = 0;
        }
        // The Gateway is away, so no server can answer it: the wait is for its
        // return (`resumed` or `ready`), and no attempt is spent on it.
        if reason == NO_SERVER && !self.gateway_up && call.joined {
            note!(
                CALL,
                "lost: {reason}; the Gateway is reconnecting: waiting for it, no attempt spent"
            );
            call.drop_connection();
            call.rejoin_at = None;
            call.awaiting_gateway = true;
            return vec![Event::CallPhase {
                channel: call.channel,
                phase: CallPhase::Reconnecting {
                    attempt: call.rejoins,
                },
                quality: None,
            }];
        }
        // A dead network path is rejoined without a budget, 1 s doubling to
        // 10 s; any other loss, a server that never came included, has the
        // three attempts of REJOIN_DELAYS.
        let network = reason == connection::NO_ANSWER || reason == connection::CUT;
        let delay = if network {
            Some(Duration::from_secs((1u64 << call.rejoins.min(4)).min(10)))
        } else {
            REJOIN_DELAYS.get(call.rejoins as usize).copied()
        };
        let Some(delay) = delay.filter(|_| call.joined) else {
            if call.joined {
                note!(CALL, "lost: {reason}; no rejoin attempts left");
            } else {
                note!(CALL, "failed before it ever connected: {reason}");
            }
            return self.leave_events(Some(reason));
        };
        call.rejoins += 1;
        note!(
            CALL,
            "lost: {reason}; rejoin attempt {} in {} s (fresh Identify, and op 4 again)",
            call.rejoins,
            delay.as_secs()
        );
        // The voice session is the Gateway's and stays, and so does the
        // server until the service names another: the official client
        // identifies again with them, without asking the Gateway; op 4
        // goes again too, as Serein does, in case the service moves it.
        call.drop_connection();
        call.rejoin_at = Some(Instant::now() + delay);
        vec![Event::CallPhase {
            channel: call.channel,
            phase: CallPhase::Reconnecting {
                attempt: call.rejoins,
            },
            quality: None,
        }]
    }

    /// The account's voice went on with another session (another device):
    /// the call ends here and the service is told nothing, so the other
    /// device keeps it.
    fn taken_over(&mut self) -> Vec<Event> {
        let Some(call) = self.call.take() else {
            return Vec::new();
        };
        note!(CALL, "ended: the account's voice went on with another session (another device); nothing told to the Gateway");
        self.ring = None;
        self.want.send_replace(None);
        vec![Event::CallEnded {
            channel: call.channel,
            reason: Some(t(TAKEN_OVER).into()),
        }]
    }

    /// The main Gateway is reconnecting: see `lost`.
    pub(crate) fn gateway_away(&mut self) -> Vec<Event> {
        self.gateway_up = false;
        Vec::new()
    }

    /// The main Gateway resumed: a rejoin waiting for it goes at once.
    pub(crate) fn resumed(&mut self) -> Vec<Event> {
        self.gateway_up = true;
        let Some(call) = self.call.as_ref() else {
            return Vec::new();
        };
        if call.rejoin_at.is_some() || call.awaiting_gateway {
            self.ask();
            self.start();
        }
        Vec::new()
    }

    /// A new Gateway session (`READY`): the service forgot this device's
    /// call. Ask for it again, unless the account's voice already went on
    /// with another device.
    pub(crate) fn ready(&mut self, data: &Value) -> Vec<Event> {
        self.gateway_up = true;
        let (Some(call), Some(me)) = (self.call.as_mut(), self.me) else {
            return Vec::new();
        };
        // Only a session the voice server accepted is surely this device's;
        // still waiting for the server since an earlier READY, the one kept.
        let ours = call
            .session
            .clone()
            .filter(|_| call.confirmed)
            .or_else(|| call.guild_wait.take().and_then(|(_, ours)| ours));
        let guilds = || data["guilds"].as_array().into_iter().flatten();
        let states =
            guilds().flat_map(|guild| guild["voice_states"].as_array().into_iter().flatten());
        let gateway = self.gateway_session.as_deref().map(String::as_str);
        if ours
            .as_ref()
            .is_some_and(|ours| elsewhere(states, me, ours, gateway))
        {
            return self.taken_over();
        }
        // Its server may come after READY (unavailable for now): its voice
        // states are not known before its GUILD_CREATE (voz-protocolo).
        let arrived = call.guild.is_none_or(|guild| {
            guilds().any(|row| {
                wire::id(&row["id"]).ok() == Some(guild)
                    && row["unavailable"].as_bool() != Some(true)
            })
        });
        call.forget();
        let channel = call.channel;
        // The new session is not watching or sharing anything: asked for
        // again (ops 20 and 18).
        for watch in call.watches.iter_mut().chain(call.share.as_mut()) {
            watch.forget();
        }
        if arrived {
            note!(
                CALL,
                "new Gateway session (READY): the voice connection goes; asking again (op 4)"
            );
            self.ask();
        } else {
            note!(
                CALL,
                "new Gateway session (READY) without the call's server yet: op 4 waits for its GUILD_CREATE"
            );
            // Nor a rejoin under way: it would ask before the server came.
            call.rejoin_at = None;
            call.guild_wait = Some((Instant::now() + JOIN_TIMEOUT, ours));
        }
        vec![Event::CallPhase {
            channel,
            phase: CallPhase::Joining,
            quality: None,
        }]
    }

    // Voice protocol (voz-protocolo).
    /// A server arrived after READY (`GUILD_CREATE`): a call held for it is
    /// asked for again, unless its voice states show the account's voice on
    /// another device.
    pub(crate) fn guild_created(&mut self, data: &Value) -> Vec<Event> {
        let (Some(call), Some(me)) = (self.call.as_mut(), self.me) else {
            return Vec::new();
        };
        if call.guild_wait.is_none() || call.guild != wire::id(&data["id"]).ok() {
            return Vec::new();
        }
        let Some((_, ours)) = call.guild_wait.take() else {
            return Vec::new();
        };
        let states = data["voice_states"].as_array().into_iter().flatten();
        let gateway = self.gateway_session.as_deref().map(String::as_str);
        if ours.is_some_and(|ours| elsewhere(states, me, &ours, gateway)) {
            return self.taken_over();
        }
        note!(
            CALL,
            "the call's server arrived (GUILD_CREATE): asking again (op 4)"
        );
        self.ask();
        Vec::new()
    }

    /// A voice dispatch of the main Gateway, or `None` for any other.
    pub(crate) fn dispatch(
        &mut self,
        snapshot: &mut Snapshot,
        kind: &str,
        data: &Value,
    ) -> Option<Vec<Event>> {
        match kind {
            "VOICE_STATE_UPDATE" => Some(self.voice_state(snapshot, data)),
            "VOICE_SERVER_UPDATE" => Some(self.voice_server(data)),
            "STREAM_CREATE" | "STREAM_SERVER_UPDATE" | "STREAM_UPDATE" | "STREAM_DELETE" => {
                Some(self.stream(kind, data))
            }
            // Private calls.
            "CALL_CREATE" | "CALL_UPDATE" | "CALL_DELETE" => {
                Some(self.private_call(snapshot, kind, data))
            }
            // Voice protocol (voz-protocolo).
            "CHANNEL_INFO" | "VOICE_CHANNEL_START_TIME_UPDATE" => Some(start_times(kind, data)),
            _ => None,
        }
    }

    /// A dispatch about a stream: only those watched count, and the
    /// account's own.
    fn stream(&mut self, kind: &str, data: &Value) -> Vec<Event> {
        let (Some(call), Some(key)) = (self.call.as_mut(), data["stream_key"].as_str()) else {
            return Vec::new();
        };
        let channel = call.channel;
        let want = *self.want.borrow();
        let own = Some(key) == want.and_then(|want| want.share_key()).as_deref();
        let watched = want.is_some_and(|want| {
            want.stream_keys()
                .iter()
                .any(|(asked, _)| asked.as_str() == key)
        });
        if !own && !watched {
            return Vec::new();
        }
        // A key asked for ends in its owner's id.
        let owner = key.rsplit(':').next().and_then(|id| id.parse().ok());
        let Some(watch) = owner.and_then(|owner| call.slot(own, owner)) else {
            return Vec::new();
        };
        let owner = UserId(watch.owner);
        let scope = if own { SHARE } else { STREAM };
        let phase = |phase| {
            vec![Event::StreamPhase {
                channel,
                owner,
                phase,
            }]
        };
        match kind {
            "STREAM_CREATE" => {
                // Without its server yet: the official client waits for the
                // next one (its deadline still runs).
                if data["rtc_server_id"].is_null() {
                    note!(scope, "STREAM_CREATE without an rtc_server_id yet: waiting");
                    return Vec::new();
                }
                let Ok(server) = wire::id(&data["rtc_server_id"]) else {
                    note!(scope, "STREAM_CREATE refused: rtc_server_id is not an id");
                    return self.watch_ended(
                        own,
                        owner.0,
                        Some(t("Discord envió una transmisión inválida.").into()),
                    );
                };
                // The official client's fallback when no channel is named.
                let named = wire::id(&data["rtc_channel_id"]).ok();
                let group = named.unwrap_or(server - 1);
                watch.paused = data["paused"].as_bool().unwrap_or(false);
                note!(
                    scope,
                    "STREAM_CREATE: rtc server known, rtc channel {}, paused {}",
                    if named.is_some() {
                        "named"
                    } else {
                        "missing (server - 1)"
                    },
                    watch.paused
                );
                if watch.rtc != Some((server, group)) {
                    watch.connection = None;
                    watch.rtc = Some((server, group));
                }
                self.start_watch(own, owner.0);
                Vec::new()
            }
            "STREAM_SERVER_UPDATE" => {
                let token = connection::token(&data["token"]);
                // A null endpoint: its server is being allocated; another update follows.
                let Some(endpoint) = data["endpoint"].as_str() else {
                    note!(
                        scope,
                        "STREAM_SERVER_UPDATE with a null endpoint: waiting for the next one"
                    );
                    watch.connection = None;
                    return Vec::new();
                };
                let url = connection::endpoint(endpoint);
                note!(
                    scope,
                    "STREAM_SERVER_UPDATE: endpoint {} ({}), token {}",
                    diag::shown_endpoint(endpoint),
                    if url.is_ok() {
                        "accepted"
                    } else {
                        "refused: not a *.discord.media host with a valid port"
                    },
                    if token.is_some() {
                        "present"
                    } else {
                        "missing or invalid"
                    }
                );
                let (Some(token), Ok(url)) = (token, url) else {
                    return self.watch_ended(
                        own,
                        owner.0,
                        Some(t("Discord envió una transmisión inválida.").into()),
                    );
                };
                #[cfg(test)]
                let url = self.local.clone().unwrap_or(url);
                watch.server = Some((Zeroizing::new(token.to_owned()), url));
                watch.connection = None;
                self.start_watch(own, owner.0);
                Vec::new()
            }
            "STREAM_UPDATE" => {
                watch.paused = data["paused"].as_bool().unwrap_or(false);
                note!(scope, "STREAM_UPDATE: paused {}", watch.paused);
                let connected = watch
                    .phase
                    .as_ref()
                    .is_some_and(|phase| phase.borrow().is_connected());
                match (watch.paused || watch.pause, connected) {
                    (true, _) => phase(StreamPhase::Paused),
                    (false, true) => phase(StreamPhase::Live),
                    (false, false) => Vec::new(),
                }
            }
            _ if data["unavailable"].as_bool() == Some(true) => {
                note!(scope, "STREAM_DELETE unavailable: asking again (op 21)");
                // Its server went away for now: asked for again (op 21).
                watch.forget();
                watch.request = watch.request.wrapping_add(1);
                self.tell();
                phase(StreamPhase::Reconnecting)
            }
            // The account's own, ended by Discord: why, unless it was asked.
            _ => {
                let reason = data["reason"].as_str();
                note!(
                    scope,
                    "STREAM_DELETE: reason {}",
                    reason.map_or_else(|| "none".into(), diag::reason)
                );
                self.watch_ended(own, owner.0, stream_ended(reason, own))
            }
        }
    }

    /// With the stream's server identity, token and endpoint known, reach
    /// it: the one of `owner` watched, or the account's own.
    fn start_watch(&mut self, own: bool, owner: u64) {
        let (Some(call), Some(port), Some(video), Some(me)) = (
            self.call.as_mut(),
            self.port.as_ref(),
            self.video.as_ref(),
            self.me,
        ) else {
            return;
        };
        let session = call.session.clone();
        let controls = call.controls.subscribe();
        let Some(watch) = call.slot(own, owner) else {
            return;
        };
        if watch.connection.is_some() {
            return;
        }
        let (Some(session), Some((server, group)), Some((token, url))) =
            (session.as_ref(), watch.rtc, watch.server.as_ref())
        else {
            return;
        };
        let credentials = Credentials {
            server,
            channel: group,
            user: me.0,
            session: session.clone(),
            token: token.clone(),
            url: url.clone(),
            codes: None,
            #[cfg(test)]
            local: self.local.is_some(),
        };
        watch.deadline = None;
        note!(
            if own { SHARE } else { STREAM },
            "session, rtc server and endpoint known: connecting"
        );
        let (phase, phases) = watch::channel(CallPhase::Joining);
        let (keys, asked) = mpsc::channel(REQUESTS);
        watch.phase = Some(phases);
        watch.keys = Some(keys);
        let video = Video {
            kind: VideoKind::Screen,
            incoming: video.incoming.clone(),
            keys: asked,
            wanted: None,
            send: watch.quality.map(|quality| Outgoing {
                quality,
                units: video.outgoing.clone(),
                feedback: video.feedback.clone(),
                sound: Some(video.sound.clone()),
            }),
        };
        watch.connection = Some(Box::pin(connection::run(
            credentials,
            port.clone(),
            Some(video),
            controls,
            phase,
        )));
    }

    fn voice_state(&mut self, snapshot: &mut Snapshot, data: &Value) -> Vec<Event> {
        let Some((user, state)) = state(data) else {
            return Vec::new();
        };
        let me = snapshot.me.as_ref().map(|me| me.id);
        let mut events = Vec::new();
        let before = snapshot.voice.iter().position(|known| known.user == user);
        match (&state, before) {
            (Some(state), _) => {
                if let Some(at) = before {
                    snapshot.voice.remove(at);
                }
                if snapshot.voice.len() < MAX_STATES {
                    snapshot.voice.push(state.clone());
                    events.push(Event::VoiceChanged(state.clone()));
                }
            }
            (None, Some(at)) => {
                let left = snapshot.voice.remove(at);
                events.push(Event::VoiceLeft {
                    user,
                    channel: left.channel,
                });
            }
            (None, None) => {}
        }
        if Some(user) != me {
            return events;
        }
        let Some(call) = self.call.as_mut() else {
            return events;
        };
        let session = session(data);
        // Whose it is: this device's (the Gateway's session), another
        // device's, or not known (no session in it, or none known yet).
        let device = match (self.gateway_session.as_deref(), session) {
            (Some(ours), Some(theirs)) => Some(ours.as_str() == theirs),
            _ => None,
        };
        let here = state
            .as_ref()
            .is_some_and(|state| state.channel == call.channel);
        let other = match (call.session.as_deref(), session) {
            (Some(ours), Some(theirs)) => ours.as_str() != theirs,
            _ => false,
        };
        let place = match &state {
            None => "no channel",
            Some(_) if here => "the call's channel",
            Some(_) => "another channel",
        };
        let what = match (session, other) {
            (None, _) => "without a session",
            (Some(_), true) => "with another session than the call's",
            (Some(_), false) if call.session.is_none() => "with a session",
            (Some(_), false) => "with the call's session",
        };
        note!(
            CALL,
            "own VOICE_STATE_UPDATE: {place}, {what}, {}, {}",
            match device {
                Some(true) => "this device's Gateway session",
                Some(false) => "another device's Gateway session",
                None => "Gateway session not compared",
            },
            if call.confirmed {
                "voice server accepted ours"
            } else {
                "ours not accepted yet"
            }
        );
        match device {
            // Another device's: before this device's own state, the account's
            // other client (the official one makes its connection only on its
            // own state); after, that device took the call.
            Some(false) if call.session.is_none() => {
                note!(CALL, "  another device's, before ours: ignored");
                return events;
            }
            Some(false) => {
                events.extend(self.taken_over());
                return events;
            }
            Some(true) => {
                if !here && !call.confirmed {
                    note!(
                        CALL,
                        "  stale before the voice server accepted ours: ignored"
                    );
                    return events;
                }
                if other {
                    note!(
                        CALL,
                        "  this device's newer session: connecting again with it"
                    );
                    call.session = None;
                }
            }
            // Not known: before this device's voice server accepts a session,
            // the service may still describe another client or an older
            // request (Serein): states elsewhere are stale, and the newest
            // session in the channel is the candidate.
            None if !call.confirmed => {
                if !here {
                    note!(
                        CALL,
                        "  stale before the voice server accepted ours: ignored"
                    );
                    return events;
                }
                if other {
                    note!(CALL, "  a newer candidate: connecting again with it");
                    call.session = None;
                }
            }
            // Once accepted, another session is another device's.
            None if other => {
                events.extend(self.taken_over());
                return events;
            }
            None => {}
        }
        let Some(call) = self.call.as_mut() else {
            return events;
        };
        match state {
            Some(state) if state.channel == call.channel => {
                if let Some(session) = session.filter(|_| call.session.is_none()) {
                    call.session = Some(Zeroizing::new(session.to_owned()));
                    call.connection = None;
                }
                self.start();
                let owners: Vec<u64> = self
                    .call
                    .iter()
                    .flat_map(|call| &call.watches)
                    .map(|watch| watch.owner)
                    .collect();
                for owner in owners {
                    self.start_watch(false, owner);
                }
                self.start_watch(true, me.map_or(0, |me| me.0));
                // A watched stream, or the account's own, may be asked for now.
                self.tell();
            }
            Some(state) => {
                let old_channel = call.channel;
                let moved_within_guild =
                    call.guild.is_some() && wire::id(&data["guild_id"]).ok() == call.guild;
                if moved_within_guild {
                    note!(
                        CALL,
                        "the account was moved by the service: following the new channel"
                    );
                    if let Some(call) = self.call.as_mut() {
                        call.channel = state.channel;
                        call.forget();
                        call.session = session.map(|session| Zeroizing::new(session.to_owned()));
                        call.watches.clear();
                        call.share = None;
                        call.keys = None;
                        call.cameras.send_replace(Vec::new());
                        call.controls
                            .send_modify(|controls| controls.camera = false);
                        call.connected_at = None;
                        call.rejoins = 0;
                        call.rejoin_at = None;
                    }
                    // Re-assert the destination rather than sending op 4 with
                    // no channel: the service has already moved the account.
                    // Asking also covers VOICE_SERVER_UPDATE arriving first.
                    self.ask();
                    events.push(Event::CallEnded {
                        channel: old_channel,
                        reason: None,
                    });
                    events.push(Event::CallPhase {
                        channel: state.channel,
                        phase: CallPhase::Joining,
                        quality: None,
                    });
                } else {
                    events.extend(self.leave_events(Some(es("Te movieron a otro canal de voz."))));
                }
            }
            None => events.extend(self.leave_events(Some(es("Te desconectaron de la llamada.")))),
        }
        events
    }

    fn voice_server(&mut self, data: &Value) -> Vec<Event> {
        let Some(call) = self.call.as_mut() else {
            return Vec::new();
        };
        // A conversation's call names its channel and no guild, as the
        // official client takes it.
        let ours = match call.guild {
            Some(guild) => wire::id(&data["guild_id"]).ok() == Some(guild),
            None => {
                data["guild_id"].is_null()
                    && wire::id(&data["channel_id"]).ok() == Some(call.channel.0)
            }
        };
        if !ours {
            note!(
                CALL,
                "VOICE_SERVER_UPDATE of another guild or call: ignored"
            );
            return Vec::new();
        }
        // A null endpoint: the voice server is being reallocated; another
        // update follows, in time or the call is lost.
        let Some(endpoint) = data["endpoint"].as_str() else {
            note!(
                CALL,
                "VOICE_SERVER_UPDATE with a null endpoint: waiting for the next one"
            );
            call.server = None;
            call.connection = None;
            call.deadline = Some(Instant::now() + JOIN_TIMEOUT);
            return Vec::new();
        };
        let token = connection::token(&data["token"]);
        let url = connection::endpoint(endpoint);
        note!(
            CALL,
            "VOICE_SERVER_UPDATE: endpoint {} ({}), token {}",
            diag::shown_endpoint(endpoint),
            if url.is_ok() {
                "accepted"
            } else {
                "refused: not a *.discord.media host with a valid port"
            },
            if token.is_some() {
                "present"
            } else {
                "missing or invalid"
            }
        );
        let (Some(token), Ok(url)) = (token, url) else {
            // Waiting would only end in «no server in time», with the wrong reason.
            return self.lost(es("Discord envió un servidor de voz inválido."));
        };
        #[cfg(test)]
        let url = self.local.clone().unwrap_or(url);
        call.server = Some((Zeroizing::new(token.to_owned()), url));
        // A new server or token: connect again.
        call.connection = None;
        self.start();
        Vec::new()
    }

    /// With the session and the server known, reach the voice server.
    fn start(&mut self) {
        let (Some(call), Some(port)) = (self.call.as_mut(), self.port.as_ref()) else {
            return;
        };
        if call.connection.is_some() {
            return;
        }
        let (Some(session), Some((token, url)), Some(me)) =
            (call.session.as_ref(), call.server.as_ref(), self.me)
        else {
            return;
        };
        // The codes of this connection's group (P6).
        let (privacy, told) = watch::channel(None);
        let (verify, asked) = mpsc::channel(4);
        call.privacy = Some(told);
        call.verify = Some(verify);
        call.verifying = None;
        // And what it measures, and since when it is connected (voz-protocolo).
        let (quality, measured) = watch::channel(None);
        call.quality = Some(measured);
        call.connected_at = None;
        let credentials = Credentials {
            // A conversation's call goes by its channel (the official client's
            // `server_id`, Serein's too).
            server: call.guild.unwrap_or(call.channel.0),
            channel: call.channel.0,
            user: me.0,
            session: session.clone(),
            token: token.clone(),
            url: url.clone(),
            codes: Some(Codes {
                privacy,
                verify: asked,
                quality,
            }),
            #[cfg(test)]
            local: self.local.is_some(),
        };
        call.deadline = None;
        note!(
            CALL,
            "session and voice server known: connecting to {}",
            diag::host(url)
        );
        let (phase, phases) = watch::channel(CallPhase::Joining);
        let controls = call.controls.subscribe();
        let port = port.clone();
        // The others' cameras come over the call's own connection, and the
        // account's goes out on it while on.
        let video = self.video.as_ref().map(|video| {
            let (keys, asked) = mpsc::channel(REQUESTS);
            call.keys = Some(keys);
            Video {
                kind: VideoKind::Camera,
                incoming: video.incoming.clone(),
                keys: asked,
                wanted: Some(call.cameras.subscribe()),
                send: Some(Outgoing {
                    quality: CAMERA,
                    units: video.camera_outgoing.clone(),
                    feedback: video.camera_feedback.clone(),
                    sound: None,
                }),
            }
        });
        call.phase = Some(phases);
        call.connection = Some(Box::pin(connection::run(
            credentials,
            port,
            video,
            controls,
            phase,
        )));
    }

    /// One of the host's requests, to the connection it is about.
    fn request(&mut self, request: VideoRequest) {
        let Some(call) = self.call.as_mut() else {
            return;
        };
        match request {
            VideoRequest::KeyFrame { user, kind } => {
                let keys = match kind {
                    VideoKind::Screen => call
                        .watches
                        .iter()
                        .find(|watch| watch.owner == user.0)
                        .and_then(|watch| watch.keys.as_ref()),
                    VideoKind::Camera => call.keys.as_ref(),
                };
                // Full: key frames are asked for already.
                if let Some(keys) = keys {
                    let _ = keys.try_send(user.0);
                }
            }
            VideoRequest::Wants {
                user,
                kind: VideoKind::Camera,
                on,
            } => {
                call.cameras.send_if_modified(|cameras| {
                    let shown = cameras.contains(&user.0);
                    if on && !shown && cameras.len() < MAX_CAMERAS {
                        cameras.push(user.0);
                        true
                    } else if !on && shown {
                        cameras.retain(|camera| *camera != user.0);
                        true
                    } else {
                        false
                    }
                });
            }
            // The screens watched are those asked for with WatchStream.
            VideoRequest::Wants { .. } => {}
        }
    }

    /// What the connection of the stream of `owner` watched (or the
    /// account's own, `own`) had to say, as events.
    fn watched(&mut self, own: bool, owner: u64, watched: Watched) -> Vec<Event> {
        let reason = match watched {
            Watched::Late => es("Discord no conectó la transmisión a tiempo."),
            Watched::Ended(result) => result.err().unwrap_or(es("La transmisión terminó.")),
            Watched::Phase(phase) => {
                let Some(call) = self.call.as_mut() else {
                    return Vec::new();
                };
                let channel = call.channel;
                let Some(watch) = call.slot(own, owner) else {
                    return Vec::new();
                };
                let phase = match phase {
                    // The connection went with its phases; its result follows.
                    None => {
                        watch.phase = None;
                        return Vec::new();
                    }
                    Some(phase) if phase.is_connected() && (watch.paused || watch.pause) => {
                        StreamPhase::Paused
                    }
                    Some(phase) if phase.is_connected() => StreamPhase::Live,
                    Some(CallPhase::Reconnecting { .. }) => StreamPhase::Reconnecting,
                    Some(_) => StreamPhase::Connecting,
                };
                return vec![Event::StreamPhase {
                    channel,
                    owner: UserId(watch.owner),
                    phase,
                }];
            }
        };
        self.watch_ended(own, owner, Some(t(reason).into()))
    }

    /// Ask the call in `channel` for the code the account compares with
    /// `user` (P6); its answer comes from [`Self::next`], at once `None`
    /// when there is no such call.
    pub(crate) fn verify(&mut self, channel: ChannelId, user: UserId) -> Vec<Event> {
        let asked = self
            .call
            .as_mut()
            .filter(|call| call.channel == channel)
            .and_then(|call| {
                let (answer, answered) = oneshot::channel();
                call.verify.as_ref()?.try_send((user.0, answer)).ok()?;
                call.verifying = Some((user, answered));
                Some(())
            });
        match asked {
            Some(()) => Vec::new(),
            None => vec![Event::VoiceVerification {
                channel,
                user,
                code: None,
            }],
        }
    }

    /// The next thing the call has to report. Pending without a call.
    pub(crate) async fn next(&mut self) -> Vec<Event> {
        let Some(call) = self.call.as_mut() else {
            return std::future::pending().await;
        };
        let channel = call.channel;
        let reason = tokio::select! {
            request = async {
                match self.requests.as_mut() { Some(requests) => requests.recv().await, None => std::future::pending().await }
            } => {
                match request {
                    Some(request) => self.request(request),
                    // The host let go of its end.
                    None => self.requests = None,
                }
                return Vec::new();
            }
            (owner, watched) = progress_any(&mut call.watches) => return self.watched(false, owner, watched),
            shared = progress(&mut call.share) => {
                let me = self.me.map_or(0, |me| me.0);
                return self.watched(true, me, shared);
            }
            _ = async {
                match call.deadline { Some(deadline) => sleep_until(deadline).await, None => std::future::pending().await }
            } => NO_SERVER,
            _ = async {
                match call.rejoin_at { Some(at) => sleep_until(at).await, None => std::future::pending().await }
            } => {
                // Streams are asked for again, as READY does: the path they used is gone.
                if let Some(call) = self.call.as_mut() {
                    for watch in call.watches.iter_mut().chain(call.share.as_mut()) {
                        watch.forget();
                    }
                }
                self.ask();
                self.start();
                return Vec::new();
            }
            // Voice protocol (voz-protocolo): the server never came; asked anyway.
            _ = async {
                match &call.guild_wait { Some((at, _)) => sleep_until(*at).await, None => std::future::pending().await }
            } => {
                note!(CALL, "no GUILD_CREATE of the call's server in time: asking again (op 4)");
                self.ask();
                return Vec::new();
            }
            changed = async {
                match call.quality.as_mut() { Some(quality) => quality.changed().await, None => std::future::pending().await }
            } => {
                let (Ok(()), Some(quality)) = (changed, call.quality.as_mut()) else {
                    call.quality = None;
                    return Vec::new();
                };
                let quality = *quality.borrow_and_update();
                let phase = call.phase.as_ref().map(|phase| phase.borrow().clone());
                return match phase.filter(CallPhase::is_connected) {
                    Some(phase) => vec![Event::CallPhase { channel, phase, quality }],
                    None => Vec::new(),
                };
            }
            // Desktop (P6): the call's privacy code, and the code asked for.
            changed = async {
                match call.privacy.as_mut() { Some(privacy) => privacy.changed().await, None => std::future::pending().await }
            } => {
                let code = match (changed, call.privacy.as_mut()) {
                    (Ok(()), Some(privacy)) => privacy.borrow_and_update().clone(),
                    _ => {
                        call.privacy = None;
                        return Vec::new();
                    }
                };
                return vec![Event::CallPrivacyCode { channel, code }];
            }
            answered = async {
                match call.verifying.as_mut() { Some((_, answer)) => answer.await.ok().flatten(), None => std::future::pending().await }
            } => {
                let Some((user, _)) = call.verifying.take() else { return Vec::new() };
                return vec![Event::VoiceVerification { channel, user, code: answered }];
            }
            changed = async {
                match call.phase.as_mut() { Some(phase) => phase.changed().await, None => std::future::pending().await }
            } => {
                let phase = match (changed, call.phase.as_mut()) {
                    (Ok(()), Some(phase)) => phase.borrow_and_update().clone(),
                    // The connection went with its phases; its result follows.
                    _ => {
                        call.phase = None;
                        return Vec::new();
                    }
                };
                // Past Identify: the voice server accepted this session.
                if phase != CallPhase::Joining && !call.confirmed {
                    call.confirmed = true;
                    note!(CALL, "the voice server accepted this device's session");
                }
                // Waiting and Connected alternate as people come and go: the
                // same connection, connected since it first was (`start`
                // forgets it for a new one).
                if phase.is_connected() {
                    call.joined = true;
                    call.connected_at.get_or_insert_with(Instant::now);
                }
                return vec![Event::CallPhase { channel, phase, quality: None }];
            }
            result = async {
                match call.connection.as_mut() { Some(connection) => connection.await, None => std::future::pending().await }
            } => match result {
                // The host let go of the call.
                Ok(()) => return self.leave_events(Some(es("La llamada terminó."))),
                // Removed by Discord: never rejoined, as the official client
                // and Serein do.
                Err(reason) if reason == connection::KICKED => return self.leave_events(Some(reason)),
                Err(reason) => reason,
            },
        };
        // A call that had connected rejoins within its budget, whatever
        // ended it (Serein's rule); one that never did ends here.
        self.lost(reason)
    }
}
