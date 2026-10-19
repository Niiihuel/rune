//! RTP framing and the transport encryption Discord requires,
//! `aead_xchacha20_poly1305_rtpsize`: the RTP header (with CSRCs and the
//! extension preamble) stays clear and authenticated; the extension body and
//! payload are sealed; a 32-bit big-endian counter, the start of the 24-byte
//! nonce, trails the packet. RTCP is sealed the same way with its first 8
//! bytes (header and sender SSRC) clear: how Serein sends it; the bundles
//! of the official client do not show it (docs/REA.md, «Sin confirmar»).
//! RTCP from the server is opened the same way only by a connection that
//! sends video, for the [`Report`]s about it.

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use zeroize::Zeroizing;

pub(crate) const MODE: &str = "aead_xchacha20_poly1305_rtpsize";
/// Received datagrams above this are not voice.
pub(crate) const MAX_DATAGRAM: usize = 4096;
/// Opus, as Discord's voice servers number it.
pub(crate) const OPUS: u8 = 120;
/// H264 and its retransmissions, as RUNE offers them in Select Protocol.
pub(crate) const H264: u8 = 101;
pub(crate) const RTX: u8 = 102;
const HEADER: usize = 12;
const TAG: usize = 16;
const NONCE: usize = 4;

/// One authenticated packet; `payload` is still end-to-end encrypted.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Rtp {
    pub ssrc: u32,
    pub payload_type: u8,
    /// The last packet of a video picture.
    pub marker: bool,
    pub sequence: u16,
    pub timestamp: u32,
    pub payload: Vec<u8>,
}

pub(crate) fn header(sequence: u16, timestamp: u32, ssrc: u32) -> [u8; HEADER] {
    video_header(OPUS, false, sequence, timestamp, ssrc)
}

/// The header of a packet of `payload_type`; `marker` ends a picture.
pub(crate) fn video_header(
    payload_type: u8,
    marker: bool,
    sequence: u16,
    timestamp: u32,
    ssrc: u32,
) -> [u8; HEADER] {
    let mut header = [0; HEADER];
    header[0] = 0x80;
    header[1] = payload_type | if marker { 0x80 } else { 0 };
    header[2..4].copy_from_slice(&sequence.to_be_bytes());
    header[4..8].copy_from_slice(&timestamp.to_be_bytes());
    header[8..12].copy_from_slice(&ssrc.to_be_bytes());
    header
}

/// The session's transport key and the counter of packets sealed with it.
pub(crate) struct Transport {
    cipher: XChaCha20Poly1305,
    counter: u32,
}

impl Transport {
    /// The cipher keeps its own copy; the caller's key is erased by its owner.
    pub(crate) fn new(key: &Zeroizing<[u8; 32]>) -> Self {
        Self {
            cipher: XChaCha20Poly1305::new(key.as_ref().into()),
            counter: 0,
        }
    }

    fn nonce(counter: [u8; NONCE]) -> XNonce {
        let mut nonce = [0; 24];
        nonce[..NONCE].copy_from_slice(&counter);
        nonce.into()
    }

    /// Seal one packet. A nonce is never reused: after 2³² packets (two
    /// years of audio) the call must be joined again.
    pub(crate) fn seal(&mut self, header: &[u8], payload: &[u8]) -> Option<Vec<u8>> {
        self.counter = self.counter.checked_add(1)?;
        let counter = self.counter.to_be_bytes();
        let sealed = self
            .cipher
            .encrypt(
                &Self::nonce(counter),
                Payload {
                    msg: payload,
                    aad: header,
                },
            )
            .ok()?;
        let mut packet = Vec::with_capacity(header.len() + sealed.len() + NONCE);
        packet.extend_from_slice(header);
        packet.extend_from_slice(&sealed);
        packet.extend_from_slice(&counter);
        Some(packet)
    }

    /// Authenticate and open an Opus, H264 or RTX packet; anything else,
    /// RTCP included, forged or malformed, is `None`.
    pub(crate) fn open(&self, packet: &[u8]) -> Option<Rtp> {
        if packet.len() > MAX_DATAGRAM
            || packet.len() < HEADER + TAG + NONCE
            || packet[0] >> 6 != 2
            || ![OPUS, H264, RTX].contains(&(packet[1] & 0x7f))
        {
            return None;
        }
        let csrc_end = HEADER + usize::from(packet[0] & 0x0f) * 4;
        let extended = packet[0] & 0x10 != 0;
        let clear = csrc_end + if extended { 4 } else { 0 };
        if clear + TAG + NONCE > packet.len() {
            return None;
        }
        let (body, counter) = packet.split_at(packet.len() - NONCE);
        let mut plain = self
            .cipher
            .decrypt(
                &Self::nonce(counter.try_into().ok()?),
                Payload {
                    msg: &body[clear..],
                    aad: &body[..clear],
                },
            )
            .ok()?;
        if extended {
            let words = u16::from_be_bytes([packet[csrc_end + 2], packet[csrc_end + 3]]);
            let length = usize::from(words) * 4;
            if length > plain.len() {
                return None;
            }
            plain.drain(..length);
        }
        if packet[0] & 0x20 != 0 {
            let padding = usize::from(*plain.last()?);
            if padding == 0 || padding > plain.len() {
                return None;
            }
            plain.truncate(plain.len() - padding);
        }
        Some(Rtp {
            payload_type: packet[1] & 0x7f,
            marker: packet[1] & 0x80 != 0,
            sequence: u16::from_be_bytes([packet[2], packet[3]]),
            timestamp: u32::from_be_bytes(packet[4..8].try_into().ok()?),
            ssrc: u32::from_be_bytes(packet[8..12].try_into().ok()?),
            payload: plain,
        })
    }
}

// Voice protocol (voz-protocolo).
/// The element that marks a shared screen's sound, `(id, value)`: what
/// Serein sends. Neither is in the official client's strings: not verified
/// (docs/REA.md).
pub(crate) const SCREEN_SOUND: (u8, u8) = (9, 4);

/// RFC 8285 §4.2, one-byte form: the extension's preamble (profile `0xBEDE`
/// and its length in 32-bit words) and its single element `id` (1 to 14)
/// holding `value` (1 to 16 bytes), zero-padded to a whole word.
fn one_byte_extension(id: u8, value: &[u8]) -> Option<([u8; 4], Vec<u8>)> {
    if !(1..=14).contains(&id) || !(1..=16).contains(&value.len()) {
        return None;
    }
    let mut element = Vec::with_capacity(20);
    element.push(id << 4 | (value.len() - 1) as u8);
    element.extend_from_slice(value);
    element.resize(element.len().div_ceil(4) * 4, 0);
    let [high, low] = ((element.len() / 4) as u16).to_be_bytes();
    Some(([0xbe, 0xde, high, low], element))
}

impl Transport {
    /// Seal one packet carrying a header extension with the element `id`
    /// of `value`: the X bit and the preamble go clear and authenticated
    /// with the header, the element sealed ahead of the payload, as
    /// [`Self::open`] strips it.
    pub(crate) fn seal_extended(
        &mut self,
        header: &[u8; HEADER],
        (id, value): (u8, &[u8]),
        payload: &[u8],
    ) -> Option<Vec<u8>> {
        let (preamble, mut sealed) = one_byte_extension(id, value)?;
        let mut clear = [0; HEADER + 4];
        clear[..HEADER].copy_from_slice(header);
        clear[0] |= 0x10;
        clear[HEADER..].copy_from_slice(&preamble);
        sealed.extend_from_slice(payload);
        self.seal(&clear, &sealed)
    }
}

impl Transport {
    /// Authenticate and open a compound RTCP packet: its first 8 bytes
    /// clear, the rest sealed. Anything else is `None`.
    pub(crate) fn open_rtcp(&self, packet: &[u8]) -> Option<Vec<u8>> {
        if packet.len() > MAX_DATAGRAM
            || packet.len() < 8 + TAG + NONCE
            || packet[0] >> 6 != 2
            || !(200..=206).contains(&packet[1])
        {
            return None;
        }
        let (body, counter) = packet.split_at(packet.len() - NONCE);
        let plain = self
            .cipher
            .decrypt(
                &Self::nonce(counter.try_into().ok()?),
                Payload {
                    msg: &body[8..],
                    aad: &body[..8],
                },
            )
            .ok()?;
        Some([&body[..8], &plain].concat())
    }
}

/// What the server's RTCP says about the video sent from one SSRC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Report {
    /// PLI or FIR: someone needs a key frame.
    KeyFrame,
    /// A receiver report's fraction lost, in 256ths.
    Loss(u8),
    /// REMB: the bit rate the receivers can take.
    Estimate(u32),
    /// A generic NACK entry: `first` and the sequence numbers of the bits
    /// set in `mask` after it are lost ([`super::nack::lost`]).
    Nack { first: u16, mask: u16 },
}

/// The reports of a compound RTCP packet about `ssrc`; reading stops at the
/// first packet that does not hold together.
pub(crate) fn reports(compound: &[u8], ssrc: u32) -> Vec<Report> {
    let word = |bytes: &[u8], at: usize| -> Option<u32> {
        Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let mut found = Vec::new();
    let mut rest = compound;
    while rest.len() >= 4 && rest[0] >> 6 == 2 {
        let size = (usize::from(u16::from_be_bytes([rest[2], rest[3]])) + 1) * 4;
        let Some(packet) = rest.get(..size) else {
            break;
        };
        rest = &rest[size..];
        let (count, kind) = (usize::from(packet[0] & 0x1f), packet[1]);
        match kind {
            // Sender and receiver reports: a block per source they heard.
            200 | 201 => {
                let first = if kind == 200 { 28 } else { 8 };
                for block in 0..count {
                    let at = first + block * 24;
                    if word(packet, at) == Some(ssrc) {
                        found.extend(packet.get(at + 4).map(|lost| Report::Loss(*lost)));
                    }
                }
            }
            // Generic NACK: an entry per run of lost packets.
            205 if count == 1 && word(packet, 8) == Some(ssrc) => {
                let entries = packet.get(12..).unwrap_or_default().chunks_exact(4);
                found.extend(entries.map(|entry| Report::Nack {
                    first: u16::from_be_bytes([entry[0], entry[1]]),
                    mask: u16::from_be_bytes([entry[2], entry[3]]),
                }));
            }
            // PLI.
            206 if count == 1 && word(packet, 8) == Some(ssrc) => found.push(Report::KeyFrame),
            // FIR: an entry per source asked.
            206 if count == 4 => {
                let asked = packet
                    .get(12..)
                    .unwrap_or_default()
                    .chunks_exact(8)
                    .any(|entry| word(entry, 0) == Some(ssrc));
                if asked {
                    found.push(Report::KeyFrame);
                }
            }
            // REMB, for the sources it lists.
            206 if count == 15 && packet.get(12..16) == Some(b"REMB") => {
                let (Some(&sources), Some(rate)) = (packet.get(16), word(packet, 16)) else {
                    continue;
                };
                let listed =
                    (0..usize::from(sources)).any(|n| word(packet, 20 + n * 4) == Some(ssrc));
                let (exponent, mantissa) = ((rate >> 18) & 0x3f, rate & 0x3_ffff);
                if listed {
                    let bitrate = u64::from(mantissa) << exponent;
                    found.push(Report::Estimate(bitrate.min(u64::from(u32::MAX)) as u32));
                }
            }
            _ => {}
        }
    }
    found
}

/// A Picture Loss Indication (RFC 4585, PSFB FMT 1) from `sender` asking
/// `media` for a key frame: its first 8 bytes stay clear when sealed.
pub(crate) fn pli(sender: u32, media: u32) -> [u8; 12] {
    let mut packet = [0x81, 206, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0];
    packet[4..8].copy_from_slice(&sender.to_be_bytes());
    packet[8..].copy_from_slice(&media.to_be_bytes());
    packet
}

/// A generic NACK (RFC 4585, RTPFB FMT 1) from `sender` asking `media` for
/// the packets `lost` again, in order; at most [`super::nack::MAX_NACKED`]
/// of them. Its first 8 bytes stay clear when sealed.
pub(crate) fn nack(sender: u32, media: u32, lost: &[u16]) -> Vec<u8> {
    let mut entries: Vec<(u16, u16)> = Vec::new();
    for &sequence in lost.iter().take(super::nack::MAX_NACKED) {
        match entries.last_mut() {
            Some((first, mask)) if (1..=16).contains(&sequence.wrapping_sub(*first)) => {
                *mask |= 1 << (sequence.wrapping_sub(*first) - 1);
            }
            _ => entries.push((sequence, 0)),
        }
    }
    let words = (2 + entries.len()) as u16;
    let mut packet = vec![0x81, 205];
    packet.extend(words.to_be_bytes());
    packet.extend(sender.to_be_bytes());
    packet.extend(media.to_be_bytes());
    for (first, mask) in entries {
        packet.extend(first.to_be_bytes());
        packet.extend(mask.to_be_bytes());
    }
    packet
}

/// The 74-byte IP discovery request for `ssrc` (type 1, length 70).
pub(crate) fn discovery_request(ssrc: u32) -> [u8; 74] {
    let mut packet = [0; 74];
    packet[..4].copy_from_slice(&[0, 1, 0, 70]);
    packet[4..8].copy_from_slice(&ssrc.to_be_bytes());
    packet
}

/// The address and port the voice server sees for us, from its answer
/// (type 2): a NUL-terminated address in 64 bytes, then the port.
pub(crate) fn discovery_reply(packet: &[u8], ssrc: u32) -> Option<(std::net::IpAddr, u16)> {
    if packet.len() != 74 || packet[..4] != [0, 2, 0, 70] || packet[4..8] != ssrc.to_be_bytes() {
        return None;
    }
    let end = packet[8..72].iter().position(|byte| *byte == 0)?;
    let address = std::str::from_utf8(&packet[8..8 + end])
        .ok()?
        .parse()
        .ok()?;
    let port = u16::from_be_bytes([packet[72], packet[73]]);
    (port != 0).then_some((address, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn the_cipher_matches_the_xchacha_draft_vector() {
        // draft-irtf-cfrg-xchacha-03, appendix A.3.1.
        let key: [u8; 32] = std::array::from_fn(|n| 0x80 + n as u8);
        let nonce: [u8; 24] = std::array::from_fn(|n| 0x40 + n as u8);
        let sealed = XChaCha20Poly1305::new(&key.into())
            .encrypt(
                &nonce.into(),
                Payload {
                    msg: b"Ladies and Gentlemen of the class of '99: If I could offer you \
                           only one tip for the future, sunscreen would be it.",
                    aad: &hex("50515253c0c1c2c3c4c5c6c7"),
                },
            )
            .unwrap();
        assert_eq!(
            sealed,
            hex(concat!(
                "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb",
                "731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b452",
                "2f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff9",
                "21f9664c97637da9768812f615c68b13b52e",
                "c0875924c1c7987947deafd8780acf49"
            ))
        );
    }

    #[test]
    fn packets_are_framed_as_rtpsize_and_any_change_is_rejected() {
        let key = Zeroizing::new([7; 32]);
        let mut transport = Transport::new(&key);
        let header = header(0xfffe, 960, 0x0102_0304);
        assert_eq!(header, [0x80, 120, 0xff, 0xfe, 0, 0, 3, 0xc0, 1, 2, 3, 4]);
        let packet = transport.seal(&header, b"opus").unwrap();
        // Header, then ciphertext and tag, then the counter (1) that started the nonce.
        assert_eq!(&packet[..12], &header);
        assert_eq!(&packet[packet.len() - 4..], &[0, 0, 0, 1]);
        let mut nonce = [0; 24];
        nonce[3] = 1;
        let expected = XChaCha20Poly1305::new(&[7; 32].into())
            .encrypt(
                &nonce.into(),
                Payload {
                    msg: b"opus",
                    aad: &header,
                },
            )
            .unwrap();
        assert_eq!(&packet[12..packet.len() - 4], expected.as_slice());
        let opened = transport.open(&packet).unwrap();
        assert_eq!(
            opened,
            Rtp {
                ssrc: 0x0102_0304,
                payload_type: OPUS,
                marker: false,
                sequence: 0xfffe,
                timestamp: 960,
                payload: b"opus".to_vec()
            }
        );
        for at in 0..packet.len() {
            let mut forged = packet.clone();
            forged[at] ^= 0x01;
            assert!(transport.open(&forged).is_none(), "byte {at}");
            assert!(transport.open(&packet[..at]).is_none());
        }
        assert_eq!(
            &transport.seal(&header, b"x").unwrap()[13 + 16..],
            &[0, 0, 0, 2]
        );
        transport.counter = u32::MAX;
        assert!(transport.seal(&header, b"x").is_none(), "no nonce reuse");
    }

    #[test]
    fn an_encrypted_extension_body_is_stripped() {
        let key = Zeroizing::new([9; 32]);
        let transport = Transport::new(&key);
        let mut clear = header(1, 2, 3).to_vec();
        clear[0] |= 0x10;
        clear.extend([0xbe, 0xde, 0, 1]);
        let sealed = XChaCha20Poly1305::new(&[9; 32].into())
            .encrypt(
                &[0; 24].into(),
                Payload {
                    msg: &[0x10, 0xff, 0, 0, 0xaa, 0xbb],
                    aad: &clear,
                },
            )
            .unwrap();
        let mut packet = clear;
        packet.extend(sealed);
        packet.extend([0; 4]);
        assert_eq!(transport.open(&packet).unwrap().payload, [0xaa, 0xbb]);
    }

    /// The elements of an RFC 8285 one-byte extension body, padding skipped.
    fn elements(mut body: &[u8]) -> Vec<(u8, Vec<u8>)> {
        let mut found = Vec::new();
        while let Some((&first, rest)) = body.split_first() {
            if first == 0 {
                body = rest;
                continue;
            }
            let length = usize::from(first & 0x0f) + 1;
            found.push((first >> 4, rest[..length].to_vec()));
            body = &rest[length..];
        }
        found
    }

    #[test]
    fn one_byte_extensions_are_built_padded_to_a_word_and_opened_past() {
        // The screen's sound: one word, its element 9 holding 4.
        let (screen, body) = one_byte_extension(SCREEN_SOUND.0, &[SCREEN_SOUND.1]).unwrap();
        assert_eq!(
            (screen, elements(&body)),
            ([0xbe, 0xde, 0, 1], vec![(9, vec![4])])
        );
        // Four bytes of value take two words.
        let (preamble, body) = one_byte_extension(3, &[1, 2, 3, 4]).unwrap();
        assert_eq!(body.len(), 8);
        assert_eq!(
            (preamble[3], elements(&body)),
            (2, vec![(3, vec![1, 2, 3, 4])])
        );
        // Id 0 is padding and 15 reserved; a value is 1 to 16 bytes.
        for (id, value) in [(0, &[1][..]), (15, &[1]), (1, &[]), (1, &[0; 17])] {
            assert!(one_byte_extension(id, value).is_none(), "{id} {value:?}");
        }
        // Sealed and opened again: the payload alone comes back.
        let mut transport = Transport::new(&Zeroizing::new([9; 32]));
        let packet = transport
            .seal_extended(
                &header(1, 2, 3),
                (SCREEN_SOUND.0, &[SCREEN_SOUND.1]),
                b"opus",
            )
            .unwrap();
        assert_eq!(packet[0] & 0x10, 0x10, "the X bit");
        assert_eq!(packet[HEADER..HEADER + 4], screen);
        let opened = transport.open(&packet).unwrap();
        assert_eq!((opened.ssrc, &opened.payload[..]), (3, &b"opus"[..]));
    }

    #[test]
    fn a_pli_keeps_its_header_clear_and_rtcp_is_never_opened_as_media() {
        let mut transport = Transport::new(&Zeroizing::new([3; 32]));
        let pli = pli(11, 23);
        assert_eq!(pli, [0x81, 206, 0, 2, 0, 0, 0, 11, 0, 0, 0, 23]);
        let sealed = transport.seal(&pli[..8], &pli[8..]).unwrap();
        assert_eq!(&sealed[..8], &pli[..8]);
        assert_eq!(sealed.len(), 8 + 4 + 16 + 4);
        assert!(transport.open(&sealed).is_none());
    }

    #[test]
    fn rtcp_about_the_video_sent_is_opened_and_read() {
        let mut transport = Transport::new(&Zeroizing::new([4; 32]));
        // A receiver report with a block for another source and one for ours.
        let mut rr = vec![0x82, 201, 0, 13, 0, 0, 0, 9];
        for (source, lost) in [(77u32, 200u8), (23, 26)] {
            rr.extend(source.to_be_bytes());
            rr.push(lost);
            rr.extend([0; 19]);
        }
        let pli = pli(9, 23);
        let other = super::pli(9, 24);
        let mut fir = vec![0x84, 206, 0, 4, 0, 0, 0, 9, 0, 0, 0, 0];
        fir.extend(23u32.to_be_bytes());
        fir.extend([1, 0, 0, 0]);
        // REMB for one source: a mantissa of 1000 at exponent 10.
        let mut remb = vec![0x8f, 206, 0, 5, 0, 0, 0, 9, 0, 0, 0, 0];
        remb.extend(b"REMB");
        remb.extend((1u32 << 24 | 10 << 18 | 1000).to_be_bytes());
        remb.extend(23u32.to_be_bytes());
        let compound = [&rr[..], &pli, &other, &fir, &remb].concat();
        let sealed = transport.seal(&compound[..8], &compound[8..]).unwrap();
        let opened = transport.open_rtcp(&sealed).unwrap();
        assert_eq!(opened, compound);
        assert_eq!(
            reports(&opened, 23),
            [
                Report::Loss(26),
                Report::KeyFrame,
                Report::KeyFrame,
                Report::Estimate(1000 << 10)
            ]
        );
        assert!(reports(&opened, 5).is_empty(), "about another source");
        // Forged or cut, it is never read; a packet longer than it holds ends the reading.
        let mut forged = sealed.clone();
        forged[12] ^= 1;
        assert!(transport.open_rtcp(&forged).is_none());
        assert!(transport.open_rtcp(&sealed[..20]).is_none());
        assert!(transport.open(&sealed).is_none(), "never media");
        let mut long = pli.to_vec();
        long[3] = 9;
        assert!(reports(&long, 23).is_empty());
    }

    #[test]
    fn a_nack_names_runs_of_lost_packets_and_is_read_back() {
        let lost = [65_534, 65_535, 3, 40];
        let packet = nack(9, 23, &lost);
        assert_eq!(&packet[..12], &[0x81, 205, 0, 4, 0, 0, 0, 9, 0, 0, 0, 23]);
        let reports = reports(&packet, 23);
        assert_eq!(
            reports,
            [
                Report::Nack {
                    first: 65_534,
                    mask: 0b10001
                },
                Report::Nack { first: 40, mask: 0 },
            ]
        );
        let read: Vec<u16> = reports
            .iter()
            .flat_map(|report| match *report {
                Report::Nack { first, mask } => super::super::nack::lost(first, mask).collect(),
                _ => Vec::new(),
            })
            .collect();
        assert_eq!(read, lost);
        assert!(
            super::reports(&packet, 24).is_empty(),
            "about another source"
        );
        let many: Vec<u16> = (0..1000u16).map(|n| n.wrapping_mul(100)).collect();
        assert_eq!(
            nack(9, 23, &many).len(),
            12 + 4 * super::super::nack::MAX_NACKED
        );
    }

    #[test]
    fn ip_discovery_round_trip() {
        let request = discovery_request(0xabcd);
        assert_eq!(&request[..8], &[0, 1, 0, 70, 0, 0, 0xab, 0xcd]);
        assert!(request[8..].iter().all(|byte| *byte == 0));
        let mut reply = [0; 74];
        reply[..4].copy_from_slice(&[0, 2, 0, 70]);
        reply[4..8].copy_from_slice(&0xabcdu32.to_be_bytes());
        reply[8..8 + 11].copy_from_slice(b"203.0.113.9");
        reply[72..].copy_from_slice(&50_004u16.to_be_bytes());
        assert_eq!(
            discovery_reply(&reply, 0xabcd),
            Some(("203.0.113.9".parse().unwrap(), 50_004))
        );
        assert_eq!(discovery_reply(&reply, 0xabce), None, "another SSRC");
        assert_eq!(discovery_reply(&reply[..73], 0xabcd), None);
        reply[8..72].fill(b'1');
        assert_eq!(discovery_reply(&reply, 0xabcd), None, "unterminated");
    }
}
