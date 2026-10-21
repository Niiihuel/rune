//! Video losses repaired with retransmissions (RFC 4588), both ways.
//!
//! Receiving, a [`Reorder`] holds the packets that come after a gap, says
//! which sequence numbers to ask for again (RTCP generic NACK, RFC 4585) and
//! lets the held ones go in order once the missing ones arrive on the
//! sender's RTX SSRC, or once waiting for them is no longer worth it: then
//! the depacketizer sees the gap and a key frame is asked for, as before.
//!
//! Sending, a [`History`] keeps what went out (at most [`HISTORY_AGE`],
//! [`HISTORY_PACKETS`] and [`HISTORY_BYTES`]) so a NACK about the account's
//! own video is answered on its RTX SSRC ([`rtx`]); a packet no longer kept
//! asks the host for a key frame instead.

use std::collections::VecDeque;
use std::time::Duration;

use tokio::time::Instant;

use super::rtp::{Rtp, RTX};
use super::send::Packet;

/// Packets held past a gap; a gap wider than this is given up at once.
pub(crate) const MAX_SPAN: usize = 512;
/// How long a missing packet is waited for, and asked for again meanwhile.
const WAIT: Duration = Duration::from_millis(200);
const ASK_AGAIN: Duration = Duration::from_millis(60);
const MAX_ASKS: u8 = 3;
/// Sequence numbers asked for in one NACK.
pub(crate) const MAX_NACKED: usize = 64;

/// What sending kept: the official client's repair window (docs/DISCORD.md).
pub(crate) const HISTORY_AGE: Duration = Duration::from_secs(1);
pub(crate) const HISTORY_PACKETS: usize = 2048;
pub(crate) const HISTORY_BYTES: usize = 2 * 1024 * 1024;
/// A packet is sent again at most this often, this many times.
const RESEND_GAP: Duration = Duration::from_millis(50);
const MAX_RESENDS: u8 = 2;

enum Slot {
    Held(Rtp),
    /// Since when, how many times asked, and when last.
    Missing(Instant, u8, Option<Instant>),
}

/// One sender's video packets put back in order: see the module.
#[derive(Default)]
pub(crate) struct Reorder {
    /// The sequence number of the first slot.
    next: Option<u16>,
    slots: VecDeque<Slot>,
}

impl Reorder {
    /// A packet arrived, first-hand or restored from a retransmission.
    /// One already let go of, or already held, is dropped.
    pub(crate) fn push(&mut self, packet: Rtp, now: Instant) {
        let next = *self.next.get_or_insert(packet.sequence);
        let ahead = usize::from(packet.sequence.wrapping_sub(next));
        if ahead >= 0x8000 {
            return;
        }
        if ahead >= MAX_SPAN {
            // Too far to wait for: what is held is dropped, and the
            // depacketizer sees the gap.
            self.slots.clear();
            self.next = Some(packet.sequence);
            self.slots.push_back(Slot::Held(packet));
            return;
        }
        while self.slots.len() <= ahead {
            self.slots.push_back(Slot::Missing(now, 0, None));
        }
        if matches!(self.slots[ahead], Slot::Missing(..)) {
            self.slots[ahead] = Slot::Held(packet);
        }
    }

    /// The packets that may go on, in order, and the sequence numbers to ask
    /// for now. A missing packet waited for too long is given up: the
    /// packets after it go on, with the gap.
    pub(crate) fn poll(&mut self, now: Instant) -> (Vec<Rtp>, Vec<u16>) {
        let mut ready = Vec::new();
        loop {
            match self.slots.front() {
                Some(Slot::Held(_)) => {}
                Some(Slot::Missing(since, ..)) if now - *since >= WAIT => {}
                _ => break,
            }
            if let Some(Slot::Held(packet)) = self.slots.pop_front() {
                ready.push(packet);
            }
            self.next = self.next.map(|next| next.wrapping_add(1));
        }
        let Some(next) = self.next else {
            return (ready, Vec::new());
        };
        let mut asked = Vec::new();
        for (at, slot) in self.slots.iter_mut().enumerate() {
            if let Slot::Missing(_, asks, last) = slot {
                let due = last.is_none_or(|last| now - last >= ASK_AGAIN);
                if due && *asks < MAX_ASKS && asked.len() < MAX_NACKED {
                    *asks += 1;
                    *last = Some(now);
                    asked.push(next.wrapping_add(at as u16));
                }
            }
        }
        (ready, asked)
    }

    /// Something is held or missing: it has to be polled again.
    pub(crate) fn pending(&self) -> bool {
        !self.slots.is_empty()
    }
}

/// A packet that went out, and its retransmissions.
struct Sent {
    packet: Packet,
    at: Instant,
    resent: u8,
    last: Option<Instant>,
}

/// What [`History::resend`] found.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Resend {
    /// The packet as it went out.
    Packet(Packet),
    /// Sent again just now, or as often as it will be: nothing to do.
    Wait,
    /// No longer kept: only a key frame repairs the picture.
    Gone,
}

/// The account's video packets that went out lately: see the module.
#[derive(Default)]
pub(crate) struct History {
    sent: VecDeque<Sent>,
    bytes: usize,
}

impl History {
    pub(crate) fn remember(&mut self, packet: Packet, now: Instant) {
        self.expire(now);
        while self.sent.len() >= HISTORY_PACKETS
            || (!self.sent.is_empty() && self.bytes + packet.1.len() > HISTORY_BYTES)
        {
            self.drop_oldest();
        }
        self.bytes += packet.1.len();
        self.sent.push_back(Sent {
            packet,
            at: now,
            resent: 0,
            last: None,
        });
    }

    fn drop_oldest(&mut self) {
        if let Some(sent) = self.sent.pop_front() {
            self.bytes -= sent.packet.1.len();
        }
    }

    fn expire(&mut self, now: Instant) {
        while self
            .sent
            .front()
            .is_some_and(|sent| now - sent.at >= HISTORY_AGE)
        {
            self.drop_oldest();
        }
    }

    /// The packet sent with `sequence`, to send again.
    pub(crate) fn resend(&mut self, sequence: u16, now: Instant) -> Resend {
        self.expire(now);
        let Some(sent) = self
            .sent
            .iter_mut()
            .rev()
            .find(|sent| sent.packet.0[2..4] == sequence.to_be_bytes())
        else {
            return Resend::Gone;
        };
        if sent.resent >= MAX_RESENDS || sent.last.is_some_and(|last| now - last < RESEND_GAP) {
            return Resend::Wait;
        }
        sent.resent += 1;
        sent.last = Some(now);
        Resend::Packet(sent.packet.clone())
    }

    pub(crate) fn clear(&mut self) {
        self.sent.clear();
        self.bytes = 0;
    }
}

/// `packet` again on the RTX SSRC `ssrc` with its own `sequence`: the
/// original sequence number first, then the same payload (RFC 4588).
pub(crate) fn rtx(packet: &Packet, ssrc: u32, sequence: u16) -> Packet {
    let mut header = packet.0;
    header[1] = (header[1] & 0x80) | RTX;
    let original = [header[2], header[3]];
    header[2..4].copy_from_slice(&sequence.to_be_bytes());
    header[8..12].copy_from_slice(&ssrc.to_be_bytes());
    (header, [&original[..], &packet.1].concat())
}

/// The sequence numbers a NACK entry asks for: `first`, and those of the
/// bits set in `mask` after it.
pub(crate) fn lost(first: u16, mask: u16) -> impl Iterator<Item = u16> {
    std::iter::once(first).chain(
        (0..16)
            .filter(move |bit| mask & (1 << bit) != 0)
            .map(move |bit| first.wrapping_add(bit + 1)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::rtp::{video_header, H264};

    fn packet(sequence: u16) -> Rtp {
        Rtp {
            ssrc: 23,
            payload_type: H264,
            marker: false,
            sequence,
            timestamp: 0,
            payload: vec![sequence as u8],
        }
    }

    fn sequences(packets: &[Rtp]) -> Vec<u16> {
        packets.iter().map(|packet| packet.sequence).collect()
    }

    #[test]
    fn a_gap_holds_what_follows_asks_again_and_lets_it_go_once_repaired() {
        let now = Instant::now();
        let mut reorder = Reorder::default();
        reorder.push(packet(u16::MAX), now);
        let (ready, asked) = reorder.poll(now);
        assert_eq!((sequences(&ready), asked), (vec![u16::MAX], vec![]));
        // 0 and 2 are lost across the wrap; 1 and 3 wait for them.
        reorder.push(packet(1), now);
        reorder.push(packet(3), now);
        let (ready, asked) = reorder.poll(now);
        assert!(ready.is_empty());
        assert_eq!(asked, [0, 2]);
        // Not asked again before its time, then again, at most three times.
        assert_eq!(reorder.poll(now + ASK_AGAIN / 2).1, Vec::<u16>::new());
        assert_eq!(reorder.poll(now + ASK_AGAIN).1, [0, 2]);
        assert_eq!(reorder.poll(now + ASK_AGAIN * 2).1, [0, 2]);
        let later = now + ASK_AGAIN * 3 - Duration::from_millis(1);
        assert_eq!(reorder.poll(later).1, Vec::<u16>::new());
        // The retransmission of 0 lets 1 go; a duplicate of 1 is dropped.
        reorder.push(packet(0), now);
        reorder.push(packet(1), now);
        let (ready, _) = reorder.poll(now + Duration::from_millis(1));
        assert_eq!(sequences(&ready), [0, 1]);
        assert!(reorder.pending());
        // 2 never comes: given up, 3 goes on with the gap.
        let (ready, asked) = reorder.poll(now + WAIT);
        assert_eq!((sequences(&ready), asked), (vec![3], vec![]));
        assert!(!reorder.pending());
        // Too late now.
        reorder.push(packet(2), now + WAIT);
        assert!(reorder.poll(now + WAIT).0.is_empty());
    }

    #[test]
    fn a_gap_too_wide_is_given_up_at_once_and_holding_is_bounded() {
        let now = Instant::now();
        let mut reorder = Reorder::default();
        reorder.push(packet(10), now);
        reorder.poll(now);
        reorder.push(packet(12), now);
        reorder.push(packet(10 + MAX_SPAN as u16 + 5), now);
        let (ready, asked) = reorder.poll(now);
        assert_eq!(sequences(&ready), [10 + MAX_SPAN as u16 + 5]);
        assert!(asked.is_empty());
        // A burst of losses asks for at most so many at once.
        let start = 10 + MAX_SPAN as u16 + 6;
        reorder.push(packet(start + 300), now);
        let (_, asked) = reorder.poll(now);
        assert_eq!(asked.len(), MAX_NACKED);
        assert_eq!(asked[0], start);
        assert!(reorder.slots.len() <= MAX_SPAN);
    }

    fn sent(sequence: u16, size: usize) -> Packet {
        (video_header(H264, true, sequence, 9, 33), vec![7; size])
    }

    #[test]
    fn what_went_out_is_kept_for_a_second_within_bounds_and_sent_again_on_rtx() {
        let now = Instant::now();
        let mut history = History::default();
        history.remember(sent(500, 100), now);
        let Resend::Packet(again) = history.resend(500, now) else {
            panic!("kept");
        };
        let repaired = rtx(&again, 34, 7);
        assert_eq!(repaired.0[1], 0x80 | RTX, "the marker stays");
        assert_eq!(&repaired.0[2..4], &7u16.to_be_bytes());
        assert_eq!(&repaired.0[4..8], &9u32.to_be_bytes());
        assert_eq!(&repaired.0[8..12], &34u32.to_be_bytes());
        assert_eq!(&repaired.1[..2], &500u16.to_be_bytes());
        assert_eq!(&repaired.1[2..], &again.1[..]);
        // Not again at once, twice at most, and never after a second.
        assert_eq!(history.resend(500, now), Resend::Wait);
        assert!(matches!(
            history.resend(500, now + RESEND_GAP),
            Resend::Packet(_)
        ));
        assert_eq!(history.resend(500, now + RESEND_GAP * 3), Resend::Wait);
        assert_eq!(history.resend(501, now), Resend::Gone);
        assert_eq!(history.resend(500, now + HISTORY_AGE), Resend::Gone);
        // At most so many packets, and so many bytes.
        for sequence in 0..HISTORY_PACKETS as u16 + 10 {
            history.remember(sent(sequence, 10), now);
        }
        assert_eq!(history.sent.len(), HISTORY_PACKETS);
        assert_eq!(history.resend(9, now), Resend::Gone);
        assert!(matches!(history.resend(10, now), Resend::Packet(_)));
        for sequence in 0..10 {
            history.remember(sent(sequence, HISTORY_BYTES / 4), now);
        }
        assert!(history.bytes <= HISTORY_BYTES);
        assert_eq!(history.sent.len(), 4);
        history.clear();
        assert_eq!((history.sent.len(), history.bytes), (0, 0));
    }

    #[test]
    fn a_nack_entry_names_its_first_and_the_bits_after_it() {
        assert_eq!(lost(10, 0).collect::<Vec<_>>(), [10]);
        assert_eq!(lost(u16::MAX, 0b101).collect::<Vec<_>>(), [u16::MAX, 0, 2]);
        assert_eq!(lost(0, u16::MAX).count(), 17);
    }
}
