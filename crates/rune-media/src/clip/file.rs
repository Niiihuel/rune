//! A sound file other than a voice message: Symphonia reads its container
//! and decodes it, or hands its Opus packets to libopus; either way it is
//! mixed down to mono and resampled to [`PLAYBACK_RATE`], as everything
//! RUNE plays.

use std::collections::VecDeque;
use std::io::{Cursor, ErrorKind};
use std::sync::Arc;

use rune_i18n::t;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CodecParameters, DecoderOptions, CODEC_TYPE_NULL, CODEC_TYPE_OPUS};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

use super::Sound;
use crate::codec::{Decoder as Opus, MAX_PACKET_SAMPLES};
use crate::playback::Linear;
use crate::{MediaError, PLAYBACK_RATE};

/// Rates a file may use; others are not believed.
const RATES: std::ops::RangeInclusive<u32> = 8_000..=192_000;
/// Channels mixed down; more is not a file RUNE plays.
const MAX_CHANNELS: usize = 8;
/// Undecodable packets in a row before the file is given up.
const MAX_BAD_PACKETS: usize = 64;

fn broken() -> MediaError {
    MediaError(t("RUNE no pudo leer el sonido de este archivo.").into())
}

enum Codec {
    Symphonia(Box<dyn symphonia::core::codecs::Decoder>),
    /// Its packets go to the libopus calls use, which decodes at 48 kHz.
    Opus(Opus),
}

pub(super) struct FileSound {
    bytes: Arc<[u8]>,
    format: Box<dyn FormatReader>,
    params: CodecParameters,
    track: u32,
    /// Its own sample rate, before resampling.
    rate: u32,
    codec: Codec,
    resample: Linear,
    /// Mono samples at its own rate, waiting to be resampled.
    input: VecDeque<f32>,
    /// Samples at its own rate to drop: an encoder's delay, or up to where a
    /// seek asked.
    skip: u64,
    opus_out: Vec<f32>,
}

impl FileSound {
    /// The first track of sound of `bytes`; `None` when there is none, as
    /// in a video without sound.
    pub(super) fn open(bytes: Arc<[u8]>) -> Result<Option<Self>, MediaError> {
        let stream =
            MediaSourceStream::new(Box::new(Cursor::new(bytes.clone())), Default::default());
        let probed = symphonia::default::get_probe().format(
            &Hint::new(),
            stream,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        );
        let format = match probed {
            Ok(probed) => probed.format,
            // An MP4 with pictures alone: Symphonia finds no sound in it.
            Err(_) if bytes.get(4..8) == Some(b"ftyp") => return Ok(None),
            Err(_) => {
                return Err(MediaError(
                    t("RUNE no reconoce el formato de este archivo.").into(),
                ))
            }
        };
        let Some(track) = format
            .tracks()
            .iter()
            .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        else {
            return Ok(None);
        };
        let params = track.codec_params.clone();
        let track = track.id;
        let channels = params.channels.map_or(1, |channels| channels.count());
        if channels == 0 || channels > MAX_CHANNELS {
            return Err(MediaError(
                t("Este archivo tiene más canales de los que RUNE reproduce.").into(),
            ));
        }
        let (codec, rate) = if params.codec == CODEC_TYPE_OPUS {
            (Codec::Opus(Opus::new()?), PLAYBACK_RATE)
        } else {
            let rate = params
                .sample_rate
                .filter(|rate| RATES.contains(rate))
                .ok_or_else(broken)?;
            let decoder = symphonia::default::get_codecs()
                .make(&params, &DecoderOptions::default())
                .map_err(|_| {
                    MediaError(
                        t("Este archivo usa un formato de sonido que RUNE no reproduce.").into(),
                    )
                })?;
            (Codec::Symphonia(decoder), rate)
        };
        let skip = match codec {
            Codec::Opus(_) => u64::from(params.delay.unwrap_or(0)),
            Codec::Symphonia(_) => 0,
        };
        Ok(Some(Self {
            bytes,
            format,
            params,
            track,
            rate,
            codec,
            resample: Linear::new(rate, PLAYBACK_RATE),
            input: VecDeque::new(),
            skip,
            opus_out: vec![0.0; MAX_PACKET_SAMPLES],
        }))
    }

    /// Mono samples of one packet at its own rate; none for a packet that
    /// could not be decoded, which is skipped.
    fn decode(
        &mut self,
        packet: &symphonia::core::formats::Packet,
    ) -> Result<Vec<f32>, MediaError> {
        match &mut self.codec {
            Codec::Opus(opus) => Ok(opus
                .decode(packet.buf(), &mut self.opus_out)
                .map(|length| self.opus_out[..length].to_vec())
                .unwrap_or_default()),
            Codec::Symphonia(decoder) => match decoder.decode(packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let channels = spec.channels.count().max(1);
                    let mut interleaved = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                    interleaved.copy_interleaved_ref(decoded);
                    Ok(interleaved
                        .samples()
                        .chunks_exact(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                        .collect())
                }
                Err(Error::DecodeError(_)) => Ok(Vec::new()),
                Err(_) => Err(broken()),
            },
        }
    }

    fn reset(&mut self) -> Result<(), MediaError> {
        match &mut self.codec {
            Codec::Opus(opus) => *opus = Opus::new()?,
            Codec::Symphonia(decoder) => decoder.reset(),
        }
        self.input.clear();
        self.resample = Linear::new(self.rate, PLAYBACK_RATE);
        Ok(())
    }
}

impl Sound for FileSound {
    fn next(&mut self, out: &mut Vec<f32>) -> Result<bool, MediaError> {
        let mut bad = 0;
        let start = out.len();
        loop {
            let (resample, input) = (&mut self.resample, &mut self.input);
            while let Some(sample) = resample.next(|| input.pop_front()) {
                out.push(sample);
            }
            if out.len() > start {
                return Ok(true);
            }
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(Error::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => {
                    return Ok(false)
                }
                Err(Error::ResetRequired) => {
                    self.reset()?;
                    continue;
                }
                Err(_) => return Err(broken()),
            };
            if packet.track_id() != self.track {
                continue;
            }
            let mono = self.decode(&packet)?;
            if mono.is_empty() {
                bad += 1;
                if bad > MAX_BAD_PACKETS {
                    return Err(broken());
                }
                continue;
            }
            bad = 0;
            let dropped = self.skip.min(mono.len() as u64) as usize;
            self.skip -= dropped as u64;
            self.input.extend(&mono[dropped..]);
        }
    }

    fn seek(&mut self, samples: u64) -> Result<(), MediaError> {
        let seconds = samples as f64 / f64::from(PLAYBACK_RATE);
        let sought = self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: Time::from(seconds),
                track_id: Some(self.track),
            },
        );
        match sought {
            Ok(to) => {
                self.reset()?;
                // The seek lands before what was asked: decode up to it.
                let before = to.required_ts.saturating_sub(to.actual_ts);
                self.skip = match self.params.time_base {
                    Some(base) => {
                        let time = base.calc_time(before);
                        ((time.seconds as f64 + time.frac) * f64::from(self.rate)) as u64
                    }
                    None => before,
                };
            }
            Err(_) => {
                // Read again from the start and drop what comes before.
                *self = Self::open(self.bytes.clone())?.ok_or_else(broken)?;
                self.skip += (seconds * f64::from(self.rate)) as u64;
            }
        }
        Ok(())
    }

    fn length(&self) -> Option<u64> {
        let frames = self.params.n_frames?;
        let seconds = match self.params.time_base {
            Some(base) => {
                let time = base.calc_time(frames);
                time.seconds as f64 + time.frac
            }
            None => frames as f64 / f64::from(self.rate),
        };
        Some((seconds * f64::from(PLAYBACK_RATE)) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Arc<[u8]> {
        let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(path).expect("fixture").into()
    }

    /// Everything it plays from where it is, mono at the playback rate.
    fn drain(sound: &mut FileSound) -> Vec<f32> {
        let mut all = Vec::new();
        while sound.next(&mut all).unwrap() {}
        all
    }

    #[test]
    fn each_common_format_decodes_to_its_length_and_seeks() {
        for name in [
            "tone.mp3",
            "tone.flac",
            "tone.wav",
            "tone.m4a",
            "tone.ogg",
            "tone.webm",
            "clip.mp4",
        ] {
            let mut sound = FileSound::open(fixture(name)).unwrap().expect(name);
            let expected = if name == "clip.mp4" { 2.0 } else { 1.0 };
            let all = drain(&mut sound);
            let seconds = all.len() as f32 / PLAYBACK_RATE as f32;
            // Encoders pad a little; MP3 and AAC most.
            assert!((seconds - expected).abs() < 0.12, "{name}: {seconds} s");
            // The fixtures' tone peaks at 1/8.
            assert!(crate::rms(&all) > 0.05, "{name}: the tone is heard");
            if let Some(length) = sound.length() {
                let said = length as f32 / PLAYBACK_RATE as f32;
                assert!((said - expected).abs() < 0.12, "{name}: says {said} s");
            }
            sound.seek(PLAYBACK_RATE as u64 / 2).unwrap();
            let rest = drain(&mut sound).len() as f32 / PLAYBACK_RATE as f32;
            assert!(
                (rest - (expected - 0.5)).abs() < 0.15,
                "{name}: {rest} s after the middle"
            );
        }
    }

    #[test]
    fn pictures_alone_have_no_sound_and_strangers_are_refused() {
        assert!(FileSound::open(fixture("loop.mp4")).unwrap().is_none());
        assert!(FileSound::open(Arc::from(&b"not a sound file at all"[..])).is_err());
        assert!(FileSound::open(Arc::from(&b""[..])).is_err());
        // A file cut short plays what it has, or fails; it never hangs.
        let wav = fixture("tone.wav");
        if let Ok(Some(mut cut)) = FileSound::open(wav[..60].into()) {
            let mut all = Vec::new();
            while let Ok(true) = cut.next(&mut all) {}
            assert!(all.len() < PLAYBACK_RATE as usize / 10);
        }
    }
}
