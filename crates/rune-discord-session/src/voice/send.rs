//! The account's own screen on its stream connection, or its camera on the
//! call's: the bit rate the network takes, and the pace its packets go out at.
//!
//! [`Rate`] starts at the quality's most, as Serein's, and moves once a
//! second between its floor ([`MIN_BITRATE`] for a screen) and that most,
//! by the loss the receivers report, as WebRTC's loss-based estimate does:
//! over 10 % it comes down by half the fraction lost, between 2 and 10 % it
//! holds, and under 2 % it goes up 8 %, once two seconds passed without
//! loss over 2 %; a REMB that is lower caps it, and with no report at all
//! it stays. It used to start at 600 kbit/s and come down 20 % from 5 %
//! loss: OpenH264 then skips most frames of a moving 720p30 screen (2,5
//! of 30 a second at 600 kbit/s in `encode_bench`), which reads as a
//! stream that is not fluid.
//! [`Pacer`] holds one unit's packets and lets them out every [`PACE`],
//! within a byte budget that refills at 2.5 times the bit rate; the
//! retransmissions NACKs ask for go first, within the same budget, at most
//! [`MAX_REPAIRS`] waiting.

use std::collections::VecDeque;
use std::time::Duration;

use tokio::time::Instant;

use super::rtp::Report;

/// The least bit rate asked of a screen's encoder, as Serein's.
pub(crate) const MIN_BITRATE: u32 = 250_000;
/// How often the bit rate moves.
pub(crate) const RATE_STEP: Duration = Duration::from_secs(1);
/// How often a batch of packets goes out.
pub(crate) const PACE: Duration = Duration::from_millis(2);
/// The budget refills this much faster than the bit rate, so a key frame
/// is not spread over a second, and holds at most this many bytes.
const PACE_FACTOR: f64 = 2.5;
const BURST: f64 = 4.0 * 1200.0;
/// Loss over this many 256ths (10 %) cuts the bit rate…
const LOSSY: u8 = 26;
/// …and up to this many (2 %) lets it grow.
const CLEAN: u8 = 5;
/// Steps without loss over [`CLEAN`] before it grows again.
const CALM_STEPS: u32 = 2;
/// Retransmissions waiting to go out; more are not sent.
pub(crate) const MAX_REPAIRS: usize = 128;

/// The bit rate, from what the receivers report.
pub(crate) struct Rate {
    target: u32,
    min: u32,
    max: u32,
    /// The worst loss and the lowest estimate since the last step.
    loss: Option<u8>,
    estimate: Option<u32>,
    /// Steps since the last with loss over [`CLEAN`].
    calm: u32,
    /// What the diagnostic line says since it last did.
    pub(crate) seen: Seen,
}

/// What the receivers said and what the rate did, between two diagnostic
/// lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Seen {
    /// Loss reports, their sum and the worst, in 256ths.
    pub reports: u32,
    pub loss_sum: u32,
    pub worst: u8,
    /// The lowest REMB.
    pub estimate: Option<u32>,
    /// Times the rate came down.
    pub cuts: u32,
}

impl Rate {
    pub(crate) fn new(min: u32, max: u32) -> Self {
        let max = max.max(min);
        Self {
            target: max,
            min,
            max,
            loss: None,
            estimate: None,
            calm: CALM_STEPS,
            seen: Seen::default(),
        }
    }

    pub(crate) fn target(&self) -> u32 {
        self.target
    }

    pub(crate) fn max(&self) -> u32 {
        self.max
    }

    pub(crate) fn report(&mut self, report: Report) {
        match report {
            Report::Loss(lost) => {
                self.loss = Some(self.loss.map_or(lost, |worst| worst.max(lost)));
                self.seen.reports += 1;
                self.seen.loss_sum += u32::from(lost);
                self.seen.worst = self.seen.worst.max(lost);
            }
            Report::Estimate(bitrate) => {
                self.estimate = Some(self.estimate.map_or(bitrate, |low| low.min(bitrate)));
                self.seen.estimate =
                    Some(self.seen.estimate.map_or(bitrate, |low| low.min(bitrate)));
            }
            Report::KeyFrame | Report::Nack { .. } => {}
        }
    }

    /// Once a [`RATE_STEP`]: the new bit rate, when it moved.
    pub(crate) fn step(&mut self) -> Option<u32> {
        let (loss, estimate) = (self.loss.take(), self.estimate.take());
        if loss.is_none() && estimate.is_none() {
            return None;
        }
        let before = self.target;
        let mut target = f64::from(before);
        match loss {
            Some(lost) if lost > LOSSY => {
                target *= 1.0 - f64::from(lost) / 256.0 / 2.0;
                self.calm = 0;
            }
            Some(lost) if lost > CLEAN => self.calm = 0,
            _ => {
                self.calm += 1;
                if self.calm > CALM_STEPS && estimate.is_none_or(|estimate| estimate >= before) {
                    target *= 1.08;
                }
            }
        }
        if let Some(estimate) = estimate {
            target = target.min(f64::from(estimate));
        }
        self.target = (target as u32).clamp(self.min, self.max);
        if self.target < before {
            self.seen.cuts += 1;
        }
        (self.target != before).then_some(self.target)
    }
}

/// How often the sending connection says what went out
/// (`rune: voice share: network …`).
pub(crate) const REPORT_EVERY: Duration = Duration::from_secs(5);

/// What went out between two diagnostic lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Traffic {
    /// Units taken from the host and packetized, the key frames among
    /// them, and those dropped while waiting for a key frame.
    pub units: u32,
    pub keyframes: u32,
    pub dropped: u32,
    /// Media packets and their bytes; retransmissions apart.
    pub packets: u32,
    pub bytes: u64,
    pub repairs: u32,
    pub repair_bytes: u64,
    /// PLIs and FIRs: key frames the receivers asked for.
    pub asked: u32,
}

impl Traffic {
    /// The line for `elapsed` of it, with what the receivers said and the
    /// rate now. No address, key or id.
    pub(crate) fn line(&self, seen: &Seen, target: u32, elapsed: Duration) -> String {
        let seconds = elapsed.as_secs_f64().max(0.001);
        let per_second = |count: u32| f64::from(count) / seconds;
        let kbits = |bytes: u64| bytes as f64 * 8.0 / seconds / 1000.0;
        let percent = |lost: f64| lost / 256.0 * 100.0;
        let loss = if seen.reports == 0 {
            "no loss reports".to_owned()
        } else {
            format!(
                "loss {:.1} % average, {:.1} % worst ({} reports)",
                percent(f64::from(seen.loss_sum) / f64::from(seen.reports)),
                percent(f64::from(seen.worst)),
                seen.reports
            )
        };
        let estimate = seen.estimate.map_or("no REMB".to_owned(), |bitrate| {
            format!("REMB {} kbit/s", bitrate / 1000)
        });
        format!(
            "network: {:.1} units/s ({} key, {} dropped waiting for one), {:.0} packets/s, \
             {:.0} kbit/s (+{:.0} kbit/s in {} retransmissions), {loss}, {estimate}, \
             target {} kbit/s, {} cuts, {} key frames asked",
            per_second(self.units),
            self.keyframes,
            self.dropped,
            per_second(self.packets),
            kbits(self.bytes),
            kbits(self.repair_bytes),
            self.repairs,
            target / 1000,
            seen.cuts,
            self.asked,
        )
    }
}

/// One RTP packet waiting: its header and payload, sealed when it goes.
pub(crate) type Packet = ([u8; 12], Vec<u8>);

/// The packets of the unit on its way, and the retransmissions asked for.
pub(crate) struct Pacer {
    packets: VecDeque<Packet>,
    repairs: VecDeque<Packet>,
    credit: f64,
    last: Instant,
}

impl Pacer {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            packets: VecDeque::new(),
            repairs: VecDeque::new(),
            credit: BURST,
            last: now,
        }
    }

    /// No unit waits: the next one may be taken.
    pub(crate) fn idle(&self) -> bool {
        self.packets.is_empty()
    }

    /// Something waits to go out.
    pub(crate) fn pending(&self) -> bool {
        !self.packets.is_empty() || !self.repairs.is_empty()
    }

    pub(crate) fn push(&mut self, packets: impl IntoIterator<Item = Packet>) {
        self.packets.extend(packets);
    }

    /// A retransmission, ahead of the unit on its way; dropped when too
    /// many wait already.
    pub(crate) fn repair(&mut self, packet: Packet) {
        if self.repairs.len() < MAX_REPAIRS {
            self.repairs.push_back(packet);
        }
    }

    /// The packets that may go out at `now` at `bitrate`: while there is
    /// budget left, the last one taking it into debt.
    pub(crate) fn batch(&mut self, now: Instant, bitrate: u32) -> Vec<Packet> {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.credit = (self.credit + elapsed * f64::from(bitrate) * PACE_FACTOR / 8.0).min(BURST);
        let mut batch = Vec::new();
        while self.credit > 0.0 {
            let Some(packet) = self
                .repairs
                .pop_front()
                .or_else(|| self.packets.pop_front())
            else {
                break;
            };
            self.credit -= (12 + packet.1.len()) as f64;
            batch.push(packet);
        }
        batch
    }

    pub(crate) fn clear(&mut self) {
        self.packets.clear();
        self.repairs.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bit_rate_moves_once_a_second_by_the_rules_and_stays_in_bounds() {
        let mut rate = Rate::new(MIN_BITRATE, 3_500_000);
        assert_eq!(rate.target(), 3_500_000, "at the most, as Serein");
        // No report: it stays.
        assert_eq!(rate.step(), None);
        // Some loss, up to 10 %: it holds; a little more is not a cut.
        rate.report(Report::Loss(13));
        assert_eq!(rate.step(), None);
        rate.report(Report::Loss(26));
        assert_eq!(rate.step(), None);
        // Over 10 %: down by half the fraction lost, the worst of the second.
        rate.report(Report::Loss(2));
        rate.report(Report::Loss(64));
        assert_eq!(rate.step(), Some(3_062_500));
        // Clean again: up 8 % only after two calm seconds.
        for _ in 0..2 {
            rate.report(Report::Loss(0));
            assert_eq!(rate.step(), None);
        }
        rate.report(Report::Loss(5));
        assert_eq!(rate.step(), Some(3_307_500));
        // A lower estimate holds it there; a higher one lets it rise.
        rate.report(Report::Estimate(400_000));
        assert_eq!(rate.step(), Some(400_000));
        rate.report(Report::Estimate(10_000_000));
        assert_eq!(rate.step(), Some(432_000));
        assert_eq!(rate.seen.cuts, 2);
        assert_eq!((rate.seen.reports, rate.seen.worst), (7, 64));
        assert_eq!(rate.seen.estimate, Some(400_000));
        // Never under the floor, never over the quality's most.
        for _ in 0..30 {
            rate.report(Report::Loss(255));
            rate.step();
        }
        assert_eq!(rate.target(), MIN_BITRATE);
        assert_eq!(rate.step(), None);
        let mut small = Rate::new(MIN_BITRATE, 700_000);
        for _ in 0..20 {
            small.report(Report::Loss(0));
            small.step();
        }
        assert_eq!(small.target(), 700_000);
        assert_eq!(Rate::new(MIN_BITRATE, 100_000).target(), MIN_BITRATE);
        // A camera's floor is its own.
        let mut camera = Rate::new(150_000, 2_500_000);
        for _ in 0..30 {
            camera.report(Report::Loss(255));
            camera.step();
        }
        assert_eq!((camera.target(), camera.max()), (150_000, 2_500_000));
    }

    #[test]
    fn the_diagnostic_line_says_rates_per_second_and_loss_in_percent() {
        let traffic = Traffic {
            units: 150,
            keyframes: 1,
            dropped: 2,
            packets: 4_000,
            bytes: 1_500_000,
            repairs: 3,
            repair_bytes: 3_750,
            asked: 1,
        };
        let seen = Seen {
            reports: 4,
            loss_sum: 8,
            worst: 5,
            estimate: Some(3_100_000),
            cuts: 0,
        };
        assert_eq!(
            traffic.line(&seen, 2_500_000, Duration::from_secs(5)),
            "network: 30.0 units/s (1 key, 2 dropped waiting for one), 800 packets/s, \
             2400 kbit/s (+6 kbit/s in 3 retransmissions), loss 0.8 % average, \
             2.0 % worst (4 reports), REMB 3100 kbit/s, target 2500 kbit/s, 0 cuts, \
             1 key frames asked"
        );
        let quiet = Traffic::default().line(&Seen::default(), 3_500_000, Duration::from_secs(5));
        assert!(quiet.contains("no loss reports") && quiet.contains("no REMB"));
    }

    #[test]
    fn packets_go_out_in_batches_within_the_budget() {
        let start = Instant::now();
        let mut pacer = Pacer::new(start);
        assert!(pacer.idle());
        pacer.push((0..20).map(|_| ([0; 12], vec![0; 1188])));
        // The first burst: four packets of 1200 bytes.
        assert_eq!(pacer.batch(start, 1_000_000).len(), 4);
        // 2 ms at 1 Mbit/s ×2.5 is 625 bytes: a packet now and then, on credit.
        assert_eq!(pacer.batch(start + PACE, 1_000_000).len(), 1);
        assert_eq!(pacer.batch(start + PACE * 2, 1_000_000).len(), 1);
        assert_eq!(pacer.batch(start + PACE * 3, 1_000_000).len(), 0);
        // Idle a long while, the budget never passes the burst.
        assert_eq!(pacer.batch(start + PACE * 1000, 1_000_000).len(), 4);
        assert!(!pacer.idle());
        pacer.clear();
        assert!(pacer.idle() && !pacer.pending());
        // Retransmissions go first, and only so many wait.
        pacer.push([([1; 12], vec![0; 100])]);
        for _ in 0..MAX_REPAIRS + 5 {
            pacer.repair(([2; 12], vec![0; 100]));
        }
        assert_eq!(pacer.repairs.len(), MAX_REPAIRS);
        let batch = pacer.batch(start + PACE * 2000, 1_000_000);
        assert!(batch.iter().all(|(header, _)| header[0] == 2));
        pacer.clear();
        pacer.repair(([2; 12], vec![0; 100]));
        assert!(pacer.idle() && pacer.pending());
    }
}
