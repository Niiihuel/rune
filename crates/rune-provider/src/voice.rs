//! The audio path between a provider's voice transport and the host's call
//! media: Opus packets in RUNE's terms, never a protocol's.
//!
//! The provider owns a [`VoicePort`] for its lifetime and the host takes the
//! matching [`VoiceLink`] once. Both directions are bounded and never wait:
//! a full queue drops the packet, as the network would.

use std::sync::Arc;

use rune_model::UserId;
use tokio::sync::{mpsc, Mutex};

/// Received packets waiting for the host: a few ticks of a full room.
pub const INCOMING: usize = 256;
/// Packets waiting to go out: 160 ms of 20 ms frames.
pub const OUTGOING: usize = 8;

/// One Opus packet someone in the account's call sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoicePacket {
    pub user: UserId,
    pub sequence: u16,
    pub opus: Vec<u8>,
    /// The sound of a screen they stream, apart from their voice.
    pub stream: bool,
}

/// The host's end: 20 ms Opus frames of the microphone go out, already
/// gated; what the others say comes in.
pub struct VoiceLink {
    pub incoming: mpsc::Receiver<VoicePacket>,
    pub outgoing: mpsc::Sender<Vec<u8>>,
}

/// The provider's end. `outgoing` is locked by the one call that sends.
#[derive(Clone)]
pub struct VoicePort {
    pub incoming: mpsc::Sender<VoicePacket>,
    pub outgoing: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
}

pub fn voice_link() -> (VoicePort, VoiceLink) {
    let (incoming_tx, incoming_rx) = mpsc::channel(INCOMING);
    let (outgoing_tx, outgoing_rx) = mpsc::channel(OUTGOING);
    (
        VoicePort {
            incoming: incoming_tx,
            outgoing: Arc::new(Mutex::new(outgoing_rx)),
        },
        VoiceLink {
            incoming: incoming_rx,
            outgoing: outgoing_tx,
        },
    )
}
