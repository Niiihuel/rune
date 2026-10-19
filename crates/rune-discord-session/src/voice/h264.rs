//! H264 over RTP (RFC 6184, packetization mode 1) back into access units:
//! single NAL units, STAP-A and FU-A, in order, each NAL behind a 4-byte
//! start code as libwebrtc's depacketizer writes them, which is what DAVE
//! encrypted on the sender's side.
//!
//! Nothing is reordered or repaired: a gap in the sequence, a picture open
//! too long or past its bounds, or a packet that cannot be read loses the
//! picture, and then nothing comes out until a picture holding an IDR. The
//! caller asks the sender for one (PLI) on every [`Lost`].
//!
//! The account's own units go the other way ([`packetize`]): small NALs
//! together in STAP-A, large ones in FU-A pieces, every payload within
//! [`MAX_PAYLOAD`].

use std::time::{Duration, Instant};

/// A picture's bounds: packets, bytes, and how long it may stay open.
pub(crate) const MAX_PACKETS: usize = 2048;
const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_OPEN: Duration = Duration::from_millis(500);
const START: [u8; 4] = [0, 0, 0, 1];
const IDR: u8 = 5;
const STAP_A: u8 = 24;
const FU_A: u8 = 28;
/// The largest payload sent: with the RTP header, the transport's tag and
/// its nonce counter, a datagram of at most 1200 bytes.
pub(crate) const MAX_PAYLOAD: usize = 1200 - 12 - 16 - 4;

/// Something was lost: a key frame is needed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Lost;

/// One complete access unit.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Unit {
    pub keyframe: bool,
    pub data: Vec<u8>,
}

/// The assembly of one sender's pictures.
pub(crate) struct Depacketizer {
    /// The sequence number expected next.
    next: Option<u16>,
    /// The picture being assembled: its timestamp and since when.
    open: Option<(u32, Instant)>,
    unit: Vec<u8>,
    packets: usize,
    keyframe: bool,
    /// Inside an FU-A.
    fragment: bool,
    /// The open picture lost something: it is dropped at its end.
    broken: bool,
    /// Nothing comes out until a picture with an IDR.
    waiting: bool,
}

impl Default for Depacketizer {
    fn default() -> Self {
        Self {
            next: None,
            open: None,
            unit: Vec::new(),
            packets: 0,
            keyframe: false,
            fragment: false,
            broken: false,
            waiting: true,
        }
    }
}

impl Depacketizer {
    /// After a unit was dropped further on: only a key frame may follow.
    pub(crate) fn wait(&mut self) {
        self.waiting = true;
    }

    /// One packet's payload. A unit when its picture completed; [`Lost`]
    /// when something is missing, or while waiting for a key frame.
    pub(crate) fn push(
        &mut self,
        sequence: u16,
        timestamp: u32,
        marker: bool,
        payload: &[u8],
        now: Instant,
    ) -> Result<Option<Unit>, Lost> {
        let mut lost = false;
        if let Some(next) = self.next {
            let ahead = sequence.wrapping_sub(next);
            if ahead >= 0x8000 {
                // A duplicate, or too late: it was counted lost already.
                return Ok(None);
            }
            lost = ahead != 0;
        }
        self.next = Some(sequence.wrapping_add(1));
        // The open picture never ended, or ran too long.
        if self
            .open
            .is_some_and(|(open, since)| open != timestamp || now - since > MAX_OPEN)
        {
            self.clear();
            lost = true;
        }
        if lost {
            self.waiting = true;
            self.broken = true;
        }
        if self.open.is_none() {
            self.open = Some((timestamp, now));
        }
        if !self.broken && self.add(payload).is_err() {
            self.broken = true;
            self.waiting = true;
            lost = true;
        }
        if !marker {
            return if lost { Err(Lost) } else { Ok(None) };
        }
        let complete = !self.broken && !self.fragment && !self.unit.is_empty();
        let keyframe = self.keyframe;
        let data = std::mem::take(&mut self.unit);
        self.clear();
        if !complete || (self.waiting && !keyframe) {
            return Err(Lost);
        }
        self.waiting = false;
        Ok(Some(Unit { keyframe, data }))
    }

    fn clear(&mut self) {
        self.open = None;
        self.unit.clear();
        self.packets = 0;
        self.keyframe = false;
        self.fragment = false;
        self.broken = false;
    }

    /// Append what `payload` carries, or `Err` when it cannot be read or
    /// passes the bounds.
    fn add(&mut self, payload: &[u8]) -> Result<(), ()> {
        self.packets += 1;
        let &first = payload.first().ok_or(())?;
        match first & 0x1f {
            kind @ 1..=23 => self.nal(kind, payload)?,
            STAP_A => {
                let mut rest = &payload[1..];
                while !rest.is_empty() {
                    let size = usize::from(u16::from_be_bytes([
                        *rest.first().ok_or(())?,
                        *rest.get(1).ok_or(())?,
                    ]));
                    let nal = rest
                        .get(2..2 + size)
                        .filter(|nal| !nal.is_empty())
                        .ok_or(())?;
                    self.nal(nal[0] & 0x1f, nal)?;
                    rest = &rest[2 + size..];
                }
            }
            FU_A => {
                let &header = payload.get(1).ok_or(())?;
                let (start, end, kind) = (header & 0x80 != 0, header & 0x40 != 0, header & 0x1f);
                if start == self.fragment {
                    // A start inside a fragment, or a piece without its start.
                    return Err(());
                }
                if start {
                    self.unit.extend_from_slice(&START);
                    self.unit.push((first & 0xe0) | kind);
                    self.keyframe |= kind == IDR;
                }
                self.unit.extend_from_slice(&payload[2..]);
                self.fragment = !end;
            }
            // STAP-B, MTAP, FU-B and reserved types: not in mode 1.
            _ => return Err(()),
        }
        if self.packets > MAX_PACKETS || self.unit.len() > MAX_BYTES {
            return Err(());
        }
        Ok(())
    }

    fn nal(&mut self, kind: u8, nal: &[u8]) -> Result<(), ()> {
        if self.fragment || kind == 0 {
            return Err(());
        }
        self.unit.extend_from_slice(&START);
        self.unit.extend_from_slice(nal);
        self.keyframe |= kind == IDR;
        Ok(())
    }
}

/// The NALs of an Annex B unit, without their 3- or 4-byte start codes.
pub(crate) fn nals(unit: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut at = 0;
    while at + 3 <= unit.len() {
        if unit[at..at + 3] == [0, 0, 1] {
            let code = if at > 0 && unit[at - 1] == 0 {
                at - 1
            } else {
                at
            };
            starts.push((code, at + 3));
            at += 3;
        } else {
            at += 1;
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(n, &(_, begin))| {
            let end = starts.get(n + 1).map_or(unit.len(), |next| next.0);
            &unit[begin..end]
        })
        .filter(|nal| !nal.is_empty())
        .collect()
}

/// The RTP payloads of one unit, in order; the last one carries the marker.
pub(crate) fn packetize(unit: &[u8]) -> Vec<Vec<u8>> {
    let mut payloads = Vec::new();
    let mut together: Vec<&[u8]> = Vec::new();
    let flush = |together: &mut Vec<&[u8]>, payloads: &mut Vec<Vec<u8>>| {
        match together.as_slice() {
            [] => {}
            [one] => payloads.push(one.to_vec()),
            many => {
                // Any forbidden bit, and the highest importance of them.
                let forbidden = many.iter().fold(0, |bit, nal| bit | (nal[0] & 0x80));
                let importance = many.iter().map(|nal| nal[0] & 0x60).max().unwrap_or(0);
                let mut payload = vec![forbidden | importance | STAP_A];
                for nal in many {
                    payload.extend((nal.len() as u16).to_be_bytes());
                    payload.extend_from_slice(nal);
                }
                payloads.push(payload);
            }
        }
        together.clear();
    };
    for nal in nals(unit) {
        if nal.len() > MAX_PAYLOAD {
            flush(&mut together, &mut payloads);
            let pieces: Vec<&[u8]> = nal[1..].chunks(MAX_PAYLOAD - 2).collect();
            for (at, piece) in pieces.iter().enumerate() {
                let mut header = nal[0] & 0x1f;
                if at == 0 {
                    header |= 0x80;
                }
                if at == pieces.len() - 1 {
                    header |= 0x40;
                }
                let mut payload = vec![(nal[0] & 0xe0) | FU_A, header];
                payload.extend_from_slice(piece);
                payloads.push(payload);
            }
            continue;
        }
        let size: usize = 1 + together.iter().map(|nal| 2 + nal.len()).sum::<usize>();
        if !together.is_empty() && size + 2 + nal.len() > MAX_PAYLOAD {
            flush(&mut together, &mut payloads);
        }
        together.push(nal);
    }
    flush(&mut together, &mut payloads);
    payloads
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPS: [u8; 4] = [0x67, 0x42, 0xe0, 0x1f];
    const PPS: [u8; 3] = [0x68, 0xce, 0x3c];

    fn stap(nals: &[&[u8]]) -> Vec<u8> {
        let mut payload = vec![0x18];
        for nal in nals {
            payload.extend((nal.len() as u16).to_be_bytes());
            payload.extend(*nal);
        }
        payload
    }

    /// FU-A pieces of `nal`, `size` bytes of it each.
    fn fu(nal: &[u8], size: usize) -> Vec<Vec<u8>> {
        let chunks: Vec<&[u8]> = nal[1..].chunks(size).collect();
        chunks
            .iter()
            .enumerate()
            .map(|(at, chunk)| {
                let mut header = nal[0] & 0x1f;
                if at == 0 {
                    header |= 0x80;
                }
                if at == chunks.len() - 1 {
                    header |= 0x40;
                }
                let mut payload = vec![(nal[0] & 0xe0) | FU_A, header];
                payload.extend(*chunk);
                payload
            })
            .collect()
    }

    fn annex_b(nals: &[&[u8]]) -> Vec<u8> {
        nals.iter()
            .flat_map(|nal| [&START[..], nal].concat())
            .collect()
    }

    #[test]
    fn stap_a_and_fu_a_rebuild_the_key_frame_then_single_nals_follow() {
        let now = Instant::now();
        let idr: Vec<u8> = [0x65]
            .into_iter()
            .chain((0..250).map(|n| n as u8))
            .collect();
        let mut packets = vec![stap(&[&SPS, &PPS])];
        packets.extend(fu(&idr, 100));
        let mut depacketizer = Depacketizer::default();
        let last = packets.len() - 1;
        let mut out = None;
        for (at, payload) in packets.iter().enumerate() {
            out = depacketizer
                .push(10 + at as u16, 900, at == last, payload, now)
                .unwrap();
        }
        assert_eq!(
            out,
            Some(Unit {
                keyframe: true,
                data: annex_b(&[&SPS, &PPS, &idr])
            })
        );
        let delta = [0x41, 0x9a, 0x02];
        assert_eq!(
            depacketizer.push(14, 3_900, true, &delta, now),
            Ok(Some(Unit {
                keyframe: false,
                data: annex_b(&[&delta])
            }))
        );
        // A duplicate is ignored; the sequence wraps.
        assert_eq!(depacketizer.push(14, 3_900, true, &delta, now), Ok(None));
    }

    #[test]
    fn a_lost_fragment_drops_the_picture_and_waits_for_a_key_frame() {
        let now = Instant::now();
        let idr: Vec<u8> = [0x65].into_iter().chain([7; 300]).collect();
        let pieces = fu(&idr, 100);
        let mut depacketizer = Depacketizer::default();
        // Nothing before a key frame: a delta is refused and asks for one.
        assert_eq!(
            depacketizer.push(u16::MAX, 0, true, &[0x41, 1], now),
            Err(Lost)
        );
        assert_eq!(depacketizer.push(0, 90, false, &pieces[0], now), Ok(None));
        // The middle piece is lost.
        assert_eq!(depacketizer.push(2, 90, true, &pieces[2], now), Err(Lost));
        // Deltas after it are not shown until a key frame.
        assert_eq!(depacketizer.push(3, 180, true, &[0x41, 2], now), Err(Lost));
        let mut out = Ok(None);
        for (at, piece) in pieces.iter().enumerate() {
            out = depacketizer.push(4 + at as u16, 270, at == pieces.len() - 1, piece, now);
        }
        assert!(matches!(out, Ok(Some(Unit { keyframe: true, .. }))));
        // A picture that never ends, or ends past its time, is lost.
        assert_eq!(depacketizer.push(7, 360, false, &[0x41, 3], now), Ok(None));
        assert_eq!(
            depacketizer.push(8, 360, true, &[0x41, 4], now + MAX_OPEN * 2),
            Err(Lost)
        );
        depacketizer.wait();
        assert_eq!(depacketizer.push(9, 450, true, &[0x41, 5], now), Err(Lost));
    }

    /// `unit` packetized and assembled again: what comes out, and the payloads.
    fn round_trip(unit: &[u8]) -> (Option<Unit>, Vec<Vec<u8>>) {
        let payloads = packetize(unit);
        let mut depacketizer = Depacketizer::default();
        let now = Instant::now();
        let mut out = None;
        for (at, payload) in payloads.iter().enumerate() {
            assert!(payload.len() <= MAX_PAYLOAD, "{}", payload.len());
            let last = at == payloads.len() - 1;
            out = depacketizer
                .push(at as u16, 3_000, last, payload, now)
                .unwrap();
            assert_eq!(out.is_some(), last);
        }
        (out, payloads)
    }

    #[test]
    fn units_go_out_in_stap_a_and_fu_a_and_come_back_whole() {
        let sei = [0x06, 5, 1, 0x80];
        let idr: Vec<u8> = [0x65]
            .into_iter()
            .chain((0..3_000).map(|n| n as u8 | 1))
            .collect();
        let unit = annex_b(&[&SPS, &PPS, &sei, &idr]);
        let (out, payloads) = round_trip(&unit);
        assert_eq!(
            out,
            Some(Unit {
                keyframe: true,
                data: unit
            })
        );
        // The parameter sets and the SEI together; the picture in three pieces.
        assert_eq!(payloads[0][0], 0x78, "STAP-A of NRI 3");
        assert_eq!(payloads.len(), 1 + 3_000usize.div_ceil(MAX_PAYLOAD - 2));
        assert!(payloads[1..]
            .iter()
            .all(|payload| payload[0] & 0x1f == FU_A));
        assert_eq!(payloads[1][1] & 0xc0, 0x80, "start");
        assert_eq!(payloads.last().unwrap()[1] & 0xc0, 0x40, "end");
        // 3-byte start codes are read as well; what comes back has 4.
        let short = [&[0, 0, 1][..], &[0x41, 7, 7]].concat();
        let delta = annex_b(&[&[0x41, 7, 7]]);
        assert_eq!(packetize(&short), packetize(&delta));
    }

    #[test]
    fn the_largest_single_nal_fits_one_packet_and_one_more_byte_does_not() {
        let nal =
            |size: usize| -> Vec<u8> { [0x65].into_iter().chain([9].repeat(size - 1)).collect() };
        let (out, payloads) = round_trip(&annex_b(&[&nal(MAX_PAYLOAD)]));
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].len(), MAX_PAYLOAD);
        assert!(out.unwrap().keyframe);
        let (out, payloads) = round_trip(&annex_b(&[&nal(MAX_PAYLOAD + 1)]));
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0].len(), MAX_PAYLOAD);
        assert_eq!(out.unwrap().data, annex_b(&[&nal(MAX_PAYLOAD + 1)]));
        // Many small NALs fill one STAP-A and spill into the next.
        let small = [0x65].repeat(100);
        let many: Vec<&[u8]> = std::iter::repeat_n(&small[..], 20).collect();
        let (out, payloads) = round_trip(&annex_b(&many));
        assert_eq!(payloads.len(), 2);
        assert!(payloads.iter().all(|payload| payload[0] & 0x1f == STAP_A));
        assert_eq!(out.unwrap().data, annex_b(&many));
    }

    #[test]
    fn malformed_and_oversized_pictures_are_lost() {
        let now = Instant::now();
        for payload in [
            vec![],
            vec![0x18, 0, 9, 0x67], // STAP-A shorter than it says
            vec![0x1c],             // FU-A without its header
            vec![0x7c, 0x05, 1],    // FU-A piece without its start
            vec![0x19, 0, 1, 0x67], // STAP-B
            vec![0x00, 1],          // type 0
        ] {
            let mut depacketizer = Depacketizer::default();
            assert_eq!(
                depacketizer.push(0, 0, true, &payload, now),
                Err(Lost),
                "{payload:?}"
            );
        }
        let mut depacketizer = Depacketizer::default();
        let piece = [0x65; 1200];
        let mut result = Ok(None);
        for sequence in 0..=(MAX_BYTES / 1200) as u16 + 1 {
            result = depacketizer.push(sequence, 0, false, &piece, now);
            if result.is_err() {
                break;
            }
        }
        assert_eq!(result, Err(Lost), "past 2 MiB");
    }
}
