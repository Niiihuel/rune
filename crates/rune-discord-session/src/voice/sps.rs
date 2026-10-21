//! The SPS of the account's own H264, made to say that no picture is ever
//! reordered before DAVE encrypts it.
//!
//! libwebrtc's receiver (`sps_vui_rewriter.cc`, which Discord's native
//! client carries) rewrites an incoming SPS whose VUI lacks a bitstream
//! restriction with `max_num_reorder_frames = 0` and
//! `max_dec_frame_buffering ≤ max_num_ref_frames`. DAVE leaves the SPS clear
//! but authenticates it, so a rewritten one fails and the picture is lost:
//! Serein's finding (2026-09-21), not confirmed here (docs/DISCORD.md). Every SPS
//! sent therefore already carries that restriction, written the way the
//! rewriter writes it, so it finds nothing to change. Constrained Baseline
//! never reorders, so the claim is true.

/// An SPS NAL with its header, or one that needs no change (`None`): the
/// restriction is there, or it cannot be read (left as the encoder made it).
pub(crate) fn normalize(nal: &[u8]) -> Option<Vec<u8>> {
    let (&header, escaped) = nal.split_first()?;
    if header & 0x1f != 7 {
        return None;
    }
    let rbsp = unescape(escaped);
    let sps = parse(&rbsp)?;
    let mut out = Writer::default();
    match sps.restriction {
        // It says what the receiver wants already.
        Some(Restriction {
            reorder: 0,
            buffering,
            ..
        }) if buffering <= sps.references => return None,
        // Kept as it is but for those two, as the rewriter does.
        Some(Restriction { values, .. }) => {
            out.copy(&rbsp, sps.restriction_flag + 1);
            let [motion, bytes, bits, horizontal, vertical] = values;
            out.bit(motion != 0);
            for value in [bytes, bits, horizontal, vertical] {
                out.ue(value);
            }
        }
        // A VUI without the restriction, or none at all: the rewriter's defaults.
        None => {
            if sps.vui {
                out.copy(&rbsp, sps.restriction_flag);
            } else {
                // The VUI is there now, every part of it absent but the restriction.
                out.copy(&rbsp, sps.vui_flag);
                out.bit(true);
                for _ in 0..8 {
                    out.bit(false);
                }
            }
            out.bit(true);
            out.bit(true);
            for value in [2, 1, 16, 16] {
                out.ue(value);
            }
        }
    }
    out.ue(0);
    out.ue(sps.references);
    // rbsp_trailing_bits.
    out.bit(true);
    while !out.bits.is_multiple_of(8) {
        out.bit(false);
    }
    let mut rewritten = vec![header];
    escape(&out.bytes, &mut rewritten);
    Some(rewritten)
}

/// `sps` cut at its VUI: the same picture, saying nothing of reordering,
/// as an encoder other than OpenH264 may send it. For fixtures.
#[cfg(test)]
pub(crate) fn without_vui(sps: &[u8]) -> Vec<u8> {
    let rbsp = unescape(&sps[1..]);
    let mut writer = Writer::default();
    writer.copy(&rbsp, parse(&rbsp).expect("an SPS").vui_flag);
    writer.bit(false);
    writer.bit(true);
    while !writer.bits.is_multiple_of(8) {
        writer.bit(false);
    }
    let mut nal = vec![sps[0]];
    escape(&writer.bytes, &mut nal);
    nal
}

/// The bitstream restriction of a VUI: the values before the two that
/// matter, and those two.
struct Restriction {
    values: [u32; 5],
    reorder: u32,
    buffering: u32,
}

struct Sps {
    references: u32,
    /// The VUI is present.
    vui: bool,
    /// Where `vui_parameters_present_flag` is, and with a VUI where its
    /// `bitstream_restriction_flag` is, in bits.
    vui_flag: usize,
    restriction_flag: usize,
    restriction: Option<Restriction>,
}

/// The fields of an SPS up to its VUI's end, `None` if it is malformed or
/// uses what a stream never does (scaling matrices are read and kept).
fn parse(rbsp: &[u8]) -> Option<Sps> {
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
                    let size = if list < 6 { 16 } else { 64 };
                    let (mut last, mut next) = (8i32, 8i32);
                    for _ in 0..size {
                        if next != 0 {
                            next = (last + bits.se()? + 256) % 256;
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
    let flag = bits.at;
    if bits.bit()? == 0 {
        return Some(Sps {
            references,
            vui: false,
            vui_flag: flag,
            restriction_flag: flag,
            restriction: None,
        });
    }
    if bits.bit()? == 1 && bits.bits(8)? == 255 {
        bits.bits(32)?;
    }
    if bits.bit()? == 1 {
        bits.bit()?;
    }
    if bits.bit()? == 1 {
        // video_format and video_full_range_flag; then whether the colours follow.
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
    let start = bits.at;
    let restriction = if bits.bit()? == 1 {
        Some(Restriction {
            values: [bits.bit()?, bits.ue()?, bits.ue()?, bits.ue()?, bits.ue()?],
            reorder: bits.ue()?,
            buffering: bits.ue()?,
        })
    } else {
        None
    };
    Some(Sps {
        references,
        vui: true,
        vui_flag: flag,
        restriction_flag: start,
        restriction,
    })
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

/// `rbsp` escaped again onto `out`, so it never holds a start code.
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
        let byte = self.data.get(self.at / 8)?;
        let bit = (byte >> (7 - self.at % 8)) & 1;
        self.at += 1;
        Some(u32::from(bit))
    }

    fn bits(&mut self, count: u32) -> Option<u32> {
        (0..count).try_fold(0, |value, _| Some(value << 1 | self.bit()?))
    }

    /// Exp-Golomb, unsigned; longer than 31 bits is malformed.
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        let rest = self.bits(zeros)?;
        ((1u64 << zeros) - 1 + u64::from(rest)).try_into().ok()
    }

    fn se(&mut self) -> Option<i32> {
        let code = i64::from(self.ue()?);
        let value = if code % 2 == 1 {
            (code + 1) / 2
        } else {
            -code / 2
        };
        i32::try_from(value).ok()
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
            *self.bytes.last_mut().unwrap_or(&mut 0) |= 0x80 >> (self.bits % 8);
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

    /// The first `count` bits of `data`.
    fn copy(&mut self, data: &[u8], count: usize) {
        for at in 0..count {
            self.bit(data[at / 8] >> (7 - at % 8) & 1 == 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::h264::nals;

    /// The SPS of `unit`, Annex B.
    fn sps_of(unit: &[u8]) -> Vec<u8> {
        nals(unit)
            .into_iter()
            .find(|nal| nal[0] & 0x1f == 7)
            .unwrap()
            .to_vec()
    }

    fn encoded() -> Vec<u8> {
        let mut encoder = openh264::encoder::Encoder::new().unwrap();
        let pixels = [30u8, 120, 200].repeat(64 * 48);
        let yuv = openh264::formats::YUVBuffer::from_rgb8_source(
            openh264::formats::RgbSliceU8::new(&pixels, (64, 48)),
        );
        encoder.encode(&yuv).unwrap().to_vec()
    }

    #[test]
    fn exp_golomb_and_escaping_round_trip() {
        let mut writer = Writer::default();
        for value in [0, 1, 2, 7, 16, 255, 65_535] {
            writer.ue(value);
        }
        let mut reader = Reader {
            data: &writer.bytes,
            at: 0,
        };
        for value in [0, 1, 2, 7, 16, 255, 65_535] {
            assert_eq!(reader.ue(), Some(value));
        }
        let mut escaped = Vec::new();
        escape(&[0, 0, 1, 0, 0, 0, 0, 0, 3, 5], &mut escaped);
        assert_eq!(escaped, [0, 0, 3, 1, 0, 0, 3, 0, 0, 3, 0, 3, 5]);
        assert_eq!(unescape(&escaped), [0, 0, 1, 0, 0, 0, 0, 0, 3, 5]);
    }

    #[test]
    fn openh264_already_says_no_reordering_and_is_left_alone() {
        let sps = sps_of(&encoded());
        let parsed = parse(&unescape(&sps[1..])).unwrap();
        assert!(parsed.vui);
        assert!(matches!(
            parsed.restriction,
            Some(Restriction { reorder: 0, .. })
        ));
        assert_eq!(normalize(&sps), None);
        // Other NALs are never touched.
        assert_eq!(normalize(&[0x68, 0xce, 0x3c, 0x80]), None);
        assert_eq!(normalize(&[0x67]), None, "unreadable: left as is");
    }

    #[test]
    fn an_sps_without_the_restriction_gains_it_and_still_decodes() {
        let unit = encoded();
        let sps = sps_of(&unit);
        let bare = without_vui(&sps);
        let bare_parsed = parse(&unescape(&bare[1..])).unwrap();
        assert!(!bare_parsed.vui);
        let fixed = normalize(&bare).expect("rewritten");
        let parsed = parse(&unescape(&fixed[1..])).unwrap();
        let restriction = parsed.restriction.unwrap();
        assert_eq!(
            (restriction.reorder, restriction.buffering),
            (0, parsed.references)
        );
        assert_eq!(restriction.values, [1, 2, 1, 16, 16]);
        assert_eq!(normalize(&fixed), None, "a second pass finds nothing to do");
        // The picture still decodes with the rewritten SPS in its place.
        let mut replaced = Vec::new();
        for nal in nals(&unit) {
            replaced.extend([0, 0, 0, 1]);
            replaced.extend(if nal[0] & 0x1f == 7 { &fixed[..] } else { nal });
        }
        let mut decoder = openh264::decoder::Decoder::new().unwrap();
        let picture = decoder.decode(&replaced).unwrap().unwrap();
        assert_eq!(openh264::formats::YUVSource::dimensions(&picture), (64, 48));
    }

    #[test]
    fn a_vui_with_its_video_signal_type_is_read_whole() {
        // NVENC's (GStreamer 1.26, 1920×1080): colours and timing, no restriction.
        let sps = [
            0x67, 0x42, 0xc0, 0x2a, 0x95, 0x90, 0x07, 0x80, 0x22, 0x7e, 0x5c, 0x05, 0xa8, 0x08,
            0x08, 0x0a, 0x00, 0x00, 0x07, 0xd0, 0x00, 0x03, 0xa9, 0x81, 0x08,
        ];
        let parsed = parse(&unescape(&sps[1..])).unwrap();
        assert!(parsed.vui && parsed.restriction.is_none());
        assert_eq!(parsed.references, 3);
        // The restriction goes after the timing, which stays.
        let fixed = normalize(&sps).expect("rewritten");
        assert_eq!(fixed[..sps.len() - 1], sps[..sps.len() - 1]);
        let restriction = parse(&unescape(&fixed[1..])).unwrap().restriction.unwrap();
        assert_eq!((restriction.reorder, restriction.buffering), (0, 3));
        assert_eq!(normalize(&fixed), None);
    }

    #[test]
    fn a_restriction_that_allows_reordering_is_rewritten_keeping_the_rest() {
        let sps = sps_of(&encoded());
        let rbsp = unescape(&sps[1..]);
        let parsed = parse(&rbsp).unwrap();
        let mut writer = Writer::default();
        writer.copy(&rbsp, parsed.restriction_flag);
        writer.bit(true);
        writer.bit(false);
        for value in [3, 4, 9, 10, 2, 5] {
            writer.ue(value);
        }
        writer.bit(true);
        while !writer.bits.is_multiple_of(8) {
            writer.bit(false);
        }
        let mut reordering = vec![sps[0]];
        escape(&writer.bytes, &mut reordering);
        let fixed = normalize(&reordering).unwrap();
        let restriction = parse(&unescape(&fixed[1..])).unwrap().restriction.unwrap();
        assert_eq!(restriction.values, [0, 3, 4, 9, 10]);
        assert_eq!(
            (restriction.reorder, restriction.buffering),
            (0, parsed.references)
        );
    }
}
