//! Ogg Opus (RFC 7845), as voice messages are: RUNE's own reader of the
//! pages, each checked against its checksum, and libopus's decoders, for one
//! mono or stereo stream (channel mapping family 0) or up to eight channels
//! in several streams (family 1), downmixed to mono. What the encoder padded
//! past the last page's granule position is not played.

use std::ptr::NonNull;

use rune_i18n::t;

use super::{invalid, Sound, MAX_PACKET_BYTES};
use crate::codec::{Decoder, MAX_PACKET_SAMPLES};
use crate::{MediaError, PLAYBACK_RATE};

/// The first logical stream of an Ogg Opus file.
struct Opus {
    /// Samples the decoder must drop first.
    pre_skip: u64,
    /// Where its last page's granule position falls in what decodes, from
    /// the first sample, pre-skip included; what decodes past it is padding.
    /// `None` when the granules do not say (none past the pre-skip).
    end: Option<u64>,
    layout: Layout,
    packets: Vec<Vec<u8>>,
}

/// How the header lays out the channels.
#[derive(Clone, Debug, PartialEq)]
enum Layout {
    /// One mono or stereo stream, which the mono decoder downmixes.
    Plain,
    /// `channels` in `streams`, the first `coupled` of them stereo, each
    /// channel's stream (or 255, silence) in `mapping`.
    Streams {
        channels: u8,
        streams: u8,
        coupled: u8,
        mapping: Vec<u8>,
    },
}

/// Ogg's page checksum: CRC-32, polynomial 0x04C11DB7, not reflected, from 0.
const CRC: [u32; 256] = {
    let mut table = [0; 256];
    let mut index = 0;
    while index < 256 {
        let mut remainder = (index as u32) << 24;
        let mut bit = 0;
        while bit < 8 {
            remainder = if remainder & 0x8000_0000 != 0 {
                (remainder << 1) ^ 0x04C1_1DB7
            } else {
                remainder << 1
            };
            bit += 1;
        }
        table[index] = remainder;
        index += 1;
    }
    table
};

/// The checksum of `page`, header and body, whose own four bytes count as zeros.
fn checksum(page: &[u8]) -> u32 {
    page.iter().enumerate().fold(0, |crc, (at, &byte)| {
        let byte = if (22..26).contains(&at) { 0 } else { byte };
        (crc << 8) ^ CRC[usize::from((crc >> 24) as u8 ^ byte)]
    })
}

/// Read `ogg`: a page cut short, or one that is not what its checksum
/// says, refuses the whole file.
fn read(ogg: &[u8]) -> Result<Opus, MediaError> {
    let mut packets = Vec::new();
    let mut partial = Vec::new();
    let mut serial = None;
    let mut end = None;
    // The first page that ends an audio packet: its granule, and those packets.
    let mut first = None;
    let mut rest = ogg;
    while !rest.is_empty() {
        if rest.len() < 27 || &rest[..4] != b"OggS" || rest[4] != 0 {
            return Err(invalid());
        }
        let header = 27 + usize::from(rest[26]);
        let table = rest.get(27..header).ok_or_else(invalid)?;
        let length: usize = table.iter().map(|&lacing| usize::from(lacing)).sum();
        let page = rest.get(..header + length).ok_or_else(invalid)?;
        if checksum(page) != u32::from_le_bytes([page[22], page[23], page[24], page[25]]) {
            return Err(invalid());
        }
        let body = &page[header..];
        let page_serial = u32::from_le_bytes([page[14], page[15], page[16], page[17]]);
        let granule = u64::from_le_bytes(page[6..14].try_into().map_err(|_| invalid())?);
        rest = &rest[page.len()..];
        if *serial.get_or_insert(page_serial) != page_serial {
            continue;
        }
        // The two headers are packets 0 and 1; audio follows.
        let before = packets.len().max(2);
        let mut offset = 0;
        for &lacing in table {
            let lacing = usize::from(lacing);
            partial.extend_from_slice(&body[offset..offset + lacing]);
            offset += lacing;
            if partial.len() > MAX_PACKET_BYTES {
                return Err(invalid());
            }
            // A segment shorter than 255 bytes ends its packet.
            if lacing < 255 && !partial.is_empty() {
                packets.push(std::mem::take(&mut partial));
            }
        }
        // -1: no packet ends on this page.
        if granule != u64::MAX {
            end = Some(granule);
            if first.is_none() && packets.len() > before {
                first = Some((granule, before..packets.len()));
            }
        }
    }
    let head = packets
        .first()
        .filter(|head| head.len() >= 19 && head.starts_with(b"OpusHead"))
        .ok_or_else(invalid)?;
    let unsupported = || {
        MediaError(t("Este mensaje de voz usa un formato de audio que RUNE no reproduce.").into())
    };
    if head[8] >> 4 != 0 {
        return Err(unsupported());
    }
    let channels = head[9];
    let layout = match head[18] {
        0 if (1..=2).contains(&channels) => Layout::Plain,
        // Vorbis's channel order, up to 7.1; libopus checks the table.
        1 if (1..=8).contains(&channels) => {
            let table = head
                .get(19..21 + usize::from(channels))
                .ok_or_else(invalid)?;
            Layout::Streams {
                channels,
                streams: table[0],
                coupled: table[1],
                mapping: table[2..].to_vec(),
            }
        }
        // Ambisonics and layouts of no defined order are not voice messages.
        _ => return Err(unsupported()),
    };
    let pre_skip = u64::from(u16::from_le_bytes([head[10], head[11]]));
    if !packets
        .get(1)
        .is_some_and(|tags| tags.starts_with(b"OpusTags"))
    {
        return Err(invalid());
    }
    // Granules count from where the stream starts: not 0 when it was cut
    // from a longer one (RFC 7845 §4.5). A first page whose granule is short
    // of its own packets is the last one too, trimmed at its end.
    let start = match first {
        Some((granule, range)) => {
            let mut samples = 0;
            for packet in &packets[range] {
                let length = opus::packet::get_nb_samples(packet, PLAYBACK_RATE);
                samples += length.map_err(|_| invalid())? as u64;
            }
            granule.saturating_sub(samples)
        }
        None => 0,
    };
    // Granules that end before the pre-skip (a muxer that writes 0) say
    // nothing: all of it plays, as it decodes.
    let end = end
        .map(|end| end.saturating_sub(start))
        .filter(|&end| end > pre_skip);
    super::too_long(end.map_or(0, |end| end - pre_skip))?;
    packets.drain(..2);
    Ok(Opus {
        pre_skip,
        end,
        layout,
        packets,
    })
}

/// libopus's decoder of several streams, which the `opus` bindings do not
/// wrap: the same bundled library, through the bindings they build on.
struct Streams {
    state: NonNull<opusic_sys::OpusMSDecoder>,
    channels: usize,
    /// One packet's samples, interleaved.
    pcm: Vec<f32>,
}

// SAFETY: libopus's decoder state is plain memory reached only through this
// pointer, which `Streams` owns; `&mut self` keeps it to one thread at a time.
unsafe impl Send for Streams {}

impl Streams {
    fn new(channels: u8, streams: u8, coupled: u8, mapping: &[u8]) -> Result<Self, MediaError> {
        if mapping.len() != usize::from(channels) {
            return Err(invalid());
        }
        let mut error = 0;
        // SAFETY: `mapping` holds `channels` entries, read during the call
        // only; libopus checks the counts and the table and returns null
        // when they do not fit together.
        let state = unsafe {
            opusic_sys::opus_multistream_decoder_create(
                PLAYBACK_RATE as i32,
                channels.into(),
                streams.into(),
                coupled.into(),
                mapping.as_ptr(),
                &mut error,
            )
        };
        let state = NonNull::new(state).ok_or_else(invalid)?;
        let decoder = Self {
            state,
            channels: channels.into(),
            pcm: vec![0.0; MAX_PACKET_SAMPLES * usize::from(channels)],
        };
        (error == opusic_sys::OPUS_OK)
            .then_some(decoder)
            .ok_or_else(invalid)
    }

    /// Decode `packet` into `out` (room for [`MAX_PACKET_SAMPLES`]) as mono,
    /// its channels averaged; returns the samples written.
    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, MediaError> {
        if packet.is_empty() || out.len() < MAX_PACKET_SAMPLES {
            return Err(invalid());
        }
        // SAFETY: `packet` is read during the call only and is shorter than
        // `MAX_PACKET_BYTES`; `pcm` holds `MAX_PACKET_SAMPLES` samples of
        // every channel, the most a frame size of `MAX_PACKET_SAMPLES` writes.
        let decoded = unsafe {
            opusic_sys::opus_multistream_decode_float(
                self.state.as_ptr(),
                packet.as_ptr(),
                packet.len() as i32,
                self.pcm.as_mut_ptr(),
                MAX_PACKET_SAMPLES as i32,
                0,
            )
        };
        // Negative: libopus's error for a broken packet.
        let decoded = usize::try_from(decoded).map_err(|_| invalid())?;
        let samples = self.pcm.chunks_exact(self.channels).take(decoded);
        for (mono, frame) in out.iter_mut().zip(samples) {
            *mono = frame.iter().sum::<f32>() / self.channels as f32;
        }
        Ok(decoded.min(MAX_PACKET_SAMPLES))
    }
}

impl Drop for Streams {
    fn drop(&mut self) {
        // SAFETY: made by `opus_multistream_decoder_create` and freed once, here.
        unsafe { opusic_sys::opus_multistream_decoder_destroy(self.state.as_ptr()) }
    }
}

enum Decoding {
    Plain(Decoder),
    Streams(Streams),
}

impl Decoding {
    fn new(layout: &Layout) -> Result<Self, MediaError> {
        Ok(match layout {
            Layout::Plain => Self::Plain(Decoder::new()?),
            Layout::Streams {
                channels,
                streams,
                coupled,
                mapping,
            } => Self::Streams(Streams::new(*channels, *streams, *coupled, mapping)?),
        })
    }

    fn decode(&mut self, packet: &[u8], out: &mut [f32]) -> Result<usize, MediaError> {
        match self {
            Self::Plain(decoder) => decoder.decode(packet, out),
            Self::Streams(decoder) => decoder.decode(packet, out),
        }
    }
}

/// An Ogg Opus file, read whole and decoded as it plays.
pub(super) struct OggOpus {
    opus: Opus,
    decoder: Decoding,
    /// The next packet, and the samples decoded before it from the start.
    next: usize,
    at: u64,
    /// The first sample played: after the pre-skip and where it was sought.
    from: u64,
    out: Vec<f32>,
}

impl OggOpus {
    pub(super) fn open(ogg: &[u8]) -> Result<Self, MediaError> {
        let opus = read(ogg)?;
        Ok(Self {
            decoder: Decoding::new(&opus.layout)?,
            next: 0,
            at: 0,
            from: opus.pre_skip,
            opus,
            out: vec![0.0; MAX_PACKET_SAMPLES],
        })
    }
}

impl Sound for OggOpus {
    fn next(&mut self, out: &mut Vec<f32>) -> Result<bool, MediaError> {
        let end = self.opus.end.unwrap_or(u64::MAX);
        let Some(packet) = self.opus.packets.get(self.next).filter(|_| self.at < end) else {
            return Ok(false);
        };
        let length = self.decoder.decode(packet, &mut self.out)? as u64;
        self.next += 1;
        let start = self.at;
        self.at += length;
        // Only what lies after the pre-skip or the place sought, and before the end.
        let first = (self.from.clamp(start, self.at) - start) as usize;
        let last = (end.clamp(start, self.at) - start) as usize;
        if first < last {
            out.extend_from_slice(&self.out[first..last]);
        }
        Ok(true)
    }

    fn seek(&mut self, samples: u64) -> Result<(), MediaError> {
        // shortcut: a seek decodes again from the start, milliseconds for
        // a clip of minutes; index pages by granule for hour-long audio.
        self.decoder = Decoding::new(&self.opus.layout)?;
        (self.next, self.at) = (0, 0);
        self.from = self.opus.pre_skip.saturating_add(samples);
        Ok(())
    }

    fn length(&self) -> Option<u64> {
        Some(self.opus.end? - self.opus.pre_skip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four seconds of a synthesized, speech-like tone (see docs/MEDIA.md).
    const SAMPLE: &[u8] = include_bytes!("../../../../assets/voice-sample.ogg");
    /// 110 ms of a 440 Hz tone, mono: its last packet is mostly padding.
    const SHORT: &[u8] = include_bytes!("../../fixtures/short.ogg");
    /// One second of 440 Hz in the third of three channels (left, centre,
    /// right in Vorbis's order): two streams, one coupled.
    const SURROUND: &[u8] = include_bytes!("../../fixtures/surround.ogg");

    /// Everything `ogg` plays, from `from` samples in.
    fn played(ogg: &[u8], from: u64) -> Vec<f32> {
        let mut sound = OggOpus::open(ogg).unwrap();
        sound.seek(from).unwrap();
        let mut out = Vec::new();
        while sound.next(&mut out).unwrap() {}
        out
    }

    /// `ogg` with every page's checksum made right again, after a change.
    fn sealed(ogg: Vec<u8>) -> Vec<u8> {
        regranuled(ogg, |granule| granule)
    }

    /// `ogg` with each page's granule position changed, and sealed.
    fn regranuled(mut ogg: Vec<u8>, change: impl Fn(u64) -> u64) -> Vec<u8> {
        let mut at = 0;
        while at < ogg.len() {
            let granule = u64::from_le_bytes(ogg[at + 6..at + 14].try_into().unwrap());
            ogg[at + 6..at + 14].copy_from_slice(&change(granule).to_le_bytes());
            let header = 27 + usize::from(ogg[at + 26]);
            let length: usize = ogg[at + 27..at + header]
                .iter()
                .map(|&n| usize::from(n))
                .sum();
            let page = at..at + header + length;
            let crc = checksum(&ogg[page.clone()]);
            ogg[at + 22..at + 26].copy_from_slice(&crc.to_le_bytes());
            at = page.end;
        }
        ogg
    }

    #[test]
    fn a_real_file_plays_exactly_its_length_without_the_padding() {
        let sample = played(SAMPLE, 0);
        assert_eq!(sample.len(), 4 * PLAYBACK_RATE as usize);
        assert!(crate::rms(&sample) > 0.05, "the tone is there");
        let opus = read(SHORT).unwrap();
        assert_eq!((opus.pre_skip, opus.end), (312, Some(312 + 5_280)));
        let mut decoder = Decoder::new().unwrap();
        let mut out = vec![0.0; MAX_PACKET_SAMPLES];
        let decoded: usize = opus
            .packets
            .iter()
            .map(|packet| decoder.decode(packet, &mut out).unwrap())
            .sum();
        assert!(decoded - 312 > 5_280, "the encoder padded it");
        let short = played(SHORT, 0);
        assert_eq!(short.len(), 5_280, "110 ms and not one sample more");
        assert_eq!(OggOpus::open(SHORT).unwrap().length(), Some(5_280));
        // A jump keeps the end where it is.
        assert_eq!(played(SHORT, 4_800).len(), 480);
        assert!(played(SHORT, 9_000).is_empty());
    }

    #[test]
    fn granules_count_from_where_the_stream_starts() {
        let four = 4 * PLAYBACK_RATE as u64;
        // Cut from a recording ten seconds in: four seconds still, to the sample.
        let later = regranuled(SAMPLE.to_vec(), |granule| match granule {
            0 => 0,
            granule => granule + 10 * u64::from(PLAYBACK_RATE),
        });
        assert_eq!(OggOpus::open(&later).unwrap().length(), Some(four));
        assert_eq!(played(&later, 0).len() as u64, four);
        // A muxer that writes 0 everywhere: no end is known and all of it
        // plays, padding included, instead of nothing.
        let zeros = regranuled(SAMPLE.to_vec(), |_| 0);
        assert_eq!(OggOpus::open(&zeros).unwrap().length(), None);
        assert!(played(&zeros, 0).len() as u64 > four);
        // An end hours away is refused, as any file longer than RUNE plays.
        let endless = regranuled(SAMPLE.to_vec(), |granule| {
            if granule > four {
                5 * 3_600 * u64::from(PLAYBACK_RATE)
            } else {
                granule
            }
        });
        assert!(read(&endless).is_err());
    }

    #[test]
    fn three_channels_in_two_streams_play_downmixed() {
        let opus = read(SURROUND).unwrap();
        assert_eq!(
            opus.layout,
            Layout::Streams {
                channels: 3,
                streams: 2,
                coupled: 1,
                mapping: vec![0, 2, 1],
            }
        );
        let heard = played(SURROUND, 0);
        assert_eq!(heard.len(), PLAYBACK_RATE as usize);
        // The tone is in one of three channels: a third of its 0.35 RMS.
        let level = crate::rms(&heard[4_800..]);
        assert!(level > 0.08 && level < 0.16, "{level}");
    }

    #[test]
    fn every_page_must_match_its_checksum() {
        assert!(read(SAMPLE).is_ok() && read(&sealed(SAMPLE.to_vec())).is_ok());
        let mut damaged = SAMPLE.to_vec();
        let last = damaged.len() - 10;
        damaged[last] ^= 0x40;
        assert!(read(&damaged).is_err(), "one bit in the last page");
        assert!(read(&sealed(damaged)).is_ok(), "the same bytes, sealed");
    }

    #[test]
    fn broken_or_foreign_files_are_refused() {
        assert!(read(b"").is_err());
        assert!(read(b"RIFF....WAVEfmt ").is_err());
        assert!(read(&SAMPLE[..SAMPLE.len() / 2]).is_err(), "cut short");
        // The first page's body starts at 28; its mapping family is byte 18.
        let mut ambisonics = SAMPLE.to_vec();
        ambisonics[28 + 18] = 2;
        assert!(read(&sealed(ambisonics)).is_err());
        let mut later = SAMPLE.to_vec();
        later[28 + 3] = b'x';
        assert!(read(&sealed(later)).is_err(), "no OpusHead first");
        // A table that names a stream it does not have.
        let mut table = SURROUND.to_vec();
        table[28 + 21] = 7;
        assert!(OggOpus::open(&sealed(table)).is_err());
    }
}
