//! Faster or slower without changing the pitch: WSOLA (Verhelst and
//! Roelands, 1993). Frames of 30 ms are taken from the input a hop apart
//! that follows the speed, each moved by up to 6.7 ms to where it best
//! continues the last one, and overlapped by half under a Hann window.

/// 30 ms at 48 kHz.
const FRAME: usize = 1_440;
const HOP: usize = FRAME / 2;
/// Half the period of a 75 Hz voice: a frame always finds a matching phase.
const SEEK: usize = 320;

pub(crate) struct Stretch {
    window: Vec<f32>,
    /// Input not needed any more is dropped; this holds it from `base` on.
    input: Vec<f32>,
    base: usize,
    /// Where the next frame starts before it is lined up.
    nominal: f64,
    /// Where the last frame would have gone on; `None` before the first.
    natural: Option<usize>,
    /// The last frame's second half, windowed, still to be overlapped.
    tail: Vec<f32>,
    /// Input samples per hop of output.
    hop_in: f64,
}

impl Stretch {
    pub fn new() -> Self {
        Self {
            // Periodic Hann: two halves a hop apart always sum to one.
            window: (0..FRAME)
                .map(|n| 0.5 - 0.5 * (std::f32::consts::TAU * n as f32 / FRAME as f32).cos())
                .collect(),
            input: Vec::new(),
            base: 0,
            nominal: 0.0,
            natural: None,
            tail: vec![0.0; HOP],
            hop_in: HOP as f64,
        }
    }

    /// Add `samples` and append to `out` what is ready, at `speed` times.
    pub fn push(&mut self, samples: &[f32], speed: f32, out: &mut Vec<f32>) {
        self.input.extend_from_slice(samples);
        self.hop_in = HOP as f64 * f64::from(speed);
        loop {
            let nominal = self.nominal.round() as usize;
            let (lowest, highest) = match self.natural {
                Some(_) => (nominal.saturating_sub(SEEK), nominal + SEEK),
                None => (nominal, nominal),
            };
            let needed = (highest + FRAME).max(self.natural.map_or(0, |natural| natural + HOP));
            if needed > self.base + self.input.len() {
                return;
            }
            let start = match self.natural {
                Some(natural) => self.best(lowest, highest, natural),
                None => nominal,
            };
            let frame = &self.input[start - self.base..][..FRAME];
            for index in 0..HOP {
                out.push(self.tail[index] + frame[index] * self.window[index]);
                self.tail[index] = frame[HOP + index] * self.window[HOP + index];
            }
            self.natural = Some(start + HOP);
            self.nominal += self.hop_in;
            let keep = (self.nominal.round() as usize)
                .saturating_sub(SEEK)
                .min(start + HOP);
            if keep > self.base {
                self.input.drain(..keep - self.base);
                self.base = keep;
            }
        }
    }

    /// Append the rest of the input to `out`, faded out, and start over.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        let speed = (self.hop_in / HOP as f64) as f32;
        // Silence after it lets the last frames through.
        self.push(&[0.0; FRAME + 2 * SEEK], speed, out);
        out.extend_from_slice(&self.tail);
        *self = Self::new();
    }

    /// The start between `lowest` and `highest` whose first half best matches
    /// what follows `natural`, by normalized correlation. Every other sample
    /// and start is enough for voice and costs a quarter.
    fn best(&self, lowest: usize, highest: usize, natural: usize) -> usize {
        let target = &self.input[natural - self.base..][..HOP];
        let mut best = (f32::MIN, natural.clamp(lowest, highest));
        for start in (lowest..=highest).step_by(2) {
            let candidate = &self.input[start - self.base..][..HOP];
            let (dot, energy) = candidate
                .iter()
                .zip(target)
                .step_by(2)
                .fold((0.0, 0.0), |(dot, energy), (c, t)| {
                    (dot + c * t, energy + c * c)
                });
            let score = dot / (energy + 1e-9_f32).sqrt();
            if score > best.0 {
                best = (score, start);
            }
        }
        best.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, seconds: f32) -> Vec<f32> {
        (0..(seconds * 48_000.0) as usize)
            .map(|n| 0.5 * (std::f32::consts::TAU * hz * n as f32 / 48_000.0).sin())
            .collect()
    }

    /// Crossings of zero per second, in the middle half, away from the fades.
    fn frequency(pcm: &[f32]) -> f32 {
        let middle = &pcm[pcm.len() / 4..pcm.len() * 3 / 4];
        let crossings = middle
            .windows(2)
            .filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0))
            .count();
        crossings as f32 / 2.0 / (middle.len() as f32 / 48_000.0)
    }

    #[test]
    fn a_tone_keeps_its_pitch_while_its_length_follows_the_speed() {
        let input = sine(220.0, 2.0);
        for speed in [0.75, 1.5, 2.0] {
            let mut stretch = Stretch::new();
            let mut out = Vec::new();
            for chunk in input.chunks(960) {
                stretch.push(chunk, speed, &mut out);
            }
            stretch.finish(&mut out);
            let expected = input.len() as f32 / speed;
            // The silence that lets the last frames through adds a little.
            let extra = (FRAME + 2 * SEEK) as f32 / speed + FRAME as f32;
            assert!(
                out.len() as f32 >= expected - FRAME as f32 && out.len() as f32 <= expected + extra,
                "{speed}×: {} samples for {expected}",
                out.len()
            );
            let heard = frequency(&out);
            assert!((heard - 220.0).abs() < 4.0, "{speed}×: {heard} Hz");
            let loudest = out.iter().fold(0.0_f32, |max, s| max.max(s.abs()));
            assert!(loudest > 0.45 && loudest < 0.55, "{speed}×: {loudest}");
        }
    }
}
