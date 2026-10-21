//! Sums the remote streams of a call into one 20 ms frame for the output.
//!
//! Each source (a person, by the caller's key) has its own [`Jitter`],
//! [`Decoder`] and a fixed buffer of decoded samples for packets longer than
//! a tick. There are at most [`MAX_SOURCES`]; each one's volume or mute is
//! the listener's own choice and stays when the person leaves and comes back.

use std::collections::HashMap;

use crate::codec::{Decoder, FRAME, MAX_PACKET_SAMPLES};
use crate::jitter::{Jitter, JitterStats, Popped};
use crate::MAX_VOLUME;

/// Remote streams mixed at once; Discord's voice channels take up to 99
/// people, so the rest are not heard. shortcut: raise it if a bigger room
/// is ever measured to mix in time.
pub const MAX_SOURCES: usize = 64;
/// Remembered per-person volume choices.
pub const MAX_CHOICES: usize = 256;
/// Marks the key of a stream's sound in the mix, apart from its owner's
/// voice, so it has a volume of its own: ids are below 2⁶³.
pub const STREAM_SOUND: u64 = 1 << 63;
/// A source heard above this RMS (about −40 dBFS) counts as speaking...
const SPEAKING_RMS: f32 = 0.01;
/// ...and for this many ticks after: 300 ms.
const SPEAKING_HOLD: u8 = 15;
/// A source that starts or stops ramps over this many samples (2 ms)
/// instead of jumping, which clicks.
const FADE: usize = 96;
/// Past this level the mix bends smoothly towards 1 instead of being cut
/// flat, which crackles.
const KNEE: f32 = 0.9;

struct Source {
    key: u64,
    jitter: Jitter,
    decoder: Decoder,
    /// Decoded samples not played yet.
    pcm: Box<[f32; MAX_PACKET_SAMPLES]>,
    start: usize,
    end: usize,
    speaking: u8,
    /// The last sample played, while it played last tick.
    last: Option<f32>,
}

impl Source {
    /// One frame of this source, ramped where it starts and stops; `false`
    /// while it has nothing to play.
    fn faded(&mut self, out: &mut [f32; FRAME]) -> bool {
        if self.next(out) {
            if self.last.is_none() {
                for (n, sample) in out.iter_mut().take(FADE).enumerate() {
                    *sample *= (n + 1) as f32 / FADE as f32;
                }
            }
            self.last = Some(out[FRAME - 1]);
            return true;
        }
        match self.last.take() {
            Some(tail) if tail != 0.0 => {
                out.fill(0.0);
                for (n, sample) in out.iter_mut().take(FADE).enumerate() {
                    *sample = tail * (1.0 - (n + 1) as f32 / FADE as f32);
                }
                true
            }
            _ => false,
        }
    }

    /// One frame of this source, or `false` while it has nothing to play.
    /// Packets shorter than a tick are decoded until a frame fills, up to
    /// eight per tick; longer ones leave samples for the next ticks.
    fn next(&mut self, out: &mut [f32; FRAME]) -> bool {
        let mut decoded = [0.0; MAX_PACKET_SAMPLES];
        for _ in 0..8 {
            if self.end - self.start >= FRAME {
                break;
            }
            let count = match self.jitter.pop() {
                Popped::Packet(packet) => match self.decoder.decode(&packet, &mut decoded) {
                    Ok(count) => count,
                    // A corrupt packet plays as a lost one.
                    Err(_) => self.conceal(None, &mut decoded),
                },
                Popped::Lost { next } => self.conceal(next.as_deref(), &mut decoded),
                Popped::Empty => break,
            };
            let remaining = self.end - self.start;
            self.pcm.copy_within(self.start..self.end, 0);
            self.start = 0;
            self.end = remaining;
            let count = count.min(MAX_PACKET_SAMPLES - self.end);
            self.pcm[self.end..self.end + count].copy_from_slice(&decoded[..count]);
            self.end += count;
        }
        if self.end - self.start < FRAME {
            // A partial frame left over is not worth a click: drop it.
            self.start = 0;
            self.end = 0;
            return false;
        }
        out.copy_from_slice(&self.pcm[self.start..self.start + FRAME]);
        self.start += FRAME;
        true
    }

    fn conceal(&mut self, next: Option<&[u8]>, into: &mut [f32; MAX_PACKET_SAMPLES]) -> usize {
        let mut frame = [0.0; FRAME];
        self.decoder.conceal(next, &mut frame);
        into[..FRAME].copy_from_slice(&frame);
        FRAME
    }
}

#[derive(Default)]
pub struct Mixer {
    sources: Vec<Source>,
    /// Volume (0–[`MAX_VOLUME`]) and mute, by key.
    choices: HashMap<u64, (f32, bool)>,
}

impl Mixer {
    /// Queue a packet from `key`. A new source beyond [`MAX_SOURCES`], or one
    /// whose decoder cannot start, is not heard; returns whether it was kept.
    pub fn push(&mut self, key: u64, sequence: u16, packet: Vec<u8>) -> bool {
        let index = match self.sources.iter().position(|source| source.key == key) {
            Some(index) => index,
            None => {
                if self.sources.len() >= MAX_SOURCES {
                    return false;
                }
                let Ok(decoder) = Decoder::new() else {
                    return false;
                };
                self.sources.push(Source {
                    key,
                    jitter: Jitter::default(),
                    decoder,
                    pcm: Box::new([0.0; MAX_PACKET_SAMPLES]),
                    start: 0,
                    end: 0,
                    speaking: 0,
                    last: None,
                });
                self.sources.len() - 1
            }
        };
        self.sources[index].jitter.push(sequence, packet)
    }

    /// How loud `key` plays, from 0 to [`MAX_VOLUME`], and whether the
    /// listener muted them.
    pub fn set_choice(&mut self, key: u64, volume: f32, muted: bool) {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, MAX_VOLUME)
        } else {
            1.0
        };
        if self.choices.len() >= MAX_CHOICES && !self.choices.contains_key(&key) {
            return;
        }
        self.choices.insert(key, (volume, muted));
    }

    /// The volume and mute chosen for `key`.
    #[cfg(test)]
    pub(crate) fn choice(&self, key: u64) -> (f32, bool) {
        self.choices.get(&key).copied().unwrap_or((1.0, false))
    }

    /// Someone left: forget their stream, keep their volume.
    pub fn remove(&mut self, key: u64) {
        self.sources.retain(|source| source.key != key);
    }

    /// Forget every stream, as when the call ends or is deafened.
    pub fn clear(&mut self) {
        self.sources.clear();
    }

    /// Mix one frame into `out`. Returns whether anyone was heard.
    pub fn mix(&mut self, out: &mut [f32; FRAME]) -> bool {
        out.fill(0.0);
        let mut heard = false;
        let mut frame = [0.0; FRAME];
        for source in &mut self.sources {
            if !source.faded(&mut frame) {
                source.speaking = source.speaking.saturating_sub(1);
                continue;
            }
            if crate::rms(&frame) >= SPEAKING_RMS {
                source.speaking = SPEAKING_HOLD;
            } else {
                source.speaking = source.speaking.saturating_sub(1);
            }
            let (volume, muted) = self
                .choices
                .get(&source.key)
                .copied()
                .unwrap_or((1.0, false));
            if muted || volume == 0.0 {
                continue;
            }
            heard = true;
            for (sum, sample) in out.iter_mut().zip(frame) {
                *sum += sample * volume;
            }
        }
        // Several loud people at once must not wrap around: past the knee
        // the sum bends towards 1, never past it.
        for sample in out.iter_mut() {
            *sample = if sample.is_finite() {
                soft_clip(*sample)
            } else {
                0.0
            };
        }
        heard
    }

    /// Who is speaking now, by key, among those with a stream.
    pub fn speaking(&self) -> impl Iterator<Item = (u64, bool)> + '_ {
        self.sources
            .iter()
            .map(|source| (source.key, source.speaking > 0))
    }

    /// Counters summed over every stream.
    pub fn stats(&self) -> JitterStats {
        self.sources
            .iter()
            .fold(JitterStats::default(), |sum, source| JitterStats {
                received: sum.received + source.jitter.stats.received,
                late: sum.late + source.jitter.stats.late,
                lost: sum.lost + source.jitter.stats.lost,
            })
    }
}

/// `sample` as it is up to [`KNEE`], then bent smoothly towards ±1.
fn soft_clip(sample: f32) -> f32 {
    let level = sample.abs();
    if level <= KNEE {
        return sample;
    }
    let room = 1.0 - KNEE;
    (KNEE + room * ((level - KNEE) / room).tanh()).copysign(sample)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::Encoder;

    #[test]
    fn the_mix_bends_past_the_knee_and_never_passes_one() {
        assert_eq!(soft_clip(0.5), 0.5);
        assert_eq!(soft_clip(-KNEE), -KNEE);
        for level in [0.95f32, 1.0, 1.5, 4.0, 100.0] {
            let bent = soft_clip(level);
            // Far past the knee it reaches full scale, never past it.
            assert!(bent > KNEE && bent <= 1.0, "{level}: {bent}");
            assert_eq!(soft_clip(-level), -bent);
        }
        assert!(
            soft_clip(1.0) < 1.0,
            "a sum just past full scale is bent, not flat"
        );
        assert!(soft_clip(1.5) > soft_clip(1.0), "louder stays louder");
    }

    #[test]
    fn a_source_that_stops_ramps_down_instead_of_jumping_to_silence() {
        let mut mixer = Mixer::default();
        let mut encoder = Encoder::new().unwrap();
        let mut out = [0.0; FRAME];
        for frame in 0..6u16 {
            mixer.push(1, frame, voice(&mut encoder, 0.8, frame.into()));
        }
        // Everything it had plays, concealed while it might still come,
        // then it stops: the first silent tick starts from where the sound
        // was and ramps to zero.
        let mut previous = 0.0;
        let mut stopped = false;
        for _ in 0..12 {
            let heard = mixer.mix(&mut out);
            if !heard {
                stopped = true;
                continue;
            }
            assert!(
                (out[0] - previous).abs() < 0.25,
                "no jump between ticks: {previous} -> {}",
                out[0]
            );
            previous = out[FRAME - 1];
        }
        assert!(stopped, "it stops in the end");
        assert!(out.iter().all(|sample| *sample == 0.0));
    }

    fn voice(encoder: &mut Encoder, amplitude: f32, frame: usize) -> Vec<u8> {
        let pcm: [f32; FRAME] = std::array::from_fn(|n| {
            let t = (frame * FRAME + n) as f32 / 48_000.0;
            amplitude * (t * 300.0 * std::f32::consts::TAU).sin()
        });
        encoder.encode(&pcm).unwrap()
    }

    #[test]
    fn two_people_are_summed_clipped_and_follow_their_volume() {
        let mut mixer = Mixer::default();
        let (mut a, mut b) = (Encoder::new().unwrap(), Encoder::new().unwrap());
        let mut out = [0.0; FRAME];
        let mut loudest = 0.0f32;
        for frame in 0..20u16 {
            mixer.push(1, frame, voice(&mut a, 0.9, frame.into()));
            mixer.push(2, frame, voice(&mut b, 0.9, frame.into()));
            if mixer.mix(&mut out) {
                loudest = loudest.max(out.iter().fold(0.0, |max, s| max.max(s.abs())));
            }
        }
        assert!(
            loudest <= 1.0 && loudest > 0.9,
            "summed and clipped: {loudest}"
        );
        assert!(mixer.speaking().all(|(_, speaking)| speaking));
        mixer.set_choice(1, 0.0, false);
        mixer.set_choice(2, 1.0, true);
        for frame in 20..24u16 {
            mixer.push(1, frame, voice(&mut a, 0.9, frame.into()));
            mixer.push(2, frame, voice(&mut b, 0.9, frame.into()));
            assert!(!mixer.mix(&mut out) || out.iter().all(|s| *s == 0.0));
        }
        assert!(
            mixer.speaking().all(|(_, speaking)| speaking),
            "the ring still shows who talks"
        );
        mixer.remove(1);
        mixer.push(1, 0, voice(&mut a, 0.9, 0));
        assert_eq!(
            mixer.choices[&1],
            (0.0, false),
            "a volume outlives a rejoin"
        );
    }

    #[test]
    fn sources_and_choices_are_bounded_and_silence_stops_speaking() {
        let mut mixer = Mixer::default();
        let mut encoder = Encoder::new().unwrap();
        let packet = voice(&mut encoder, 0.5, 0);
        for key in 0..(MAX_SOURCES as u64 + 10) {
            mixer.push(key, 0, packet.clone());
        }
        assert_eq!(mixer.sources.len(), MAX_SOURCES);
        for key in 0..(MAX_CHOICES as u64 + 10) {
            mixer.set_choice(key, f32::NAN, false);
        }
        assert_eq!(mixer.choices.len(), MAX_CHOICES);
        assert_eq!(mixer.choices[&0], (1.0, false));
        let mut out = [0.0; FRAME];
        for _ in 0..40 {
            mixer.mix(&mut out);
        }
        assert!(mixer.speaking().all(|(_, speaking)| !speaking));
        mixer.clear();
        assert!(!mixer.mix(&mut out));
    }
}
