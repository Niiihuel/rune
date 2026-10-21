//! Room for one more picture in a stream's SPS, for the GPU's decoders.
//!
//! GStreamer's H264 decoders (VA-API's and NVDEC's, 1.26) show a live
//! stream's picture as soon as it is decoded only while their picture
//! buffer has a free place: when it is exactly full, as with the
//! `max_dec_frame_buffering` equal to `max_num_ref_frames` that OpenH264
//! and libwebrtc write, each picture waits for the next one, and a still
//! screen shows its last change only when its sender repeats it. One more
//! place changes nothing else: no picture is reordered either way.
//!
//! shortcut: the SPS reading repeats `rune-discord-session`'s `voice::sps`
//! (the provider does not depend on this crate); share it if a third user
//! appears.

/// `unit`, an access unit in Annex B, with every SPS made to leave one
/// place free; as it is when none needs it.
pub(crate) fn roomy(unit: Vec<u8>) -> Vec<u8> {
    let mut nals = Vec::new();
    let mut at = 0;
    while let Some(start) = next_start(&unit, at) {
        let end = next_start(&unit, start).map_or(unit.len(), |next| next - 3);
        nals.push((start, end));
        at = start;
    }
    let rewritten: Vec<_> = nals
        .iter()
        .map(|&(start, end)| roomy_sps(&unit[start..end]))
        .collect();
    if rewritten.iter().all(Option::is_none) {
        return unit;
    }
    let mut out = Vec::with_capacity(unit.len() + 8);
    for (&(start, end), new) in nals.iter().zip(rewritten) {
        out.extend_from_slice(&[0, 0, 0, 1]);
        match new {
            Some(new) => out.extend_from_slice(&new),
            None => out.extend_from_slice(&unit[start..end]),
        }
    }
    out
}

/// Where the NAL after the start code found at or after `from` begins.
fn next_start(data: &[u8], from: usize) -> Option<usize> {
    data.get(from..)?
        .windows(3)
        .position(|code| code == [0, 0, 1])
        .map(|at| from + at + 3)
}

/// An SPS NAL whose picture buffer is exactly full of references, with one
/// place more; `None` for any other NAL, one with room, or one not read.
fn roomy_sps(nal: &[u8]) -> Option<Vec<u8>> {
    // A four-byte start code leaves its first zero at the end of the last NAL.
    let nal = nal.strip_suffix(&[0]).unwrap_or(nal);
    let (&header, escaped) = nal.split_first()?;
    if header & 0x1f != 7 {
        return None;
    }
    let rbsp = unescape(escaped);
    let (references, (from, _, buffering)) = buffering(&rbsp)?;
    if buffering > references || references >= 16 {
        return None;
    }
    let mut out = Writer::default();
    out.copy(&rbsp, 0, from);
    out.ue(references + 1);
    // rbsp_trailing_bits.
    out.bit(true);
    while !out.bits.is_multiple_of(8) {
        out.bit(false);
    }
    let mut rewritten = vec![header];
    escape(&out.bytes, &mut rewritten);
    Some(rewritten)
}

/// The SPS's `max_num_ref_frames` and where its `max_dec_frame_buffering`
/// is (first and last bit, value), when its VUI has a bitstream restriction.
fn buffering(rbsp: &[u8]) -> Option<(u32, (usize, usize, u32))> {
    let mut bits = Reader { data: rbsp, at: 0 };
    let profile = bits.bits(8)?;
    bits.bits(16)?; // constraint flags and level
    bits.ue()?; // seq_parameter_set_id
    if [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135].contains(&profile) {
        let chroma = bits.ue()?;
        if chroma == 3 {
            bits.bit()?;
        }
        bits.ue()?;
        bits.ue()?;
        bits.bit()?;
        if bits.bit()? == 1 {
            for list in 0..if chroma == 3 { 12 } else { 8 } {
                if bits.bit()? == 1 {
                    let (mut last, mut next) = (8i32, 8i32);
                    for _ in 0..if list < 6 { 16 } else { 64 } {
                        if next != 0 {
                            // Wrapping: a crafted delta near i32::MAX must not overflow.
                            next = last.wrapping_add(bits.se()?).rem_euclid(256);
                        }
                        last = if next == 0 { last } else { next };
                    }
                }
            }
        }
    }
    bits.ue()?; // log2_max_frame_num_minus4
    match bits.ue()? {
        0 => {
            bits.ue()?;
        }
        1 => {
            bits.bit()?;
            bits.se()?;
            bits.se()?;
            for _ in 0..bits.ue()?.min(255) {
                bits.se()?;
            }
        }
        _ => {}
    }
    let references = bits.ue()?;
    bits.bit()?; // gaps_in_frame_num_value_allowed_flag
    bits.ue()?;
    bits.ue()?;
    if bits.bit()? == 0 {
        bits.bit()?; // mb_adaptive_frame_field_flag
    }
    bits.bit()?; // direct_8x8_inference_flag
    if bits.bit()? == 1 {
        for _ in 0..4 {
            bits.ue()?;
        }
    }
    if bits.bit()? == 0 {
        return None; // no VUI: the buffer is the level's, larger
    }
    if bits.bit()? == 1 && bits.bits(8)? == 255 {
        bits.bits(32)?;
    }
    if bits.bit()? == 1 {
        bits.bit()?;
    }
    if bits.bit()? == 1 {
        bits.bits(4)?;
        if bits.bit()? == 1 {
            bits.bits(24)?;
        }
    }
    if bits.bit()? == 1 {
        bits.ue()?;
        bits.ue()?;
    }
    if bits.bit()? == 1 {
        bits.bits(32)?;
        bits.bits(32)?;
        bits.bit()?;
    }
    let mut hrd = false;
    for _ in 0..2 {
        if bits.bit()? == 1 {
            hrd = true;
            let count = bits.ue()?.min(31) + 1;
            bits.bits(8)?;
            for _ in 0..count {
                bits.ue()?;
                bits.ue()?;
                bits.bit()?;
            }
            bits.bits(20)?;
        }
    }
    if hrd {
        bits.bit()?; // low_delay_hrd_flag
    }
    bits.bit()?; // pic_struct_present_flag
    if bits.bit()? == 0 {
        return None; // no restriction: the buffer is the level's, larger
    }
    bits.bit()?;
    for _ in 0..5 {
        bits.ue()?; // byte and bit ceilings, vector lengths, reordering
    }
    let from = bits.at;
    let buffering = bits.ue()?;
    Some((references, (from, bits.at, buffering)))
}

/// The RBSP of an escaped NAL payload: `00 00 03` loses its `03`.
fn unescape(escaped: &[u8]) -> Vec<u8> {
    let mut rbsp = Vec::with_capacity(escaped.len());
    let mut zeros = 0;
    for &byte in escaped {
        if zeros >= 2 && byte == 3 {
            zeros = 0;
            continue;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        rbsp.push(byte);
    }
    rbsp
}

/// `rbsp` escaped into `out`: no `00 00 0x` with x ≤ 3 is left.
fn escape(rbsp: &[u8], out: &mut Vec<u8>) {
    let mut zeros = 0;
    for &byte in rbsp {
        if zeros >= 2 && byte <= 3 {
            out.push(3);
            zeros = 0;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        out.push(byte);
    }
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn bit(&mut self) -> Option<u32> {
        let byte = *self.data.get(self.at / 8)?;
        let bit = (byte >> (7 - self.at % 8)) & 1;
        self.at += 1;
        Some(u32::from(bit))
    }

    fn bits(&mut self, count: u32) -> Option<u32> {
        (0..count).try_fold(0, |value, _| Some(value << 1 | self.bit()?))
    }

    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some((1u32 << zeros) - 1 + self.bits(zeros)?)
    }

    fn se(&mut self) -> Option<i32> {
        let code = self.ue()? as i64;
        Some(if code % 2 == 1 {
            (code + 1) / 2
        } else {
            -code / 2
        } as i32)
    }
}

#[derive(Default)]
struct Writer {
    bytes: Vec<u8>,
    bits: usize,
}

impl Writer {
    fn bit(&mut self, on: bool) {
        if self.bits.is_multiple_of(8) {
            self.bytes.push(0);
        }
        if on {
            *self.bytes.last_mut().expect("pushed") |= 0x80 >> (self.bits % 8);
        }
        self.bits += 1;
    }

    fn ue(&mut self, value: u32) {
        let code = u64::from(value) + 1;
        let length = 64 - code.leading_zeros();
        for _ in 1..length {
            self.bit(false);
        }
        for at in (0..length).rev() {
            self.bit(code >> at & 1 == 1);
        }
    }

    /// The bits of `data` from `from` to `to`.
    fn copy(&mut self, data: &[u8], from: usize, to: usize) {
        for at in from..to {
            self.bit(data[at / 8] >> (7 - at % 8) & 1 == 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use openh264::decoder::Decoder;
    use openh264::encoder::Encoder;
    use openh264::formats::{RgbSliceU8, YUVBuffer, YUVSource};

    fn key_unit() -> Vec<u8> {
        let mut encoder = Encoder::new().unwrap();
        let pixels = [90u8, 30, 200].repeat(320 * 180);
        let picture = YUVBuffer::from_rgb8_source(RgbSliceU8::new(&pixels, (320, 180)));
        encoder.encode(&picture).unwrap().to_vec()
    }

    fn sps(unit: &[u8]) -> Vec<u8> {
        let start = next_start(unit, 0).unwrap();
        let end = next_start(unit, start).map_or(unit.len(), |next| next - 3);
        unescape(&unit[start + 1..end])
    }

    #[test]
    fn openh264s_full_buffer_gets_one_place_more_and_still_decodes() {
        let unit = key_unit();
        let (references, (_, _, buffering)) = buffering(&sps(&unit)).expect("a restriction");
        assert_eq!(buffering, references, "as OpenH264 writes it");
        let roomy = super::roomy(unit.clone());
        assert_ne!(roomy, unit);
        assert_eq!(buffering_of(&roomy), Some((references, references + 1)));
        // Everything else as it was: it decodes to the same picture.
        let mut decoder = Decoder::new().unwrap();
        let picture = decoder.decode(&roomy).unwrap().unwrap();
        assert_eq!(picture.dimensions(), (320, 180));
        // Once is enough: a roomy SPS is left alone, as is a unit without one.
        assert_eq!(super::roomy(roomy.clone()), roomy);
        let delta = vec![0, 0, 0, 1, 0x41, 0x9a, 0x02];
        assert_eq!(super::roomy(delta.clone()), delta);
    }

    fn buffering_of(unit: &[u8]) -> Option<(u32, u32)> {
        buffering(&sps(unit)).map(|(references, (_, _, buffering))| (references, buffering))
    }

    #[test]
    fn exp_golomb_and_escaping_round_trip() {
        let mut out = Writer::default();
        for value in [0, 1, 2, 7, 255, 65_535] {
            out.ue(value);
        }
        let mut bits = Reader {
            data: &out.bytes,
            at: 0,
        };
        for value in [0, 1, 2, 7, 255, 65_535] {
            assert_eq!(bits.ue(), Some(value));
        }
        let rbsp = [0, 0, 1, 0, 0, 0, 0, 0, 3, 7];
        let mut escaped = Vec::new();
        escape(&rbsp, &mut escaped);
        assert!(!escaped
            .windows(3)
            .any(|code| code[0] == 0 && code[1] == 0 && code[2] <= 2));
        assert_eq!(unescape(&escaped), rbsp);
    }
}
