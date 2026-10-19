//! A fixed-size reorder buffer for one remote stream, played one 20 ms
//! packet per tick.
//!
//! It holds at most [`SLOTS`] packets of at most [`MAX_PACKET`] bytes. It
//! starts once [`START`] packets wait (40 ms), puts late arrivals back in
//! order, drops duplicates and packets already played, and reports a gap as
//! [`Popped::Lost`] (with the next packet, for FEC) at most [`MAX_CONCEALED`]
//! times in a row. After that the stream counts as stopped and buffers again.

use crate::codec::MAX_PACKET;

/// Packets kept at once: 320 ms of 20 ms packets.
pub const SLOTS: usize = 16;
/// Packets waiting before playback starts.
pub const START: usize = 2;
/// Consecutive gaps concealed before the stream counts as stopped.
pub const MAX_CONCEALED: u8 = 3;

/// What one tick plays.
#[derive(Debug, PartialEq, Eq)]
pub enum Popped {
    Packet(Vec<u8>),
    /// The packet is missing; `next` is the following one if it is here.
    Lost {
        next: Option<Vec<u8>>,
    },
    /// Nothing to play: not started, or stopped.
    Empty,
}

/// Counters for the call's quality.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JitterStats {
    pub received: u64,
    /// Arrived after its turn, or a second time.
    pub late: u64,
    pub lost: u64,
}

pub struct Jitter {
    slots: [Option<(u16, Vec<u8>)>; SLOTS],
    /// The sequence number to play next, once started.
    next: Option<u16>,
    started: bool,
    concealed: u8,
    pub stats: JitterStats,
}

impl Default for Jitter {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            next: None,
            started: false,
            concealed: 0,
            stats: JitterStats::default(),
        }
    }
}

impl Jitter {
    fn waiting(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    /// Take a received packet. Returns whether it was kept.
    pub fn push(&mut self, sequence: u16, packet: Vec<u8>) -> bool {
        if packet.is_empty() || packet.len() > MAX_PACKET {
            return false;
        }
        self.stats.received += 1;
        let next = *self.next.get_or_insert(sequence);
        let ahead = sequence.wrapping_sub(next);
        if ahead >= u16::MAX / 2 {
            let behind = usize::from(next.wrapping_sub(sequence));
            if self.started {
                // Its turn passed.
                self.stats.late += 1;
                return false;
            }
            if behind >= SLOTS {
                self.reset();
            }
            // Still buffering: begin from the earlier packet.
            self.next = Some(sequence);
        } else if usize::from(ahead) >= SLOTS {
            // Too far ahead to wait for the gap: a new talk spurt, or the
            // sender jumped. Start over from here.
            self.reset();
            self.next = Some(sequence);
        }
        let slot = &mut self.slots[usize::from(sequence) % SLOTS];
        if slot.as_ref().is_some_and(|(kept, _)| *kept == sequence) {
            self.stats.late += 1;
            return false;
        }
        *slot = Some((sequence, packet));
        true
    }

    /// The packet for this tick.
    pub fn pop(&mut self) -> Popped {
        let Some(next) = self.next else {
            return Popped::Empty;
        };
        if !self.started {
            if self.waiting() < START {
                return Popped::Empty;
            }
            // Begin at the oldest waiting packet.
            let oldest = (0..SLOTS as u16)
                .map(|offset| next.wrapping_add(offset))
                .find(|sequence| self.has(*sequence))
                .unwrap_or(next);
            self.next = Some(oldest);
            self.started = true;
            return self.pop();
        }
        self.next = Some(next.wrapping_add(1));
        if let Some(packet) = self.take(next) {
            self.concealed = 0;
            return Popped::Packet(packet);
        }
        if self.waiting() == 0 || self.concealed >= MAX_CONCEALED {
            // Nothing coming: the speaker paused. Buffer again before playing.
            self.started = false;
            self.concealed = 0;
            if self.waiting() == 0 {
                self.next = None;
            }
            return Popped::Empty;
        }
        self.concealed += 1;
        self.stats.lost += 1;
        let following = next.wrapping_add(1);
        Popped::Lost {
            next: self.slots[usize::from(following) % SLOTS]
                .as_ref()
                .filter(|(kept, _)| *kept == following)
                .map(|(_, packet)| packet.clone()),
        }
    }

    fn has(&self, sequence: u16) -> bool {
        self.slots[usize::from(sequence) % SLOTS]
            .as_ref()
            .is_some_and(|(kept, _)| *kept == sequence)
    }

    fn take(&mut self, sequence: u16) -> Option<Vec<u8>> {
        let slot = &mut self.slots[usize::from(sequence) % SLOTS];
        if slot.as_ref().is_some_and(|(kept, _)| *kept == sequence) {
            slot.take().map(|(_, packet)| packet)
        } else {
            None
        }
    }

    /// Forget everything waiting.
    pub fn reset(&mut self) {
        self.slots.iter_mut().for_each(|slot| *slot = None);
        self.next = None;
        self.started = false;
        self.concealed = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(sequence: u16) -> Vec<u8> {
        sequence.to_be_bytes().to_vec()
    }

    fn played(jitter: &mut Jitter, ticks: usize) -> Vec<Option<u16>> {
        (0..ticks)
            .map(|_| match jitter.pop() {
                Popped::Packet(bytes) => Some(u16::from_be_bytes([bytes[0], bytes[1]])),
                Popped::Lost { .. } => None,
                Popped::Empty => Some(u16::MAX),
            })
            .collect()
    }

    #[test]
    fn reordered_packets_play_in_order_after_the_start_delay() {
        let mut jitter = Jitter::default();
        assert!(jitter.push(11, packet(11)));
        assert_eq!(jitter.pop(), Popped::Empty, "waits for 40 ms");
        assert!(jitter.push(10, packet(10)));
        assert!(jitter.push(13, packet(13)));
        assert!(jitter.push(12, packet(12)));
        assert_eq!(
            played(&mut jitter, 4),
            [Some(10), Some(11), Some(12), Some(13)]
        );
        assert_eq!(jitter.stats.lost, 0);
    }

    #[test]
    fn a_gap_is_concealed_with_the_next_packet_and_late_or_repeated_ones_drop() {
        let mut jitter = Jitter::default();
        for sequence in [65_534, 65_535, 1, 2] {
            jitter.push(sequence, packet(sequence));
        }
        assert_eq!(played(&mut jitter, 2), [Some(65_534), Some(65_535)]);
        assert_eq!(
            jitter.pop(),
            Popped::Lost {
                next: Some(packet(1))
            },
            "sequence 0 is lost across the wrap"
        );
        assert!(!jitter.push(0, packet(0)), "its turn passed");
        assert!(!jitter.push(2, packet(2)), "already waiting");
        assert_eq!(played(&mut jitter, 2), [Some(1), Some(2)]);
        assert_eq!(jitter.stats.late, 2);
        assert_eq!(jitter.stats.lost, 1);
    }

    #[test]
    fn a_stopped_stream_conceals_a_bounded_number_of_ticks_then_buffers_again() {
        let mut jitter = Jitter::default();
        for sequence in [1, 2, 7] {
            jitter.push(sequence, packet(sequence));
        }
        assert_eq!(played(&mut jitter, 2), [Some(1), Some(2)]);
        assert_eq!(played(&mut jitter, 3), [None, None, None]);
        assert_eq!(jitter.pop(), Popped::Empty, "no endless concealment");
        jitter.push(8, packet(8));
        assert_eq!(played(&mut jitter, 2), [Some(7), Some(8)]);
        assert_eq!(jitter.pop(), Popped::Empty, "a pause is silence");
        jitter.push(40, packet(40));
        jitter.push(41, packet(41));
        assert_eq!(played(&mut jitter, 2), [Some(40), Some(41)]);
    }

    #[test]
    fn memory_stays_bounded_whatever_arrives() {
        let mut jitter = Jitter::default();
        assert!(!jitter.push(1, vec![0; MAX_PACKET + 1]));
        assert!(!jitter.push(1, Vec::new()));
        for sequence in 0..10_000u16 {
            jitter.push(sequence.wrapping_mul(7919), vec![1; MAX_PACKET]);
            assert!(jitter.waiting() <= SLOTS);
        }
        // A jump far ahead restarts there instead of waiting for the gap.
        let mut jitter = Jitter::default();
        jitter.push(1, packet(1));
        jitter.push(2, packet(2));
        jitter.push(500, packet(500));
        jitter.push(501, packet(501));
        assert_eq!(played(&mut jitter, 2), [Some(500), Some(501)]);
    }
}
