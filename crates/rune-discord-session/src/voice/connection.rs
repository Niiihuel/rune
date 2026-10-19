//! One voice connection: the voice gateway (version 8) and its UDP media.
//!
//! Hello → Identify (DAVE version 1) → Ready (SSRC, address, modes) → IP
//! discovery over UDP → Select Protocol (`aead_xchacha20_poly1305_rtpsize`)
//! → Session Description (transport key) → DAVE key package and group →
//! media. Heartbeats carry `seq_ack`; a dropped socket resumes (op 7) a
//! bounded number of times. Keys live in this future and are erased when it
//! ends or is dropped.
//!
//! Given a [`Video`], it says it handles video (op 0) and decodes H264
//! (op 1), learns who sends each video SSRC and its RTX SSRC (op 12) and
//! turns H264 packets into decrypted access units for the host. Someone who
//! sends a camera in layers (simulcast: rid "100" and "50") is asked for
//! one only, the best while at most two cameras show and the lightest past
//! that; the other is neither forwarded nor assembled. A gap asks
//! for the missing packets again (RTCP NACK) and waits a little for their
//! retransmissions ([`super::nack`]); a picture lost all the same asks for a
//! key frame (RTCP PLI). Watching someone's screen stream, on a
//! connection of its own, it sends nothing of the microphone and asks for
//! every stream (op 15 `{any: 100}`); the call's own connection asks for
//! the cameras on screen only (op 15 per SSRC: 100, or 0). Either repeats
//! op 15 every 5 s, every second while a wanted source shows nothing.
//!
//! Sharing the account's screen, on a stream connection of its own, it
//! announces one screen stream (op 0), takes its SSRC from Ready (RTX is the
//! next one unless named), says it encodes H264 (op 1) and describes the
//! stream (op 12). Each unit from the host gets an SPS that says no picture
//! is reordered ([`super::sps`]), is encrypted for the group, packetized and
//! paced out ([`super::send`]); a PLI or FIR asks the host for a key frame,
//! receiver reports and REMB move the bit rate, and a NACK is answered with
//! the packets kept for a second on the RTX SSRC (or, gone, with a key
//! frame). Nothing goes out without a group, and after anything dropped
//! only from a key frame on.
//!
//! The call's own connection sends the account's camera the same way: it
//! always announces one camera stream (op 0), as the official client does,
//! so Ready gives it an SSRC and turning the camera on mid-call needs no new
//! connection. While [`Controls::camera`] is on it describes the camera
//! (op 12) and sends the host's units; off, op 12 says `video_ssrc: 0`.
//!
//! Connected alone in its channel, a connection is `Waiting` for the others
//! (Serein's phase), `Connected` once someone is there. The call's own
//! measures its connection at each heartbeat's acknowledgement: the round
//! trip of that heartbeat and the share of the others' audio packets that
//! never came since the last one, by the gaps in their sequence numbers.

use rune_i18n::es;
use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use rune_model::{CallPhase, CallQuality, StreamQuality, UserId, VideoKind};
use rune_provider::video::{Feedback, VideoUnit, CAMERA_MAX_BITRATE, CAMERA_MIN_BITRATE};
use rune_provider::voice::{VoicePacket, VoicePort};
use serde_json::{json, Map, Value};
use tokio::{
    net::{TcpStream, UdpSocket},
    sync::{mpsc, oneshot, watch, Mutex},
    time::{interval, sleep, sleep_until, timeout, Instant, MissedTickBehavior},
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{client::IntoClientRequest, protocol::WebSocketConfig, Message},
    MaybeTlsStream, WebSocketStream,
};
use zeroize::Zeroizing;

use super::{
    dave::{op, Dave, Reply},
    diag::note,
    h264::{self, Depacketizer},
    nack::{self, History, Reorder, Resend},
    rtp::{self, Report, Transport},
    send::{Pacer, Rate, Traffic, MIN_BITRATE, PACE, RATE_STEP, REPORT_EVERY},
    sps,
};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Voice gateway messages are small; DAVE welcomes for a full room are the largest.
const MAX_SIGNAL: usize = 64 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// From the first socket to Connected, unless alone.
const SETUP_TIMEOUT: Duration = Duration::from_secs(30);
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(8);
/// Consecutive resumptions before giving up.
pub(crate) const MAX_RESUMES: u32 = 3;
/// Every resumption failed: the call may still be asked for again.
pub(crate) const LOST: &str = es("No se pudo recuperar la conexión de voz.");
/// Closed with 4014: kicked, moved or the channel deleted. Never rejoined.
pub(crate) const KICKED: &str = es("Discord te desconectó de la llamada.");
/// IP discovery is asked again this often until [`DISCOVERY_TIMEOUT`]: one
/// lost datagram must not end the call.
const DISCOVERY_RETRY: Duration = Duration::from_secs(1);
/// No frame from the host for this long ends a talk spurt.
const SPURT_GAP: Duration = Duration::from_millis(100);
const SILENCE_FRAMES: u8 = 5;
const SILENCE: [u8; 3] = [0xf8, 0xff, 0xfe];
/// Op 5 says what an SSRC carries: someone's voice, or a screen's sound.
const VOICE: u8 = 1;
const SOUNDSHARE: u8 = 2;
const UDP_KEEPALIVE: Duration = Duration::from_secs(5);
/// Control messages per second before the server counts as misbehaving.
const MAX_SIGNALS_PER_SECOND: u32 = 256;
/// Datagrams handled per wakeup, so signaling is never starved.
const MAX_DATAGRAMS: usize = 64;
/// Key frames asked of one stream at most this often.
const PLI_GAP: Duration = Duration::from_millis(500);
/// Op 15 again this often, as Serein does: the server may stop forwarding
/// video once a viewer's wants lapse (not confirmed in the bundles).
const WANTS_REFRESH: Duration = Duration::from_secs(5);
/// Sooner while a wanted source has shown nothing for this long.
const WANTS_STALLED: Duration = Duration::from_secs(1);
/// Past this many cameras on screen, a camera sent in layers is asked for
/// its lightest: the tiles are small.
const BEST_LAYERS: usize = 2;
// Voice protocol (voz-protocolo).
/// A jump this far ahead in someone's audio is a new start, not packets lost.
const MAX_GAP: u16 = 250;
/// Audio sources whose sequence is followed; past it they start over.
const MAX_HEARD: usize = 256;

/// What a connection carrying video is given: see the module.
pub(crate) struct Video {
    /// `Screen`: someone's stream, on a connection of its own; `Camera`:
    /// the cameras of the call, on the call's connection.
    pub kind: VideoKind,
    pub incoming: mpsc::Sender<VideoUnit>,
    /// Whose key frames the host asks for.
    pub keys: mpsc::Receiver<u64>,
    /// Whose cameras are on screen; the call's connection only.
    pub wanted: Option<watch::Receiver<Vec<u64>>>,
    /// The account's own screen, on the stream connection that shares it,
    /// or its camera, on the call's.
    pub send: Option<Outgoing>,
}

/// The account's screen or camera to send: its quality, the host's units
/// and where to say what the encoder must do; and the screen's sound.
pub(crate) struct Outgoing {
    pub quality: StreamQuality,
    pub units: std::sync::Arc<Mutex<mpsc::Receiver<VideoUnit>>>,
    pub feedback: mpsc::Sender<Feedback>,
    pub sound: Option<std::sync::Arc<Mutex<mpsc::Receiver<Vec<u8>>>>>,
}

/// What a connection sending the account's screen or camera keeps.
struct Sender {
    kind: VideoKind,
    quality: StreamQuality,
    feedback: mpsc::Sender<Feedback>,
    /// From Ready: the screen's SSRC and its retransmissions'.
    ssrc: u32,
    rtx: u32,
    sequence: u16,
    rtx_sequence: u16,
    /// What went out lately, for the NACKs.
    history: History,
    /// The RTP clock (90 kHz) at `since`.
    clock: u32,
    since: std::time::Instant,
    pacer: Pacer,
    next_pace: Instant,
    rate: Rate,
    next_rate: Instant,
    /// Nothing goes out until a key frame: at the start, before the group,
    /// after anything dropped.
    waiting: bool,
    /// The last key frame asked of the host.
    asked: Option<Instant>,
    /// Op 12 described it to the voice server, and not taken back.
    announced: bool,
    /// What went out since the last diagnostic line, and when the next is due.
    traffic: Traffic,
    next_report: Instant,
}

impl Sender {
    fn new(outgoing: Outgoing, kind: VideoKind, random: [u8; 8]) -> Self {
        let now = Instant::now();
        let (min, max) = match kind {
            VideoKind::Screen => (MIN_BITRATE, outgoing.quality.max_bitrate()),
            VideoKind::Camera => (CAMERA_MIN_BITRATE, CAMERA_MAX_BITRATE),
        };
        Self {
            kind,
            rate: Rate::new(min, max),
            quality: outgoing.quality,
            feedback: outgoing.feedback,
            ssrc: 0,
            rtx: 0,
            sequence: u16::from_be_bytes([random[0], random[1]]),
            rtx_sequence: u16::from_be_bytes([random[6], random[7]]),
            history: History::default(),
            clock: u32::from_be_bytes([random[2], random[3], random[4], random[5]]),
            since: now.into_std(),
            pacer: Pacer::new(now),
            next_pace: now,
            next_rate: now + RATE_STEP,
            waiting: true,
            asked: None,
            announced: false,
            traffic: Traffic::default(),
            next_report: now + REPORT_EVERY,
        }
    }

    /// Ask the host for a key frame, at most every [`PLI_GAP`].
    fn key_frame(&mut self, now: Instant) {
        if self.asked.is_none_or(|at| now - at >= PLI_GAP) {
            self.asked = Some(now);
            // Full: the encoder has key frames to make already.
            let _ = self.feedback.try_send(Feedback::KeyFrame);
        }
    }

    /// A NACK entry about what it sent: what is kept goes again on RTX,
    /// ahead of the unit on its way; anything gone asks for a key frame.
    fn resend(&mut self, first: u16, mask: u16, now: Instant) {
        let mut gone = false;
        for sequence in nack::lost(first, mask) {
            match self.history.resend(sequence, now) {
                Resend::Packet(packet) => {
                    self.pacer
                        .repair(nack::rtx(&packet, self.rtx, self.rtx_sequence));
                    self.rtx_sequence = self.rtx_sequence.wrapping_add(1);
                }
                Resend::Wait => {}
                Resend::Gone => gone = true,
            }
        }
        if gone {
            self.key_frame(now);
        }
        self.next_pace = now;
    }
}

/// Someone's video SSRC.
struct Source {
    user: u64,
    /// The layer it is, 0 to 100: 50 for a simulcast camera's lighter one.
    quality: u8,
    /// Where its retransmissions come from.
    rtx: u32,
    /// Its packets in order, waiting for those asked for again.
    reorder: Reorder,
    assembly: Depacketizer,
    /// The last key frame asked of it.
    asked: Option<Instant>,
    /// The last unit it brought to the host.
    shown: Option<Instant>,
}

/// What the main Gateway gave to reach this call's voice server.
pub(crate) struct Credentials {
    pub server: u64,
    pub channel: u64,
    pub user: u64,
    pub session: Zeroizing<String>,
    pub token: Zeroizing<String>,
    /// A validated `wss://…/?v=8` address.
    pub url: String,
    /// The call's codes (P6); a stream's connection has none.
    pub codes: Option<Codes>,
    /// Fixtures only: the voice server may be on this machine.
    #[cfg(test)]
    pub local: bool,
}

impl Credentials {
    fn local(&self) -> bool {
        #[cfg(test)]
        {
            self.local
        }
        #[cfg(not(test))]
        {
            false
        }
    }
}

/// Where the call's connection tells its privacy code, and hears who asks
/// for the code they compare with someone (P6); and where it tells what it
/// measures (voz-protocolo).
pub(crate) struct Codes {
    pub privacy: watch::Sender<Option<String>>,
    pub verify: mpsc::Receiver<(u64, oneshot::Sender<Option<String>>)>,
    pub quality: watch::Sender<Option<CallQuality>>,
}

/// Mute, deafen, the camera and the shared screen as the account has them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Controls {
    pub muted: bool,
    pub deafened: bool,
    /// The call's connection sends the account's camera.
    pub camera: bool,
    /// The account's shared screen is paused: its connection sends nothing.
    pub paused: bool,
}

/// The voice server endpoint must be Discord's media infrastructure. The
/// port it names is kept, as Serein and the official client connect to it:
/// voice servers are not only on 443.
pub(crate) fn endpoint(raw: &str) -> Result<String, &'static str> {
    const INVALID: &str = es("Discord envió un servidor de voz inválido.");
    let (host, port) = match raw.rsplit_once(':') {
        Some((host, port)) => {
            let port = port
                .bytes()
                .all(|byte| byte.is_ascii_digit())
                .then(|| port.parse::<u16>().ok())
                .flatten()
                .filter(|port| *port != 0)
                .ok_or(INVALID)?;
            (host, Some(port))
        }
        None => (raw, None),
    };
    if host.is_empty()
        || host.len() > 253
        || !host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.')
        || !host.ends_with(".discord.media")
    {
        return Err(INVALID);
    }
    Ok(match port {
        // The official client goes to 443 for a server named on 80.
        None | Some(80 | 443) => format!("wss://{host}/?v=8"),
        Some(port) => format!("wss://{host}:{port}/?v=8"),
    })
}

/// A voice or stream token as the main Gateway hands it: kept only if
/// plausible (Serein's bound).
pub(crate) fn token(raw: &Value) -> Option<&str> {
    raw.as_str().filter(|token| {
        !token.is_empty() && token.len() <= 2048 && token.bytes().all(|b| b.is_ascii_graphic())
    })
}

/// Media must go to a public address; the server is never allowed to point
/// UDP at this machine or its network.
fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [first, second, ..] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_multicast()
                || ip.is_unspecified()
                || ip.is_documentation()
                || first == 0
                || first >= 240
                || (first == 100 && (64..128).contains(&second)))
        }
        IpAddr::V6(ip) => {
            let first = ip.segments()[0];
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || ip.to_ipv4_mapped().is_some()
                || (first == 0x2001 && ip.segments()[1] == 0x0db8))
        }
    }
}

enum Ended {
    /// The socket dropped; a resume may bring it back.
    Dropped(&'static str),
    Fatal(&'static str),
}

fn close_reason(code: u16) -> Ended {
    match code {
        // Voice server crashed: the session survives.
        4015 => Ended::Dropped(es("El servidor de voz se reinició.")),
        4014 => Ended::Fatal(KICKED),
        4006 | 4009 => Ended::Fatal(es("La sesión de voz terminó. Volvé a entrar.")),
        4011 | 4012 | 4016 => Ended::Fatal(es("El servidor de voz no aceptó el cifrado de RUNE.")),
        4017 => Ended::Fatal(es(
            "Discord exige cifrado de extremo a extremo que no se pudo negociar.",
        )),
        4004 => Ended::Fatal(es("Discord rechazó la sesión de voz.")),
        _ => Ended::Fatal(es("Discord cerró la conexión de voz.")),
    }
}

struct Connection<'a> {
    credentials: &'a Credentials,
    /// What its diagnostic lines say it is ([`super::diag`]).
    scope: &'static str,
    phase: &'a watch::Sender<CallPhase>,
    port: &'a VoicePort,
    controls: Controls,
    socket: Socket,
    seq_ack: i64,
    heartbeat: Option<Duration>,
    next_heartbeat: Instant,
    /// Heartbeats in a row without their ACK; the second is a dead socket.
    missed: u8,
    awaiting_ack: bool,
    nonce: u64,
    signals: (Instant, u32),
    udp: Option<UdpSocket>,
    discovering: Option<Instant>,
    /// When IP discovery is asked again, and how many times it was.
    next_probe: Instant,
    probes: u32,
    ssrc: u32,
    transport: Option<Transport>,
    dave: Dave,
    /// Who sends from each SSRC, as the voice server announced.
    users: HashMap<u32, u64>,
    /// Carrying video: where its units go.
    video: Option<Video>,
    /// The cameras on screen, of the call's connection.
    wanted: Option<watch::Receiver<Vec<u64>>>,
    /// When op 15 is due again.
    next_wants: Instant,
    /// Each video SSRC and who sends it.
    streams: HashMap<u32, Source>,
    /// The cameras' SSRCs asked for in the last op 15: one layer each.
    layers: HashSet<u32>,
    /// Sharing the account's screen.
    sender: Option<Sender>,
    sequence: u16,
    timestamp: u32,
    started: Instant,
    speaking: bool,
    last_frame: Instant,
    silence: u8,
    next_keepalive: Instant,
    keepalive: u32,
    /// Connected was announced; until the socket drops.
    connected: bool,
    resuming: bool,
    /// When setup, or a DAVE negotiation with others, must have finished.
    deadline: Option<Instant>,
    // Desktop, language and local voice (P6).
    privacy: Option<watch::Sender<Option<String>>>,
    verify: Option<mpsc::Receiver<(u64, oneshot::Sender<Option<String>>)>>,
    // Voice protocol (voz-protocolo).
    /// Where the call's measures go; a stream's connection has none.
    quality: Option<watch::Sender<Option<CallQuality>>>,
    /// When the last heartbeat went out.
    beat_sent: Option<Instant>,
    /// The highest sequence number of each source of audio heard.
    heard: HashMap<u32, u16>,
    /// Audio packets due since the last measure, and those that came.
    expected: u32,
    received: u32,
}

/// Run the call until `controls` closes (`Ok`) or it cannot go on (`Err`,
/// with the reason in Spanish). `phase` follows its progress. With `video`
/// it carries video too: see the module.
pub(crate) async fn run(
    credentials: Credentials,
    port: VoicePort,
    video: Option<Video>,
    controls: watch::Receiver<Controls>,
    phase: watch::Sender<CallPhase>,
) -> Result<(), &'static str> {
    let scope = match &video {
        Some(Video {
            kind: VideoKind::Screen,
            send: Some(_),
            ..
        }) => "voice share",
        Some(Video {
            kind: VideoKind::Screen,
            ..
        }) => "voice stream",
        _ => "voice call",
    };
    let result = serve(scope, credentials, port, video, controls, phase).await;
    match result {
        Ok(()) => note!(scope, "connection closed: the call was left"),
        Err(reason) => note!(scope, "connection ended: {reason}"),
    }
    result
}

async fn serve(
    scope: &'static str,
    mut credentials: Credentials,
    port: VoicePort,
    mut video: Option<Video>,
    mut controls: watch::Receiver<Controls>,
    phase: watch::Sender<CallPhase>,
) -> Result<(), &'static str> {
    let _ = phase.send(CallPhase::Joining);
    // Only the call sends the microphone; a stream never does, and the
    // one sharing the screen sends its sound instead.
    let watching = video
        .as_ref()
        .is_some_and(|video| video.kind == VideoKind::Screen);
    let sound = video
        .as_ref()
        .and_then(|video| video.send.as_ref())
        .and_then(|send| send.sound.clone());
    let mut outgoing = match (watching, sound) {
        (false, _) => Some(port.outgoing.clone().lock_owned().await),
        (true, Some(sound)) => Some(sound.lock_owned().await),
        (true, None) => None,
    };
    // Frames from before this connection are not part of it.
    if let Some(outgoing) = outgoing.as_mut() {
        while outgoing.try_recv().is_ok() {}
    }
    let wanted = video.as_mut().and_then(|video| video.wanted.take());
    let kind = video.as_ref().map_or(VideoKind::Screen, |video| video.kind);
    let share = video.as_mut().and_then(|video| video.send.take());
    // The host's units, from this connection on.
    let mut units = match &share {
        Some(share) => Some(share.units.clone().lock_owned().await),
        None => None,
    };
    if let Some(units) = units.as_mut() {
        while units.try_recv().is_ok() {}
    }
    let mut random = [0; 6];
    getrandom::fill(&mut random).map_err(|_| es("No se pudo preparar la llamada."))?;
    let mut video_random = [0; 8];
    getrandom::fill(&mut video_random).map_err(|_| es("No se pudo preparar la llamada."))?;
    note!(
        scope,
        "opening the voice gateway (v=8) at {}",
        super::diag::host(&credentials.url)
    );
    let socket = open(scope, &credentials.url).await?;
    let codes = credentials.codes.take();
    let (privacy, verify, quality) = codes.map_or((None, None, None), |codes| {
        (Some(codes.privacy), Some(codes.verify), Some(codes.quality))
    });
    let mut connection = Connection {
        credentials: &credentials,
        scope,
        phase: &phase,
        port: &port,
        controls: *controls.borrow(),
        socket,
        seq_ack: -1,
        heartbeat: None,
        next_heartbeat: Instant::now(),
        missed: 0,
        awaiting_ack: false,
        nonce: 0,
        signals: (Instant::now(), 0),
        udp: None,
        discovering: None,
        next_probe: Instant::now(),
        probes: 0,
        ssrc: 0,
        transport: None,
        dave: Dave::new(credentials.user, credentials.channel)?,
        users: HashMap::new(),
        video,
        wanted,
        next_wants: Instant::now() + WANTS_REFRESH,
        streams: HashMap::new(),
        layers: HashSet::new(),
        sender: share.map(|share| Sender::new(share, kind, video_random)),
        sequence: u16::from_be_bytes([random[0], random[1]]),
        timestamp: u32::from_be_bytes([random[2], random[3], random[4], random[5]]),
        started: Instant::now(),
        speaking: false,
        last_frame: Instant::now(),
        silence: 0,
        next_keepalive: Instant::now(),
        keepalive: 0,
        connected: false,
        resuming: false,
        deadline: Some(Instant::now() + SETUP_TIMEOUT),
        privacy,
        verify,
        quality,
        beat_sent: None,
        heard: HashMap::new(),
        expected: 0,
        received: 0,
    };
    // `channel_id` as the official client says it: the call's channel, or
    // a stream's RTC channel.
    let mut identify = json!({
        "server_id": credentials.server.to_string(),
        "channel_id": credentials.channel.to_string(),
        "user_id": credentials.user.to_string(),
        "session_id": credentials.session.as_str(),
        "token": credentials.token.as_str(),
        "max_dave_protocol_version": 1,
    });
    if connection.video.is_some() {
        identify["video"] = json!(true);
        identify["streams"] = match connection.sender.as_ref().map(|sender| sender.kind) {
            // One screen, as the official client's stream connection says it.
            Some(VideoKind::Screen) => json!([{"type": "screen", "rid": "100", "quality": 100}]),
            // One camera, on or not, as its call connection says it.
            Some(VideoKind::Camera) => json!([{"type": "video", "rid": "100", "quality": 100}]),
            // Receives video and sends none: no streams of its own, as Serein.
            None => json!([]),
        };
    }
    connection
        .json(json!({"op": 0, "d": identify}))
        .await
        .map_err(Ended::reason)?;
    note!(
        scope,
        "IDENTIFY sent (op 0): max_dave_protocol_version 1, video {}, streams {}",
        identify["video"].as_bool().unwrap_or(false),
        identify["streams"].as_array().map_or(0, Vec::len)
    );
    drop(identify);
    let mut resumes = 0;
    let mut tick = interval(Duration::from_millis(20));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut datagram = vec![0u8; rtp::MAX_DATAGRAM + 1];
    loop {
        // A unit is taken only once the one before it went out, and none
        // while the shared screen is paused: the host stops encoding.
        let paused = connection.sharing() && connection.controls.paused;
        let pacing = connection.sender.as_ref().map(|sender| {
            (
                sender.pacer.idle(),
                sender.pacer.pending(),
                sender.next_pace,
            )
        });
        let ended = tokio::select! {
            unit = async {
                match units.as_mut().filter(|_| !paused && pacing.is_some_and(|(idle, ..)| idle)) {
                    Some(units) => units.recv().await,
                    None => std::future::pending().await,
                }
            } => match unit {
                Some(unit) => {
                    connection.send_unit(unit);
                    None
                }
                // The host let go of its end: a shared screen ends; the
                // call goes on without the camera.
                None if connection.sharing() => return Ok(()),
                None => {
                    units = None;
                    None
                }
            },
            _ = async {
                match pacing.filter(|(_, pending, _)| *pending) {
                    Some((.., at)) => sleep_until(at).await,
                    None => std::future::pending().await,
                }
            } => {
                connection.pace();
                None
            }
            changed = controls.changed() => {
                if changed.is_err() { return Ok(()); }
                connection.controls = *controls.borrow();
                connection.pause();
                None
            }
            _ = tick.tick() => connection.tick().await.err(),
            frame = async {
                match outgoing.as_mut() { Some(outgoing) => outgoing.recv().await, None => std::future::pending().await }
            } => match frame {
                Some(frame) => connection.send_frame(&frame).await.err(),
                None => return Ok(()),
            },
            // Someone's verification code (P6): scrypt, off this task.
            asked = async {
                match connection.verify.as_mut() { Some(verify) => verify.recv().await, None => std::future::pending().await }
            } => {
                match asked {
                    Some((user, answer)) => {
                        let fingerprints = connection.dave.fingerprints(user);
                        tokio::task::spawn_blocking(move || {
                            let _ = answer.send(fingerprints.and_then(super::dave::verification_code));
                        });
                    }
                    None => connection.verify = None,
                }
                None
            }
            key = async {
                match connection.video.as_mut() { Some(video) => video.keys.recv().await, None => std::future::pending().await }
            } => match key {
                Some(user) => {
                    connection.key_frame_of(user);
                    None
                }
                // Nobody left to ask: the call is going.
                None => return Ok(()),
            },
            changed = async {
                match connection.wanted.as_mut() { Some(wanted) => wanted.changed().await, None => std::future::pending().await }
            } => match changed {
                Ok(()) => connection.sink_wants().await.err(),
                Err(_) => return Ok(()),
            },
            received = async {
                match &connection.udp { Some(udp) => udp.recv(&mut datagram).await, None => std::future::pending().await }
            } => match received {
                Ok(length) => {
                    let mut ended = connection.datagram(&datagram[..length]).await.err();
                    // Drain what else waits, bounded, without awaiting.
                    for _ in 0..MAX_DATAGRAMS {
                        if ended.is_some() { break; }
                        let Some(udp) = &connection.udp else { break };
                        let Ok(length) = udp.try_recv(&mut datagram) else { break };
                        ended = connection.datagram(&datagram[..length]).await.err();
                    }
                    ended
                }
                Err(error) if transient(&error) => None,
                Err(error) => {
                    note!(scope, "UDP receive failed: {:?}", error.kind());
                    Some(Ended::Fatal(es("Se perdió la conexión de audio de la llamada.")))
                }
            },
            message = connection.socket.next() => match message {
                Some(Ok(message)) => connection.message(message).await.err(),
                Some(Err(error)) => {
                    note!(scope, "voice gateway socket failed: {}", super::diag::socket_error(&error));
                    Some(Ended::Dropped(es("Se cortó la conexión de voz.")))
                }
                None => {
                    note!(scope, "voice gateway socket ended without a close frame");
                    Some(Ended::Dropped(es("Se cortó la conexión de voz.")))
                }
            },
        };
        match ended {
            None => {
                if connection.connected && !connection.resuming {
                    resumes = 0;
                }
            }
            Some(Ended::Fatal(reason)) => return Err(reason),
            Some(Ended::Dropped(reason)) => loop {
                // Only an established session can be resumed.
                if connection.transport.is_none() {
                    note!(
                        scope,
                        "dropped before SESSION_DESCRIPTION: nothing to resume"
                    );
                    return Err(reason);
                }
                if resumes >= MAX_RESUMES {
                    note!(scope, "{MAX_RESUMES} resumes failed in a row");
                    return Err(LOST);
                }
                resumes += 1;
                connection.connected = false;
                note!(
                    scope,
                    "dropped ({reason}); resume {resumes}/{MAX_RESUMES} (op 7, seq_ack {})",
                    connection.seq_ack
                );
                let _ = phase.send(CallPhase::Reconnecting { attempt: resumes });
                // Leaving wins over a resumption still on its way.
                let closed = async { while controls.changed().await.is_ok() {} };
                tokio::select! {
                    _ = closed => return Ok(()),
                    resumed = async {
                        match connection.resume().await {
                            Ok(()) => true,
                            Err(ended) => {
                                note!(scope, "resume could not be sent: {}", ended.reason());
                                sleep(Duration::from_secs(u64::from(resumes))).await;
                                false
                            }
                        }
                    } => if resumed {
                        connection.controls = *controls.borrow();
                        break;
                    },
                }
            },
        }
    }
}

fn transient(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::WouldBlock
    )
}

async fn open(scope: &str, url: &str) -> Result<Socket, &'static str> {
    const FAILED: &str = es("No se pudo conectar con el servidor de voz.");
    let mut request = url.into_client_request().map_err(|_| FAILED)?;
    crate::browser::socket_headers(&mut request);
    let config = WebSocketConfig::default()
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_SIGNAL * 2)
        .max_message_size(Some(MAX_SIGNAL))
        .max_frame_size(Some(MAX_SIGNAL));
    let (socket, _) = match timeout(
        IO_TIMEOUT,
        connect_async_with_config(request, Some(config), true),
    )
    .await
    {
        Err(_) => {
            note!(
                scope,
                "voice gateway did not open in {} s",
                IO_TIMEOUT.as_secs()
            );
            return Err(FAILED);
        }
        Ok(Err(error)) => {
            note!(
                scope,
                "voice gateway did not open: {}",
                super::diag::socket_error(&error)
            );
            return Err(FAILED);
        }
        Ok(Ok(opened)) => opened,
    };
    note!(scope, "voice gateway open");
    Ok(socket)
}

fn number(data: &Value, key: &str) -> Result<u64, Ended> {
    data[key].as_u64().ok_or(MALFORMED)
}

fn user(data: &Value, key: &str) -> Result<u64, Ended> {
    data[key]
        .as_str()
        .and_then(|text| text.parse().ok())
        .filter(|id| *id != 0)
        .ok_or(MALFORMED)
}

const MALFORMED: Ended = Ended::Fatal(es("El servidor de voz envió un mensaje inválido."));

impl Ended {
    fn reason(self) -> &'static str {
        match self {
            Ended::Dropped(reason) | Ended::Fatal(reason) => reason,
        }
    }
}

impl From<&'static str> for Ended {
    fn from(reason: &'static str) -> Self {
        Ended::Fatal(reason)
    }
}

impl Connection<'_> {
    async fn json(&mut self, value: Value) -> Result<(), Ended> {
        // The serialized Identify/Resume holds the token: erase our copy.
        let text = Zeroizing::new(value.to_string());
        self.send(Message::Text(text.as_str().into())).await
    }

    async fn send(&mut self, message: Message) -> Result<(), Ended> {
        timeout(IO_TIMEOUT, self.socket.send(message))
            .await
            .map_err(|_| Ended::Dropped(es("La conexión de voz no responde.")))?
            .map_err(|_| Ended::Dropped(es("Se cortó la conexión de voz.")))
    }

    async fn replies(&mut self, replies: Vec<Reply>) -> Result<(), Ended> {
        for reply in replies {
            match reply {
                Reply::Transition { op, id } => {
                    note!(self.scope, "DAVE op {op} sent");
                    self.json(json!({"op": op, "d": {"transition_id": id}}))
                        .await?
                }
                Reply::Binary(bytes) => {
                    note!(
                        self.scope,
                        "DAVE op {} sent",
                        bytes.first().copied().unwrap_or_default()
                    );
                    self.send(Message::Binary(bytes.into())).await?
                }
            }
        }
        Ok(())
    }

    async fn resume(&mut self) -> Result<(), Ended> {
        self.socket = open(self.scope, &self.credentials.url)
            .await
            .map_err(Ended::Dropped)?;
        self.heartbeat = None;
        self.awaiting_ack = false;
        self.missed = 0;
        self.resuming = true;
        self.deadline = Some(Instant::now() + SETUP_TIMEOUT);
        let credentials = self.credentials;
        self.json(json!({"op": 7, "d": {
            "server_id": credentials.server.to_string(),
            "channel_id": credentials.channel.to_string(),
            "session_id": credentials.session.as_str(),
            "token": credentials.token.as_str(),
            "seq_ack": self.seq_ack,
        }}))
        .await
    }

    /// Ask the voice server where this machine's datagrams come from (IP
    /// discovery), again every [`DISCOVERY_RETRY`] until it answers.
    async fn probe(&mut self) -> Result<(), Ended> {
        let Some(udp) = &self.udp else {
            return Ok(());
        };
        self.probes += 1;
        self.next_probe = Instant::now() + DISCOVERY_RETRY;
        note!(
            self.scope,
            "IP discovery request sent (try {})",
            self.probes
        );
        match udp.send(&rtp::discovery_request(self.ssrc)).await {
            Ok(_) => Ok(()),
            // Only a path that keeps failing ends the call: the deadline.
            Err(error) if transient(&error) => Ok(()),
            Err(error) => {
                note!(
                    self.scope,
                    "IP discovery could not be sent: {:?}",
                    error.kind()
                );
                Err(Ended::Fatal(es("No se pudo abrir la conexión de audio.")))
            }
        }
    }

    /// Media may flow.
    fn ready(&self) -> bool {
        self.transport.is_some()
            && self.udp.is_some()
            && self.discovering.is_none()
            && !self.resuming
            && (self.dave.ready() || self.dave.alone())
    }

    async fn tick(&mut self) -> Result<(), Ended> {
        let now = Instant::now();
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            let waiting = if self.resuming {
                "RESUMED (op 9)"
            } else if self.heartbeat.is_none() {
                "HELLO (op 8)"
            } else if self.udp.is_none() {
                "READY (op 2)"
            } else if self.discovering.is_some() {
                "the IP discovery reply"
            } else if self.transport.is_none() {
                "SESSION_DESCRIPTION (op 4)"
            } else {
                "the DAVE group with the others"
            };
            note!(
                self.scope,
                "timed out ({} s) waiting for {waiting}",
                SETUP_TIMEOUT.as_secs()
            );
            return Err(if self.resuming {
                Ended::Dropped(es("El servidor de voz no retomó la llamada."))
            } else if self.transport.is_none() {
                Ended::Fatal(es("El servidor de voz no respondió."))
            } else {
                Ended::Fatal(es("No se pudo acordar el cifrado de la llamada."))
            });
        }
        if self.discovering.is_some_and(|deadline| now >= deadline) {
            note!(
                self.scope,
                "IP discovery: no reply after {} tries in {} s",
                self.probes,
                DISCOVERY_TIMEOUT.as_secs()
            );
            return Err(Ended::Fatal(es(
                "El servidor de voz no respondió por UDP; revisá el cortafuegos.",
            )));
        }
        if self.discovering.is_some() && now >= self.next_probe {
            self.probe().await?;
        }
        if let Some(interval) = self.heartbeat.filter(|_| now >= self.next_heartbeat) {
            if self.awaiting_ack {
                // A late wakeup may find the ACK unread: only a second
                // silent interval is a dead socket (Serein's rule).
                self.missed += 1;
                note!(
                    self.scope,
                    "heartbeat ACK (op 6) late ({} in a row)",
                    self.missed
                );
                if self.missed >= 2 {
                    return Err(Ended::Dropped(es("El servidor de voz dejó de responder.")));
                }
            }
            self.nonce = self.nonce.wrapping_add(1);
            let (nonce, seq_ack) = (self.nonce, self.seq_ack);
            self.json(json!({"op": 3, "d": {"t": nonce, "seq_ack": seq_ack}}))
                .await?;
            self.beat_sent = Some(now);
            self.awaiting_ack = true;
            self.next_heartbeat = now + interval;
        }
        if self.transport.is_some() && now >= self.next_wants {
            self.sink_wants().await?;
        }
        // Missing packets given up on, or due to be asked for again.
        let waiting: Vec<u32> = self
            .streams
            .iter()
            .filter(|(_, source)| source.reorder.pending())
            .map(|(ssrc, _)| *ssrc)
            .collect();
        for ssrc in waiting {
            self.repair(ssrc, now);
        }
        if let Some(sender) = self
            .sender
            .as_mut()
            .filter(|sender| now >= sender.next_rate)
        {
            sender.next_rate = now + RATE_STEP;
            if let Some(bitrate) = sender.rate.step() {
                // Full: the next step says it again.
                let _ = sender.feedback.try_send(Feedback::Bitrate(bitrate));
            }
        }
        let scope = self.scope;
        if let Some(sender) = self
            .sender
            .as_mut()
            .filter(|sender| now >= sender.next_report)
        {
            // What went out, for the owner to send when a stream is not fluid.
            if sender.announced {
                let elapsed = REPORT_EVERY + (now - sender.next_report);
                let line = sender
                    .traffic
                    .line(&sender.rate.seen, sender.rate.target(), elapsed);
                note!(scope, "{line}");
            }
            sender.next_report = now + REPORT_EVERY;
            sender.traffic = Traffic::default();
            sender.rate.seen = Default::default();
        }
        self.announce().await?;
        if self.discovering.is_none() && now >= self.next_keepalive {
            if let Some(udp) = &self.udp {
                // Keeps a quiet or receive-only path open through NAT, as Serein does.
                self.keepalive = self.keepalive.wrapping_add(1);
                let mut ping = [0x13, 0x37, 0xca, 0xfe, 0, 0, 0, 0];
                ping[4..].copy_from_slice(&self.keepalive.to_le_bytes());
                let _ = udp.try_send(&ping);
                self.next_keepalive = now + UDP_KEEPALIVE;
            }
        }
        // The call's privacy code (P6), as each group change leaves it.
        if let Some(privacy) = &self.privacy {
            let code = self.dave.privacy_code();
            privacy.send_if_modified(|told| {
                let changed = told.as_deref() != code;
                if changed {
                    *told = code.map(str::to_owned);
                }
                changed
            });
        }
        let ready = self.ready();
        if ready {
            self.deadline = None;
            // Alone, it waits for the others; with them, it is connected.
            let alone = self.dave.alone();
            let phase = if alone {
                CallPhase::Waiting
            } else {
                CallPhase::Connected
            };
            let changed = self.phase.send_if_modified(|now| {
                let changed = *now != phase;
                if changed {
                    *now = phase;
                }
                changed
            });
            if !self.connected {
                self.connected = true;
                note!(
                    self.scope,
                    "connected: media may flow ({})",
                    if alone {
                        "alone in the channel: waiting for the others"
                    } else {
                        "DAVE group ready"
                    }
                );
            } else if changed {
                note!(
                    self.scope,
                    "{}",
                    if alone {
                        "alone in the channel again: waiting for the others"
                    } else {
                        "someone is in the channel: connected"
                    }
                );
            }
        } else if self.connected && self.deadline.is_none() {
            // Someone joined an empty call: their group must form in time.
            note!(
                self.scope,
                "waiting for the DAVE group with someone who joined"
            );
            self.deadline = Some(now + SETUP_TIMEOUT);
        } else if self.transport.is_some() && !self.connected {
            // Transport ready; the end-to-end group with the others is next.
            let securing = self.phase.send_if_modified(|phase| {
                let joining = *phase == CallPhase::Joining;
                if joining {
                    *phase = CallPhase::Securing;
                }
                joining
            });
            if securing {
                note!(self.scope, "transport ready; waiting for the DAVE group");
            }
        }
        // A talk spurt ended: five silence frames, then no longer speaking.
        let muted = self.controls.muted && !self.sharing();
        if self.speaking && (muted || now - self.last_frame >= SPURT_GAP) {
            if self.silence == 0 {
                self.silence = SILENCE_FRAMES;
            }
            self.packet(&SILENCE)?;
            self.silence -= 1;
            if self.silence == 0 {
                self.speaking = false;
                let ssrc = self.ssrc;
                self.json(json!({"op": 5, "d": {"speaking": 0, "delay": 0, "ssrc": ssrc}}))
                    .await?;
            }
        }
        Ok(())
    }

    /// One 20 ms Opus frame from the host's microphone, or of the shared
    /// screen's sound (SOUNDSHARE, which muting the microphone leaves on).
    async fn send_frame(&mut self, frame: &[u8]) -> Result<(), Ended> {
        let sharing = self.sharing();
        let silent = if sharing {
            self.controls.paused
        } else {
            self.controls.muted || self.controls.deafened
        };
        if !self.ready() || silent {
            return Ok(());
        }
        if frame.is_empty() || frame.len() > 1275 {
            return Ok(());
        }
        let Some(sealed) = self.dave.encrypt(frame) else {
            // Alone, or the group is changing: nobody could decrypt it.
            return Ok(());
        };
        let now = Instant::now();
        if !self.speaking {
            self.speaking = true;
            self.silence = 0;
            let ssrc = self.ssrc;
            let flag = if sharing { SOUNDSHARE } else { VOICE };
            self.json(json!({"op": 5, "d": {"speaking": flag, "delay": 0, "ssrc": ssrc}}))
                .await?;
            // The timestamp follows the clock across the pause.
            let elapsed = (now - self.started).as_millis() as u32;
            self.timestamp = self.timestamp.wrapping_add(elapsed.wrapping_mul(48));
            self.started = now;
        }
        self.last_frame = now;
        self.packet(&sealed)
    }

    fn packet(&mut self, payload: &[u8]) -> Result<(), Ended> {
        let header = rtp::header(self.sequence, self.timestamp, self.ssrc);
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(960);
        self.started = Instant::now();
        let sharing = self.sharing();
        let (Some(transport), Some(udp)) = (self.transport.as_mut(), &self.udp) else {
            return Ok(());
        };
        // The shared screen's sound is marked as such (voz-protocolo).
        let packet = if sharing {
            let (id, value) = rtp::SCREEN_SOUND;
            transport.seal_extended(&header, (id, &[value]), payload)
        } else {
            transport.seal(&header, payload)
        }
        .ok_or(es("La llamada agotó sus claves. Volvé a entrar."))?;
        // A full send buffer drops this packet, as the network would.
        let _ = udp.try_send(&packet);
        Ok(())
    }

    /// Paused, what was on its way goes, and what follows the pause starts
    /// at a key frame.
    fn pause(&mut self) {
        let paused = self.sharing() && self.controls.paused;
        if let Some(sender) = self.sender.as_mut().filter(|_| paused) {
            sender.pacer.clear();
            sender.waiting = true;
        }
    }

    /// The connection shares the account's screen, rather than its camera.
    fn sharing(&self) -> bool {
        self.sender
            .as_ref()
            .is_some_and(|sender| sender.kind == VideoKind::Screen)
    }

    /// Tell the voice server what the account sends (op 12), once its SSRC
    /// and transport are known: the screen at once; the camera when it
    /// turns on, and `video_ssrc: 0` when it turns off again.
    async fn announce(&mut self) -> Result<(), Ended> {
        let on = self.sharing() || self.controls.camera;
        let audio = self.ssrc;
        let Some(sender) = self
            .sender
            .as_mut()
            .filter(|sender| sender.ssrc != 0 && sender.announced != on)
        else {
            return Ok(());
        };
        if self.transport.is_none() || self.resuming {
            return Ok(());
        }
        sender.announced = on;
        // Whatever was on its way is not part of what follows.
        sender.pacer.clear();
        sender.history.clear();
        sender.waiting = true;
        let (video, rtx) = (sender.ssrc, sender.rtx);
        let described = if on {
            // What it is: the receivers' sizes and rates follow.
            let (width, height) = sender.quality.size();
            let stream = json!({
                "type": if sender.kind == VideoKind::Screen { "screen" } else { "video" },
                "rid": "100", "ssrc": video, "rtx_ssrc": rtx,
                "active": true, "quality": 100,
                "max_bitrate": sender.rate.max(),
                "max_framerate": sender.quality.fps,
                "max_resolution": {"type": "fixed", "width": width, "height": height},
            });
            json!({"audio_ssrc": audio, "video_ssrc": video, "rtx_ssrc": rtx, "streams": [stream]})
        } else {
            json!({"audio_ssrc": audio, "video_ssrc": 0, "rtx_ssrc": 0, "streams": []})
        };
        self.json(json!({"op": 12, "d": described})).await
    }

    /// One unit of the account's screen or camera from the host: see the module.
    fn send_unit(&mut self, unit: VideoUnit) {
        let now = Instant::now();
        let ready = self.ready() && self.dave.ready();
        let Some(sender) = self.sender.as_mut() else {
            return;
        };
        if !sender.announced || !ready {
            // Nobody could decrypt it: the first one sent later must be a key frame.
            sender.waiting = true;
            sender.traffic.dropped += 1;
            return;
        }
        if sender.waiting && !unit.keyframe {
            sender.key_frame(now);
            sender.traffic.dropped += 1;
            return;
        }
        let normalized: Vec<u8> = h264::nals(&unit.data)
            .into_iter()
            .flat_map(|nal| {
                let nal = sps::normalize(nal).unwrap_or_else(|| nal.to_vec());
                [vec![0, 0, 0, 1], nal]
            })
            .flatten()
            .collect();
        let payloads = match self.dave.encrypt_video(&normalized) {
            Some(sealed) => h264::packetize(&sealed),
            None => Vec::new(),
        };
        if payloads.is_empty() || payloads.len() > h264::MAX_PACKETS {
            sender.waiting = true;
            sender.key_frame(now);
            return;
        }
        sender.waiting = false;
        sender.traffic.units += 1;
        sender.traffic.keyframes += u32::from(unit.keyframe);
        let elapsed = unit.at.saturating_duration_since(sender.since);
        let timestamp = sender
            .clock
            .wrapping_add((elapsed.as_micros() * 9 / 100) as u32);
        let last = payloads.len() - 1;
        let (ssrc, mut sequence) = (sender.ssrc, sender.sequence);
        sender
            .pacer
            .push(payloads.into_iter().enumerate().map(|(at, payload)| {
                let header = rtp::video_header(rtp::H264, at == last, sequence, timestamp, ssrc);
                sequence = sequence.wrapping_add(1);
                (header, payload)
            }));
        sender.sequence = sequence;
        sender.next_pace = now;
    }

    /// The packets of the unit on its way that fit the budget now.
    fn pace(&mut self) {
        let now = Instant::now();
        let Some(sender) = self.sender.as_mut() else {
            return;
        };
        let batch = sender.pacer.batch(now, sender.rate.target());
        sender.next_pace = now + PACE;
        let (Some(transport), Some(udp)) = (self.transport.as_mut(), &self.udp) else {
            return sender.pacer.clear();
        };
        for (header, payload) in batch {
            let media = header[1] & 0x7f == rtp::H264;
            if let Some(packet) = transport.seal(&header, &payload) {
                // A full send buffer drops this packet, as the network would.
                let _ = udp.try_send(&packet);
                let traffic = &mut sender.traffic;
                if media {
                    traffic.packets += 1;
                    traffic.bytes += packet.len() as u64;
                } else {
                    traffic.repairs += 1;
                    traffic.repair_bytes += packet.len() as u64;
                }
            }
            // Kept for the NACKs; a retransmission is not kept again.
            if media {
                sender.history.remember((header, payload), now);
            }
        }
    }

    async fn datagram(&mut self, packet: &[u8]) -> Result<(), Ended> {
        if packet.len() > rtp::MAX_DATAGRAM {
            return Ok(());
        }
        if self.discovering.is_some() {
            let Some((address, port)) = rtp::discovery_reply(packet, self.ssrc) else {
                return Ok(());
            };
            self.discovering = None;
            // The address is the account's own: never in a line.
            note!(
                self.scope,
                "IP discovery reply received (try {}, {})",
                self.probes,
                if address.is_ipv4() { "IPv4" } else { "IPv6" }
            );
            let mut select = json!({
                "protocol": "udp",
                "data": {"address": address.to_string(), "port": port, "mode": rtp::MODE},
            });
            if self.video.is_some() {
                // H264 only, numbered as the official client does: decoded,
                // encoded by the connection sharing the screen, or both by
                // the call's, which sends the camera.
                let (encode, decode) = (self.sender.is_some(), !self.sharing());
                select["codecs"] = json!([
                    {"name": "opus", "type": "audio", "priority": 1000, "payload_type": rtp::OPUS},
                    {"name": "H264", "type": "video", "priority": 1000, "payload_type": rtp::H264,
                     "rtx_payload_type": rtp::RTX, "encode": encode, "decode": decode},
                ]);
            }
            self.json(json!({"op": 1, "d": select})).await?;
            note!(
                self.scope,
                "SELECT_PROTOCOL sent (op 1): udp, mode {}, codecs {}",
                rtp::MODE,
                if self.video.is_some() {
                    "opus + H264"
                } else {
                    "none listed (opus)"
                }
            );
            return Ok(());
        }
        if let (Some(sender), Some(transport)) = (self.sender.as_mut(), self.transport.as_ref()) {
            if packet.len() > 1 && (200..=206).contains(&packet[1]) {
                let now = Instant::now();
                let compound = transport.open_rtcp(packet).unwrap_or_default();
                for report in rtp::reports(&compound, sender.ssrc) {
                    match report {
                        Report::KeyFrame => {
                            sender.traffic.asked += 1;
                            sender.key_frame(now);
                        }
                        Report::Nack { first, mask } => sender.resend(first, mask, now),
                        report => sender.rate.report(report),
                    }
                }
                return Ok(());
            }
        }
        let Some(rtp) = self.transport.as_ref().and_then(|t| t.open(packet)) else {
            return Ok(());
        };
        match rtp.payload_type {
            rtp::H264 => {
                self.video_packet(rtp);
                return Ok(());
            }
            rtp::RTX => {
                self.retransmission(rtp);
                return Ok(());
            }
            _ => {}
        }
        self.heard(rtp.ssrc, rtp.sequence);
        if self.controls.deafened {
            return Ok(());
        }
        let Some(&user) = self.users.get(&rtp.ssrc) else {
            return Ok(());
        };
        let Some(opus) = self.dave.decrypt(user, &rtp.payload) else {
            return Ok(());
        };
        if opus == SILENCE {
            return Ok(());
        }
        // A full queue means the host is behind: drop, as the network would.
        let _ = self.port.incoming.try_send(VoicePacket {
            user: UserId(user),
            sequence: rtp.sequence,
            opus,
            stream: self
                .video
                .as_ref()
                .is_some_and(|video| video.kind == VideoKind::Screen),
        });
        Ok(())
    }

    /// A retransmission (RFC 4588): the packet it restores goes where it
    /// would have; the sender's padding, with nothing restored, goes nowhere.
    fn retransmission(&mut self, mut rtx: rtp::Rtp) {
        let Some((&ssrc, _)) = self
            .streams
            .iter()
            .find(|(_, source)| source.rtx == rtx.ssrc)
        else {
            return;
        };
        if rtx.payload.len() <= 2 {
            return;
        }
        let original: Vec<u8> = rtx.payload.drain(..2).collect();
        rtx.sequence = u16::from_be_bytes([original[0], original[1]]);
        rtx.ssrc = ssrc;
        rtx.payload_type = rtp::H264;
        self.video_packet(rtx);
    }

    /// A packet of someone's video: put in order with the others; a gap
    /// waits for its retransmissions. A camera off screen is not even
    /// assembled.
    fn video_packet(&mut self, rtp: rtp::Rtp) {
        let ssrc = rtp.ssrc;
        let Some(source) = self.streams.get_mut(&ssrc) else {
            return;
        };
        // Only the layer asked for of a camera on screen.
        if self.wanted.is_some() && !self.layers.contains(&ssrc) {
            return;
        }
        let now = Instant::now();
        source.reorder.push(rtp, now);
        self.repair(ssrc, now);
    }

    /// What the order of `ssrc` lets go of now is assembled; what it still
    /// misses is asked for again (NACK).
    fn repair(&mut self, ssrc: u32, now: Instant) {
        let Some(source) = self.streams.get_mut(&ssrc) else {
            return;
        };
        let (ready, missing) = source.reorder.poll(now);
        for rtp in ready {
            self.assemble(&rtp, now);
        }
        if missing.is_empty() {
            return;
        }
        let (Some(transport), Some(udp)) = (self.transport.as_mut(), &self.udp) else {
            return;
        };
        let nack = rtp::nack(self.ssrc, ssrc, &missing);
        if let Some(packet) = transport.seal(&nack[..8], &nack[8..]) {
            let _ = udp.try_send(&packet);
        }
    }

    /// A packet of someone's video, in order: a complete access unit,
    /// decrypted, goes to the host; a lost or dropped one asks for a key
    /// frame.
    fn assemble(&mut self, rtp: &rtp::Rtp, now: Instant) {
        let Some(video) = &self.video else {
            return;
        };
        let Some(source) = self.streams.get_mut(&rtp.ssrc) else {
            return;
        };
        let user = source.user;
        let shown = match source.assembly.push(
            rtp.sequence,
            rtp.timestamp,
            rtp.marker,
            &rtp.payload,
            now.into_std(),
        ) {
            Ok(None) => true,
            Err(_) => false,
            Ok(Some(unit)) => match self.dave.decrypt_video(user, &unit.data) {
                // A full queue: the host is behind, and drops until a key frame.
                Some(data) => video
                    .incoming
                    .try_send(VideoUnit {
                        user: UserId(user),
                        kind: video.kind,
                        keyframe: unit.keyframe,
                        at: now.into_std(),
                        data,
                    })
                    .is_ok(),
                None => false,
            },
        };
        let Some(source) = self.streams.get_mut(&rtp.ssrc) else {
            return;
        };
        if shown {
            source.shown = Some(now);
        } else {
            source.assembly.wait();
            self.key_frame(rtp.ssrc);
        }
    }

    /// Ask for a key frame of every stream `user` sends.
    fn key_frame_of(&mut self, user: u64) {
        let ssrcs: Vec<u32> = self
            .streams
            .iter()
            .filter(|(_, source)| source.user == user)
            .map(|(ssrc, _)| *ssrc)
            .collect();
        for ssrc in ssrcs {
            self.key_frame(ssrc);
        }
    }

    /// Ask the sender of `ssrc` for a key frame (RTCP PLI), at most every
    /// [`PLI_GAP`].
    fn key_frame(&mut self, ssrc: u32) {
        let now = Instant::now();
        let Some(Source { asked, .. }) = self.streams.get_mut(&ssrc) else {
            return;
        };
        if asked.is_some_and(|at| now - at < PLI_GAP) {
            return;
        }
        let (Some(transport), Some(udp)) = (self.transport.as_mut(), &self.udp) else {
            return;
        };
        *asked = Some(now);
        let pli = rtp::pli(self.ssrc, ssrc);
        if let Some(packet) = transport.seal(&pli[..8], &pli[8..]) {
            let _ = udp.try_send(&packet);
        }
    }

    fn signal_budget(&mut self) -> Result<(), Ended> {
        let now = Instant::now();
        if now - self.signals.0 >= Duration::from_secs(1) {
            self.signals = (now, 0);
        }
        self.signals.1 += 1;
        if self.signals.1 > MAX_SIGNALS_PER_SECOND {
            return Err(Ended::Fatal(es(
                "El servidor de voz envió demasiados mensajes.",
            )));
        }
        Ok(())
    }

    async fn message(&mut self, message: Message) -> Result<(), Ended> {
        self.signal_budget()?;
        match message {
            Message::Text(text) => {
                let mut event: Value = serde_json::from_str(&text).map_err(|_| MALFORMED)?;
                if let Some(seq) = event["seq"].as_i64() {
                    self.seq_ack = seq;
                }
                let opcode = number(&event, "op")?;
                let data = event["d"].take();
                let result = self.text(opcode, data).await;
                if let Err(Ended::Fatal(reason)) = &result {
                    note!(self.scope, "opcode {opcode} ended the connection: {reason}");
                }
                result
            }
            Message::Binary(bytes) => {
                if bytes.len() < 3 {
                    return Err(MALFORMED);
                }
                self.seq_ack = i64::from(u16::from_be_bytes([bytes[0], bytes[1]]));
                let (opcode, data) = (bytes[2], &bytes[3..]);
                let scope = self.scope;
                note!(scope, "DAVE op {opcode} received");
                let replies = match opcode {
                    op::EXTERNAL_SENDER => self.dave.external_sender(data).map(|()| Vec::new()),
                    op::PROPOSALS => self
                        .dave
                        .proposals(data)
                        .map(|reply| reply.into_iter().collect()),
                    op::ANNOUNCE_COMMIT | op::WELCOME => self.dave.group_changed(opcode, data),
                    // What this client does not know is not a reason to drop the call.
                    _ => {
                        note!(scope, "  unknown binary opcode {opcode}: ignored");
                        Ok(Vec::new())
                    }
                }
                .inspect_err(|reason| {
                    note!(scope, "DAVE op {opcode} ended the connection: {reason}")
                })?;
                Ok(self.replies(replies).await?)
            }
            Message::Ping(data) => Ok(self.send(Message::Pong(data)).await?),
            Message::Close(Some(frame)) => {
                note!(
                    self.scope,
                    "voice gateway closed by Discord: code {} «{}»",
                    u16::from(frame.code),
                    super::diag::reason(&frame.reason)
                );
                Err(close_reason(u16::from(frame.code)))
            }
            Message::Close(None) => {
                note!(self.scope, "voice gateway closed by Discord without a code");
                Err(Ended::Dropped(es("Se cortó la conexión de voz.")))
            }
            Message::Pong(_) => Ok(()),
            Message::Frame(_) => Err(MALFORMED),
        }
    }

    async fn text(&mut self, opcode: u64, data: Value) -> Result<(), Ended> {
        match opcode {
            // Hello.
            8 => {
                let interval = data["heartbeat_interval"]
                    .as_f64()
                    .filter(|ms| ms.is_finite() && (100.0..=120_000.0).contains(ms))
                    .ok_or(MALFORMED)?;
                note!(
                    self.scope,
                    "HELLO (op 8): heartbeat every {} ms (sent every {} ms)",
                    interval as u64,
                    (interval as u64).min(5_000)
                );
                // Beat at least every 5 s: a zombie socket is found sooner.
                let interval = Duration::from_millis((interval as u64).min(5_000));
                self.heartbeat = Some(interval);
                self.next_heartbeat = Instant::now();
            }
            // Heartbeat ACK: the latest one's echo measures the call.
            6 => {
                self.awaiting_ack = false;
                self.missed = 0;
                if data["t"].as_u64() == Some(self.nonce) {
                    if let Some(sent) = self.beat_sent.take() {
                        self.measured(sent.elapsed());
                    }
                }
            }
            // Ready.
            2 => {
                if self.udp.is_some() {
                    note!(
                        self.scope,
                        "READY (op 2) again on the same connection: refused"
                    );
                    return Err(MALFORMED);
                }
                self.ssrc = u32::try_from(number(&data, "ssrc")?).map_err(|_| MALFORMED)?;
                let sharing = self.sharing();
                if let Some(sender) = self.sender.as_mut() {
                    let ssrc = |value: &Value| {
                        value
                            .as_u64()
                            .and_then(|ssrc| u32::try_from(ssrc).ok())
                            .filter(|ssrc| *ssrc != 0)
                    };
                    let stream = data["streams"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .take(4)
                        .find_map(|stream| {
                            Some((ssrc(&stream["ssrc"])?, ssrc(&stream["rtx_ssrc"])))
                        });
                    match stream.filter(|(video, _)| *video != self.ssrc) {
                        Some((video, rtx)) => {
                            sender.ssrc = video;
                            // The official client's rule when none is named.
                            sender.rtx = rtx.unwrap_or(video.wrapping_add(1));
                        }
                        None if sharing => {
                            return Err(Ended::Fatal(es(
                                "Discord no le dio lugar a la pantalla en la transmisión.",
                            )))
                        }
                        // The call goes on; the camera cannot be sent in it.
                        None => {}
                    }
                }
                let address: IpAddr = data["ip"]
                    .as_str()
                    .and_then(|ip| ip.parse().ok())
                    .ok_or(MALFORMED)?;
                if !(public(address) || (self.credentials.local() && address.is_loopback())) {
                    return Err(Ended::Fatal(es(
                        "El servidor de voz indicó una dirección no pública.",
                    )));
                }
                let port = u16::try_from(number(&data, "port")?)
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or(MALFORMED)?;
                let modes: Vec<&str> = data["modes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .take(16)
                    .collect();
                let offered = modes.contains(&rtp::MODE);
                note!(
                    self.scope,
                    "READY (op 2): modes offered [{}]; {}",
                    modes
                        .iter()
                        .map(|mode| super::diag::reason(mode))
                        .collect::<Vec<_>>()
                        .join(", "),
                    if offered {
                        "RUNE uses aead_xchacha20_poly1305_rtpsize"
                    } else {
                        "none RUNE supports"
                    }
                );
                if !offered {
                    return Err(Ended::Fatal(es(
                        "El servidor de voz no ofrece el cifrado requerido.",
                    )));
                }
                const UDP: &str = es("No se pudo abrir la conexión de audio.");
                let udp = UdpSocket::bind(if address.is_ipv4() {
                    "0.0.0.0:0"
                } else {
                    "[::]:0"
                })
                .await
                .map_err(|_| UDP)?;
                udp.connect(SocketAddr::new(address, port))
                    .await
                    .map_err(|_| UDP)?;
                self.udp = Some(udp);
                self.discovering = Some(Instant::now() + DISCOVERY_TIMEOUT);
                self.probes = 0;
                self.probe().await?;
            }
            // Session Description.
            4 => {
                if self.transport.is_some() || self.udp.is_none() || self.discovering.is_some() {
                    note!(
                        self.scope,
                        "SESSION_DESCRIPTION (op 4) out of order: refused"
                    );
                    return Err(MALFORMED);
                }
                note!(
                    self.scope,
                    "SESSION_DESCRIPTION (op 4): mode {}, dave_protocol_version {}, video codec {}",
                    data["mode"]
                        .as_str()
                        .map_or_else(|| "none".into(), super::diag::reason),
                    data["dave_protocol_version"]
                        .as_u64()
                        .map_or_else(|| "none".into(), |v| v.to_string()),
                    data["video_codec"]
                        .as_str()
                        .map_or_else(|| "none".into(), super::diag::reason)
                );
                if data["mode"].as_str() != Some(rtp::MODE) {
                    return Err(Ended::Fatal(es("El servidor de voz eligió otro cifrado.")));
                }
                if data["dave_protocol_version"].as_u64() != Some(1) {
                    return Err(Ended::Fatal(es(
                        "Discord no acordó cifrado de extremo a extremo; la llamada no empezó.",
                    )));
                }
                let other_codec = data["video_codec"]
                    .as_str()
                    .is_some_and(|codec| codec != "H264");
                if other_codec && self.sharing() {
                    return Err(Ended::Fatal(es(
                        "Discord eligió otro códec de video para la transmisión.",
                    )));
                }
                if let Some(sender) = self.sender.as_mut().filter(|_| other_codec) {
                    // The call goes on; its camera would reach nobody.
                    sender.ssrc = 0;
                }
                let bytes = data["secret_key"].as_array().ok_or(MALFORMED)?;
                if bytes.len() != 32 {
                    return Err(MALFORMED);
                }
                let mut key = Zeroizing::new([0u8; 32]);
                for (out, byte) in key.iter_mut().zip(bytes) {
                    *out = byte
                        .as_u64()
                        .and_then(|byte| u8::try_from(byte).ok())
                        .ok_or(MALFORMED)?;
                }
                drop(data);
                self.transport = Some(Transport::new(&key));
                let ssrc = self.ssrc;
                // Announces the SSRC before any media.
                self.json(json!({"op": 5, "d": {"speaking": 0, "delay": 0, "ssrc": ssrc}}))
                    .await?;
                let package = self.dave.key_package()?;
                self.send(Message::Binary(package.into())).await?;
                note!(
                    self.scope,
                    "SPEAKING (op 5, off) and DAVE op 26 (key package) sent"
                );
                self.sink_wants().await?;
                self.announce().await?;
            }
            // Speaking: who sends from which SSRC.
            5 => {
                let who = user(&data, "user_id")?;
                let ssrc = u32::try_from(number(&data, "ssrc")?).map_err(|_| MALFORMED)?;
                if who != self.credentials.user {
                    self.map(ssrc, who);
                }
            }
            // Resumed.
            9 => {
                if !self.resuming {
                    note!(self.scope, "RESUMED (op 9) without a resume: refused");
                    return Err(MALFORMED);
                }
                note!(self.scope, "RESUMED (op 9)");
                self.resuming = false;
                self.speaking = false;
                // The SSRC again before any media, as Serein does after a resume.
                let ssrc = self.ssrc;
                self.json(json!({"op": 5, "d": {"speaking": 0, "delay": 0, "ssrc": ssrc}}))
                    .await?;
            }
            // Clients connected.
            11 => {
                let users = data["user_ids"]
                    .as_array()
                    .ok_or(MALFORMED)?
                    .iter()
                    .map(|id| id.as_str().and_then(|id| id.parse().ok()).ok_or(MALFORMED))
                    .collect::<Result<Vec<u64>, _>>()?;
                note!(self.scope, "CLIENTS_CONNECT (op 11): {} more", users.len());
                self.dave.connected(&users)?;
            }
            // A client's media streams, including its audio SSRC.
            12 => {
                let who = user(&data, "user_id")?;
                if let Some(ssrc) = data["audio_ssrc"]
                    .as_u64()
                    .and_then(|s| u32::try_from(s).ok())
                {
                    if ssrc != 0 && who != self.credentials.user {
                        self.map(ssrc, who);
                    }
                }
                if self.video.is_some()
                    && who != self.credentials.user
                    && self.video_ssrcs(who, &data)
                {
                    self.sink_wants().await?;
                }
            }
            // Client disconnected.
            13 => {
                let who = user(&data, "user_id")?;
                note!(self.scope, "CLIENT_DISCONNECT (op 13)");
                self.users.retain(|_, user| *user != who);
                self.streams.retain(|_, source| source.user != who);
                self.dave.disconnected(who)?;
            }
            op::PREPARE_TRANSITION => {
                note!(self.scope, "DAVE op {opcode} received");
                let id = u16::try_from(number(&data, "transition_id")?).map_err(|_| MALFORMED)?;
                let replies = self
                    .dave
                    .prepare_transition(number(&data, "protocol_version")?, id)?;
                self.replies(replies).await?;
            }
            op::EXECUTE_TRANSITION => {
                note!(self.scope, "DAVE op {opcode} received");
                let id = u16::try_from(number(&data, "transition_id")?).map_err(|_| MALFORMED)?;
                self.dave.execute_transition(id);
            }
            op::PREPARE_EPOCH => {
                note!(self.scope, "DAVE op {opcode} received");
                let reply = self
                    .dave
                    .prepare_epoch(number(&data, "protocol_version")?, number(&data, "epoch")?)?;
                self.replies(reply.into_iter().collect()).await?;
            }
            // Codecs, sink wants, client flags and platform: nothing to do.
            14..=20 => {}
            // What this client does not know is not a reason to drop the call.
            _ => note!(self.scope, "unknown opcode {opcode}: ignored"),
        }
        Ok(())
    }

    // Voice protocol (voz-protocolo).
    /// One audio packet of the call heard from `ssrc`: what its sequence
    /// number skipped is due and missing, unless a late one fills it.
    fn heard(&mut self, ssrc: u32, sequence: u16) {
        if self.quality.is_none() {
            return;
        }
        if self.heard.len() >= MAX_HEARD && !self.heard.contains_key(&ssrc) {
            // shortcut: forgets every source at once; fine while a call has fewer.
            self.heard.clear();
        }
        let due = match self.heard.get_mut(&ssrc) {
            None => {
                self.heard.insert(ssrc, sequence);
                1
            }
            Some(highest) => match sequence.wrapping_sub(*highest) {
                // A duplicate.
                0 => return,
                ahead @ 1..=MAX_GAP => {
                    *highest = sequence;
                    u32::from(ahead)
                }
                // Behind: a late one, already counted as due.
                0x8000.. => 0,
                // Far ahead: it started over.
                _ => {
                    *highest = sequence;
                    1
                }
            },
        };
        self.expected = self.expected.saturating_add(due);
        self.received = self.received.saturating_add(1);
    }

    /// A heartbeat came back after `rtt`: the call's measures since the
    /// last one go to the host.
    fn measured(&mut self, rtt: Duration) {
        let (expected, received) = (self.expected, self.received);
        (self.expected, self.received) = (0, 0);
        let Some(quality) = self.quality.as_ref().filter(|_| self.connected) else {
            return;
        };
        let lost = expected.saturating_sub(received);
        let loss_permille = (u64::from(lost) * 1000 / u64::from(expected.max(1))) as u16;
        quality.send_replace(Some(CallQuality {
            rtt_ms: u32::try_from(rtt.as_millis()).unwrap_or(u32::MAX),
            loss_permille,
        }));
    }

    /// The video SSRCs op 12 announces for `who`, with their RTX SSRCs:
    /// `video_ssrc` and those of `streams` that are not audio. Zero takes
    /// theirs away. Whether any is new.
    fn video_ssrcs(&mut self, who: u64, data: &Value) -> bool {
        let ssrc = |value: &Value| value.as_u64().and_then(|ssrc| u32::try_from(ssrc).ok());
        let mut announced: Vec<(u32, Option<u32>, u8)> = ssrc(&data["video_ssrc"])
            .map(|video| (video, ssrc(&data["rtx_ssrc"]), 100))
            .into_iter()
            .collect();
        // A layer's own entry says what it is, over the top-level one.
        for stream in data["streams"].as_array().into_iter().flatten().take(4) {
            if matches!(stream["type"].as_str(), None | Some("video" | "screen")) {
                let quality = stream["quality"]
                    .as_u64()
                    .map_or(100, |quality| quality.min(100) as u8);
                if let Some(video) = ssrc(&stream["ssrc"]) {
                    announced.retain(|(known, ..)| *known != video);
                    announced.push((video, ssrc(&stream["rtx_ssrc"]), quality));
                }
            }
        }
        if announced.iter().any(|(video, ..)| *video == 0) {
            self.streams.retain(|_, source| source.user != who);
        }
        let mut new = false;
        for (ssrc, rtx, quality) in announced.into_iter().filter(|(ssrc, ..)| *ssrc != 0) {
            let known = self
                .streams
                .get(&ssrc)
                .is_some_and(|source| source.user == who);
            if !known && self.streams.len() < 2 * super::dave::MAX_MEMBERS {
                let source = Source {
                    user: who,
                    quality,
                    // The official client's rule when none is named.
                    rtx: rtx.filter(|rtx| *rtx != 0).unwrap_or(ssrc.wrapping_add(1)),
                    reorder: Reorder::default(),
                    assembly: Depacketizer::default(),
                    asked: None,
                    shown: None,
                };
                self.streams.insert(ssrc, source);
                new = true;
            }
        }
        new
    }

    /// Tell the voice server which video to forward (op 15): see the
    /// module. When to repeat it is not confirmed (docs/REA.md).
    async fn sink_wants(&mut self) -> Result<(), Ended> {
        // Only those who watch say what they want; the one sharing does not.
        if self.video.is_none() || self.sharing() {
            return Ok(());
        }
        let now = Instant::now();
        let wanted = self.wanted.as_ref().map(|wanted| wanted.borrow().clone());
        let stalled = self.streams.values().any(|source| {
            wanted
                .as_ref()
                .is_none_or(|wanted| wanted.contains(&source.user))
                && source.shown.is_none_or(|at| now - at >= WANTS_STALLED)
        });
        self.next_wants = now
            + if stalled {
                WANTS_STALLED
            } else {
                WANTS_REFRESH
            };
        if let Some(wanted) = &wanted {
            self.layers = layers(&self.streams, wanted);
        }
        match sink_wants(&self.streams, wanted.as_deref()) {
            Some(wants) => self.json(json!({"op": 15, "d": wants})).await,
            None => Ok(()),
        }
    }

    fn map(&mut self, ssrc: u32, user: u64) {
        if self.users.len() < 2 * super::dave::MAX_MEMBERS || self.users.contains_key(&ssrc) {
            self.users.retain(|_, known| *known != user);
            self.users.insert(ssrc, user);
        }
    }
}

/// The camera SSRCs to ask for: one of each person `wanted`, the best of
/// their layers while at most [`BEST_LAYERS`] cameras show, the lightest
/// past that.
fn layers(streams: &HashMap<u32, Source>, wanted: &[u64]) -> HashSet<u32> {
    let light = wanted.len() > BEST_LAYERS;
    wanted
        .iter()
        .filter_map(|user| {
            let theirs = streams.iter().filter(|(_, source)| source.user == *user);
            let pick = |(ssrc, source): (&u32, &Source)| (source.quality, *ssrc);
            if light {
                theirs.map(pick).min()
            } else {
                theirs.map(pick).max()
            }
            .map(|(_, ssrc)| ssrc)
        })
        .collect()
}

/// What op 15 says: every stream at full quality while watching one
/// (`wanted` is `None`); else 100 for the one layer of each camera of
/// someone `wanted` ([`layers`]) and 0 for the rest, and nothing while no
/// camera is known. No `any` there: it would speak for SSRCs that are not
/// cameras (not confirmed).
fn sink_wants(streams: &HashMap<u32, Source>, wanted: Option<&[u64]>) -> Option<Value> {
    let Some(wanted) = wanted else {
        return Some(json!({"any": 100}));
    };
    let asked = layers(streams, wanted);
    let wants: Map<String, Value> = streams
        .keys()
        .map(|ssrc| {
            let quality = if asked.contains(ssrc) { 100 } else { 0 };
            (ssrc.to_string(), json!(quality))
        })
        .collect();
    (!wants.is_empty()).then_some(Value::Object(wants))
}

#[cfg(test)]
mod endpoint_tests {
    use super::*;

    #[test]
    fn op_15_asks_for_the_cameras_on_screen_only() {
        let source = |user| Source {
            user,
            quality: 100,
            rtx: 0,
            reorder: Reorder::default(),
            assembly: Depacketizer::default(),
            asked: None,
            shown: None,
        };
        assert_eq!(sink_wants(&HashMap::new(), None), Some(json!({"any": 100})));
        assert_eq!(sink_wants(&HashMap::new(), Some(&[2])), None);
        let streams = HashMap::from([(23, source(2)), (33, source(3)), (43, source(4))]);
        assert_eq!(
            sink_wants(&streams, Some(&[2, 4])),
            Some(json!({"23": 100, "33": 0, "43": 100}))
        );
        assert_eq!(
            sink_wants(&streams, Some(&[])),
            Some(json!({"23": 0, "33": 0, "43": 0}))
        );
        // A camera in two layers: its best while two show, its lightest past that.
        let mut layered = streams;
        layered.insert(
            24,
            Source {
                quality: 50,
                ..source(2)
            },
        );
        assert_eq!(
            sink_wants(&layered, Some(&[2, 4])),
            Some(json!({"23": 100, "24": 0, "33": 0, "43": 100}))
        );
        assert_eq!(
            sink_wants(&layered, Some(&[2, 3, 4])),
            Some(json!({"23": 0, "24": 100, "33": 100, "43": 100}))
        );
        assert_eq!(layers(&layered, &[2]), HashSet::from([23]));
    }

    #[test]
    fn only_discord_media_hosts_and_public_addresses_are_accepted() {
        assert_eq!(
            endpoint("c-eze02-1a2b.discord.media:443").unwrap(),
            "wss://c-eze02-1a2b.discord.media/?v=8"
        );
        // Other ports are Discord's too: kept.
        assert_eq!(
            endpoint("sweetwater-12345.discord.media:2048").unwrap(),
            "wss://sweetwater-12345.discord.media:2048/?v=8"
        );
        assert_eq!(
            endpoint("c-eze02-1a2b.discord.media").unwrap(),
            "wss://c-eze02-1a2b.discord.media/?v=8"
        );
        // Named on 80, reached on 443, as the official client does.
        assert_eq!(
            endpoint("c-eze02-1a2b.discord.media:80").unwrap(),
            "wss://c-eze02-1a2b.discord.media/?v=8"
        );
        for bad in [
            "",
            "evil.example",
            "evil.example:443",
            "discord.media.evil.example",
            "x.discord.media/path",
            "user@x.discord.media",
            "x.discord.media:",
            "x.discord.media:0",
            "x.discord.media:+443",
            "x.discord.media:70000",
            "x.discord.media:443:443",
            "x.discord.media?v=1",
        ] {
            assert!(endpoint(bad).is_err(), "{bad}");
        }
        for ip in [
            "10.0.0.1",
            "127.0.0.1",
            "192.168.1.1",
            "100.64.0.1",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:8.8.8.8",
        ] {
            assert!(!public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "2606:4700::1111"] {
            assert!(public(ip.parse().unwrap()), "{ip}");
        }
    }
}
