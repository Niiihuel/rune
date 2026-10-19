//! The video path between a provider's transport and the host's decoder:
//! H264 access units in RUNE's terms, already decrypted end to end, never a
//! protocol's packets.
//!
//! The provider owns a [`VideoPort`] for its lifetime and the host takes the
//! matching [`VideoLink`] once ([`crate::Provider::take_video_link`]). Both
//! directions are bounded and never wait: a full queue drops the unit and
//! the next one shown must be a key frame.
//!
//! The host says which cameras are on screen ([`VideoRequest::Wants`]), at
//! most [`MAX_CAMERAS`]; the provider asks its service for those only.
//!
//! The account's own shared screen goes the other way: the host encodes it
//! and hands each unit over (`outgoing`, [`OUTGOING`] at most); the
//! provider says when a key frame is needed and what bit rate the network
//! takes ([`Feedback`]). The account's camera, while on in its call, goes
//! the same way through queues of its own (`camera_outgoing`,
//! `camera_feedback`), at most [`CAMERA`] and [`CAMERA_MAX_BITRATE`].
//! The shared screen's sound, when the host shares it, goes as 20 ms Opus
//! frames of 48 kHz stereo (`sound`, [`SOUND`] at most).

use std::sync::Arc;
use std::time::Instant;

use rune_model::{StreamQuality, UserId, VideoKind};
use tokio::sync::{mpsc, Mutex};

/// Units waiting for the decoder; past this the network side drops.
pub const INCOMING: usize = 8;
/// Requests waiting for the provider.
pub const REQUESTS: usize = 32;
/// Cameras decoded and asked for at once; the rest show their avatar.
pub const MAX_CAMERAS: usize = 4;
/// Streams watched at once, each on a connection and a decoder of its own.
pub const MAX_STREAMS: usize = 3;
/// The account's units waiting for the network; past this the encoder
/// drops, and the next one it sends is a key frame.
pub const OUTGOING: usize = 2;
/// What the provider says to the encoder, newest last.
pub const FEEDBACK: usize = 8;
/// The shared screen's sound waiting for the network: 160 ms.
pub const SOUND: usize = 8;
/// The account's camera is captured and sent at most this large and this
/// often, between these bit rates: the official client's camera.
pub const CAMERA: StreamQuality = StreamQuality {
    height: 720,
    fps: 30,
};
pub const CAMERA_MIN_BITRATE: u32 = 150_000;
pub const CAMERA_MAX_BITRATE: u32 = 2_500_000;

/// One access unit of someone's screen or camera: Annex B, 4-byte start codes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoUnit {
    pub user: UserId,
    pub kind: VideoKind,
    /// It holds an IDR picture: decoding can start here.
    pub keyframe: bool,
    /// When it was complete, to drop it once too old to show.
    pub at: Instant,
    pub data: Vec<u8>,
}

/// What the host asks of the provider about someone's video.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoRequest {
    /// The decoder lost its place: send a key frame of `user`'s video.
    KeyFrame { user: UserId, kind: VideoKind },
    /// `user`'s video is on screen (`on`) or no longer. Only cameras are
    /// asked for this way; the screen watched is [`crate::Command::WatchStream`].
    Wants {
        user: UserId,
        kind: VideoKind,
        on: bool,
    },
}

/// What the provider asks of the encoder of the account's screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feedback {
    /// Someone lost the picture, or it is starting: the next unit must be
    /// a key frame.
    KeyFrame,
    /// Encode at this many bits a second from now on.
    Bitrate(u32),
}

/// The host's end.
pub struct VideoLink {
    pub incoming: mpsc::Receiver<VideoUnit>,
    pub requests: mpsc::Sender<VideoRequest>,
    /// The account's own screen, encoded (its `user` is the account).
    pub outgoing: mpsc::Sender<VideoUnit>,
    pub feedback: mpsc::Receiver<Feedback>,
    /// The account's own camera, encoded, and what its encoder must do.
    pub camera_outgoing: mpsc::Sender<VideoUnit>,
    pub camera_feedback: mpsc::Receiver<Feedback>,
    /// The shared screen's sound: Opus frames, 20 ms of 48 kHz stereo each.
    pub sound: mpsc::Sender<Vec<u8>>,
}

/// The provider's end. `requests` and the outgoing queues are locked by the
/// one part reading them.
#[derive(Clone)]
pub struct VideoPort {
    pub incoming: mpsc::Sender<VideoUnit>,
    pub requests: Arc<Mutex<mpsc::Receiver<VideoRequest>>>,
    pub outgoing: Arc<Mutex<mpsc::Receiver<VideoUnit>>>,
    pub feedback: mpsc::Sender<Feedback>,
    pub camera_outgoing: Arc<Mutex<mpsc::Receiver<VideoUnit>>>,
    pub camera_feedback: mpsc::Sender<Feedback>,
    pub sound: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
}

pub fn video_link() -> (VideoPort, VideoLink) {
    let (incoming_tx, incoming_rx) = mpsc::channel(INCOMING);
    let (requests_tx, requests_rx) = mpsc::channel(REQUESTS);
    let (outgoing_tx, outgoing_rx) = mpsc::channel(OUTGOING);
    let (feedback_tx, feedback_rx) = mpsc::channel(FEEDBACK);
    let (camera_tx, camera_rx) = mpsc::channel(OUTGOING);
    let (camera_feedback_tx, camera_feedback_rx) = mpsc::channel(FEEDBACK);
    let (sound_tx, sound_rx) = mpsc::channel(SOUND);
    (
        VideoPort {
            incoming: incoming_tx,
            requests: Arc::new(Mutex::new(requests_rx)),
            outgoing: Arc::new(Mutex::new(outgoing_rx)),
            feedback: feedback_tx,
            camera_outgoing: Arc::new(Mutex::new(camera_rx)),
            camera_feedback: camera_feedback_tx,
            sound: Arc::new(Mutex::new(sound_rx)),
        },
        VideoLink {
            incoming: incoming_rx,
            requests: requests_tx,
            outgoing: outgoing_tx,
            feedback: feedback_rx,
            camera_outgoing: camera_tx,
            camera_feedback: camera_feedback_rx,
            sound: sound_tx,
        },
    )
}
