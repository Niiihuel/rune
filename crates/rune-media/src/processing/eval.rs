//! What the capture chain does to speech and noise, measured with
//! deterministic synthetic signals: no device, no file, no recording. The
//! speech is a source-filter model (a pulse train through vowel formants,
//! fricatives of shaped noise, syllables, words and pauses); the noises are
//! a fan (rumble and motor hum), a keyboard (clicks in bursts), and white and
//! pink noise.
//!
//! `cargo test --release -p rune-media suppression_table -- --ignored --nocapture`
//! prints the table of docs/PERFORMANCE.md; the other tests hold the default
//! chain to it.

use std::time::{Duration, Instant};

use super::*;
use crate::playback::Linear;
use crate::VoiceActivity;

pub(super) const RATE: usize = 48_000;
/// The call's frames: the transmission gate decides on 20 ms.
const CALL_FRAME: usize = RATE / 50;

/// xorshift32: the same numbers on every machine.
pub(super) struct Rng(u32);

impl Rng {
    pub(super) fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    /// Uniform in [-1, 1).
    fn signed(&mut self) -> f32 {
        (f64::from(self.next()) / f64::from(u32::MAX) * 2.0 - 1.0) as f32
    }

    /// Uniform in [low, high).
    fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * (self.signed() * 0.5 + 0.5)
    }
}

/// A two-pole resonance at `frequency` Hz, `width` Hz wide.
#[derive(Clone, Copy, Default)]
struct Resonator {
    a1: f32,
    a2: f32,
    gain: f32,
    y1: f32,
    y2: f32,
}

impl Resonator {
    fn tune(&mut self, frequency: f32, width: f32) {
        let radius = (-std::f32::consts::PI * width / RATE as f32).exp();
        let angle = std::f32::consts::TAU * frequency / RATE as f32;
        self.a1 = 2.0 * radius * angle.cos();
        self.a2 = -radius * radius;
        // Unity gain at the peak.
        self.gain =
            (1.0 - radius) * (1.0 - 2.0 * radius * (2.0 * angle).cos() + radius * radius).sqrt();
    }

    fn next(&mut self, input: f32) -> f32 {
        let output = self.gain * input + self.a1 * self.y1 + self.a2 * self.y2;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }
}

/// Formants of five vowels, F1 to F4 in Hz, of a low voice.
const VOWELS: [[f32; 4]; 5] = [
    [730.0, 1090.0, 2440.0, 3400.0],
    [530.0, 1840.0, 2480.0, 3500.0],
    [270.0, 2290.0, 3010.0, 3600.0],
    [570.0, 840.0, 2410.0, 3300.0],
    [300.0, 870.0, 2240.0, 3300.0],
];
const WIDTHS: [f32; 4] = [80.0, 100.0, 140.0, 200.0];

/// Synthetic speech and where it is.
pub(super) struct Speech {
    pub samples: Vec<f32>,
    /// Inside a syllable (voiced or not).
    pub speaking: Vec<bool>,
}

/// `seconds` of phrases of two to five words with pauses between them,
/// starting after 0.6 s of room; a low voice, or a high one (`high`). Its
/// RMS while speaking is 0.1 (−20 dBFS).
pub(super) fn speech(seconds: f32, high: bool, seed: u32) -> Speech {
    let length = (seconds * RATE as f32) as usize;
    let mut rng = Rng::new(seed);
    let mut samples = vec![0.0_f32; length];
    let mut speaking = vec![false; length];
    let (pitch, scale) = if high { (205.0, 1.17) } else { (118.0, 1.0) };
    let mut at = RATE * 6 / 10;
    let mut formants = VOWELS[0];
    let mut phase = 0.0_f32;
    let mut resonators = [Resonator::default(); 4];
    let mut hiss = [Resonator::default(); 2];
    hiss[0].tune(4_500.0, 2_000.0);
    hiss[1].tune(6_500.0, 2_500.0);
    let mut glottal = 0.0_f32;
    'phrases: loop {
        let base = pitch * rng.between(0.92, 1.08);
        let words = 2 + (rng.next() % 4) as usize;
        let phrase_start = at;
        for _ in 0..words {
            for _ in 0..1 + rng.next() % 3 {
                // Each sound at its own level: vowels within ±3 dB of the
                // phrase's, which falls 3 dB to its end; fricatives 10 dB under.
                let progress = (at - phrase_start) as f32 / RATE as f32;
                let level = 10_f32.powf((rng.between(-3.0, 3.0) - 1.5 * progress.min(2.0)) / 20.0);
                // A fricative first, often.
                if rng.next() % 10 < 6 {
                    let span = (rng.between(0.04, 0.09) * RATE as f32) as usize;
                    if at + span >= length {
                        break 'phrases;
                    }
                    let mut sound: Vec<f32> = (0..span)
                        .map(|_| {
                            let noise = rng.signed();
                            hiss[0].next(noise) + 0.6 * hiss[1].next(noise)
                        })
                        .collect();
                    place(&mut sound, 0.32 * level, RATE / 200, RATE / 100);
                    for (n, sample) in sound.into_iter().enumerate() {
                        samples[at + n] += sample;
                        speaking[at + n] = true;
                    }
                    at += span;
                }
                let span = (rng.between(0.11, 0.22) * RATE as f32) as usize;
                if at + span >= length {
                    break 'phrases;
                }
                let target = VOWELS[(rng.next() % 5) as usize];
                let from = formants;
                let mut sound = Vec::with_capacity(span);
                for n in 0..span {
                    if n % 48 == 0 {
                        // Glide from the last vowel over 40 ms.
                        let glide = (n as f32 / (RATE as f32 * 0.04)).min(1.0);
                        for (index, resonator) in resonators.iter_mut().enumerate() {
                            let frequency = from[index] + (target[index] - from[index]) * glide;
                            resonator.tune(frequency * scale, WIDTHS[index]);
                        }
                    }
                    // Declining intonation, a slow wobble and a little jitter.
                    let progress = (at + n - phrase_start) as f32 / RATE as f32;
                    let f0 = base
                        * (1.0 - 0.06 * progress).max(0.8)
                        * (1.0 + 0.03 * (progress * 7.0).sin())
                        * (1.0 + 0.004 * rng.signed());
                    let step = f0 / RATE as f32;
                    phase += step;
                    if phase >= 1.0 {
                        phase -= 1.0;
                    }
                    // A band-limited sawtooth (PolyBLEP), softened.
                    let mut saw = 2.0 * phase - 1.0;
                    saw -= blep(phase, step);
                    glottal += 0.25 * (saw - glottal);
                    let mut voice = glottal + 0.02 * rng.signed();
                    for resonator in &mut resonators {
                        voice = resonator.next(voice);
                    }
                    sound.push(voice);
                }
                place(&mut sound, level, RATE * 15 / 1000, RATE * 30 / 1000);
                for (n, sample) in sound.into_iter().enumerate() {
                    samples[at + n] += sample;
                    speaking[at + n] = true;
                }
                formants = target;
                at += span + (rng.between(0.0, 0.02) * RATE as f32) as usize;
            }
            at += (rng.between(0.04, 0.14) * RATE as f32) as usize;
        }
        at += (rng.between(0.7, 1.4) * RATE as f32) as usize;
        if at >= length {
            break;
        }
    }
    let spoken: Vec<f32> = samples
        .iter()
        .zip(&speaking)
        .filter(|(_, speaking)| **speaking)
        .map(|(sample, _)| *sample)
        .collect();
    let gain = 0.1 / f64::from(rms(&spoken)).max(1e-9);
    for sample in &mut samples {
        *sample = (f64::from(*sample) * gain) as f32;
    }
    Speech { samples, speaking }
}

/// PolyBLEP: the step of a sawtooth, smoothed over one sample each side.
fn blep(phase: f32, step: f32) -> f32 {
    if phase < step {
        let t = phase / step;
        t + t - t * t - 1.0
    } else if phase > 1.0 - step {
        let t = (phase - 1.0) / step;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// `sound` at RMS `level` with its attack and release.
fn place(sound: &mut [f32], level: f32, attack: usize, release: usize) {
    let gain = level / rms(sound).max(1e-9);
    let span = sound.len();
    for (n, sample) in sound.iter_mut().enumerate() {
        *sample *= gain * envelope(n, span, attack, release);
    }
}

/// A raised-cosine attack and release over a segment.
fn envelope(n: usize, span: usize, attack: usize, release: usize) -> f32 {
    let rise =
        |k: usize, of: usize| 0.5 - 0.5 * (std::f32::consts::PI * k as f32 / of as f32).cos();
    if n < attack {
        rise(n, attack)
    } else if span - n < release {
        rise(span - n, release)
    } else {
        1.0
    }
}

/// The noises measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Noise {
    /// A laptop fan: rumble, a motor's hum and a little hiss, −42 dBFS.
    Fan,
    /// Typing in bursts: clicks of keys going down and up, peaks near
    /// −10 dBFS.
    Keyboard,
    /// −45 dBFS.
    White,
    /// −40 dBFS.
    Pink,
}

impl Noise {
    pub(super) const ALL: [Self; 4] = [Self::Fan, Self::Keyboard, Self::White, Self::Pink];

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Fan => "ventilador",
            Self::Keyboard => "teclado",
            Self::White => "ruido blanco",
            Self::Pink => "ruido rosa",
        }
    }
}

/// `length` samples of `kind`.
pub(super) fn noise(kind: Noise, length: usize, seed: u32) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let db = |db: f64| 10_f64.powf(db / 20.0);
    let scaled = |samples: Vec<f32>, level: f64| {
        let gain = level / f64::from(rms(&samples)).max(1e-12);
        samples
            .into_iter()
            .map(|sample| (f64::from(sample) * gain) as f32)
            .collect()
    };
    match kind {
        Noise::White => scaled((0..length).map(|_| rng.signed()).collect(), db(-45.0)),
        Noise::Pink => scaled(pink(&mut rng, length), db(-40.0)),
        Noise::Fan => {
            // Air through the blades: broad noise with a hump around 600 Hz,
            // the blades' tone at 230 Hz and its harmonics, slowly beating.
            let pinked = pink(&mut rng, length);
            let mut hump = Resonator::default();
            hump.tune(600.0, 1_200.0);
            let samples = (0..length)
                .map(|n| {
                    let t = n as f32 / RATE as f32;
                    let beat = 1.0 + 0.1 * (std::f32::consts::TAU * 0.4 * t).sin();
                    let tone: f32 = [(1.0, 1.0), (2.0, 0.4), (3.0, 0.2)]
                        .iter()
                        .map(|(harmonic, level)| {
                            level * (std::f32::consts::TAU * 230.0 * harmonic * t).sin()
                        })
                        .sum();
                    beat * (70.0 * pinked[n] + 2.0 * hump.next(rng.signed()) + 0.25 * tone)
                })
                .collect();
            scaled(samples, db(-42.0))
        }
        Noise::Keyboard => {
            let mut samples = vec![0.0_f32; length];
            let mut tone = [Resonator::default(); 2];
            tone[0].tune(3_200.0, 2_500.0);
            tone[1].tune(1_300.0, 900.0);
            let mut at = RATE / 5;
            while at < length {
                // A burst of typing, then a pause.
                let burst = at + (rng.between(1.5, 4.0) * RATE as f32) as usize;
                while at < burst.min(length) {
                    let peak = rng.between(0.12, 0.35);
                    for (offset, level) in [
                        (0, 1.0),
                        ((rng.between(0.06, 0.12) * RATE as f32) as usize, 0.5),
                    ] {
                        for n in 0..RATE * 8 / 1000 {
                            let Some(sample) = samples.get_mut(at + offset + n) else {
                                break;
                            };
                            let decay = (-(n as f32) / (RATE as f32 * 0.0012)).exp();
                            let click = rng.signed() * decay;
                            *sample +=
                                peak * level * (tone[0].next(click) + 0.5 * tone[1].next(click));
                        }
                    }
                    at += (rng.between(0.08, 0.25) * RATE as f32) as usize;
                }
                at += (rng.between(1.0, 3.0) * RATE as f32) as usize;
            }
            samples
        }
    }
}

/// Paul Kellet's pink filter over white noise, about 0.01 RMS.
fn pink(rng: &mut Rng, length: usize) -> Vec<f32> {
    let mut b = [0.0_f32; 7];
    (0..length)
        .map(|_| {
            let white = rng.signed() * 0.05;
            b[0] = 0.99886 * b[0] + white * 0.0555179;
            b[1] = 0.99332 * b[1] + white * 0.0750759;
            b[2] = 0.96900 * b[2] + white * 0.153_852;
            b[3] = 0.86650 * b[3] + white * 0.3104856;
            b[4] = 0.55000 * b[4] + white * 0.5329522;
            b[5] = -0.7616 * b[5] - white * 0.0168980;
            let pink = b[..6].iter().sum::<f32>() + b[6] + white * 0.5362;
            b[6] = white * 0.115926;
            pink * 0.11
        })
        .collect()
}

/// What a chain did, against the clean speech.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Scores {
    /// Output against input in the pauses (from 0.5 s after speech to
    /// 50 ms before the next), in dB: negative is noise taken away.
    pub pauses_db: f64,
    /// The output's level while speaking, against the clean speech's, dB.
    pub voice_db: f64,
    /// Signal to distortion: the clean speech against everything else in
    /// the output while speaking (residual noise and damage), in dB,
    /// segmental (see [`score`]).
    pub sdr_db: f64,
    /// The same of the input: the noise's own.
    pub input_sdr_db: f64,
    /// 20 ms stretches of the pauses with anything in them: the
    /// microphone open on noise, once the call's gate decides.
    pub open: f64,
    /// Syllable stretches (20 ms) with nothing in them: speech cut.
    pub cut: f64,
}

/// Score `output`, `delay` samples late, against `clean` under `input`.
pub(super) fn score(clean: &Speech, input: &[f32], output: &[f32], delay: usize) -> Scores {
    let length = clean.samples.len().min(output.len().saturating_sub(delay));
    let late = |at: usize| output[at + delay];
    // Pauses: far enough from speech on both sides.
    let mut since = usize::MAX / 2;
    let mut pause: Vec<bool> = clean.speaking[..length]
        .iter()
        .map(|speaking| {
            since = if *speaking { 0 } else { since + 1 };
            since >= RATE / 2
        })
        .collect();
    let mut until = usize::MAX / 2;
    for (pause, speaking) in pause.iter_mut().zip(&clean.speaking[..length]).rev() {
        until = if *speaking { 0 } else { until + 1 };
        *pause &= until >= RATE / 20;
    }
    let energy = |pick: &dyn Fn(usize) -> f64, mask: &dyn Fn(usize) -> bool| {
        (0..length)
            .filter(|at| mask(*at))
            .map(|at| pick(at).powi(2))
            .sum::<f64>()
    };
    let decibels = |ratio: f64| 10.0 * ratio.max(1e-12).log10();
    let paused = |at: usize| pause[at];
    let speaking = |at: usize| clean.speaking[at];
    let pauses_db = decibels(
        energy(&|at| f64::from(late(at)), &paused) / energy(&|at| f64::from(input[at]), &paused),
    );
    let voice_db = decibels(
        energy(&|at| f64::from(late(at)), &speaking)
            / energy(&|at| f64::from(clean.samples[at]), &speaking),
    );
    // Segmental, over 100 ms of speech each: a gain that moves slowly (the
    // automatic gain) is not distortion; each segment is clamped to
    // −10…40 dB so a silent one does not dominate.
    let sdr = |signal: &dyn Fn(usize) -> f64| {
        let segment = RATE / 10;
        let mut scores = Vec::new();
        for start in (0..length.saturating_sub(segment)).step_by(segment) {
            let range = start..start + segment;
            if range.clone().filter(|at| speaking(*at)).count() < segment * 8 / 10 {
                continue;
            }
            let (mut cross, mut own) = (0.0, 0.0);
            for at in range.clone() {
                let reference = f64::from(clean.samples[at]);
                cross += signal(at) * reference;
                own += reference * reference;
            }
            let scale = cross / own.max(1e-12);
            let error: f64 = range
                .map(|at| (signal(at) - scale * f64::from(clean.samples[at])).powi(2))
                .sum();
            scores.push(decibels(scale * scale * own / error.max(1e-12)).clamp(-10.0, 40.0));
        }
        scores.iter().sum::<f64>() / scores.len().max(1) as f64
    };
    let stretches = |mask: &dyn Fn(usize) -> bool, empty: bool| {
        let (mut counted, mut found) = (0, 0);
        for start in (0..length.saturating_sub(CALL_FRAME)).step_by(CALL_FRAME) {
            if !(start..start + CALL_FRAME).all(mask) {
                continue;
            }
            counted += 1;
            let silent = (start..start + CALL_FRAME).all(|at| late(at) == 0.0);
            found += usize::from(silent == empty);
        }
        found as f64 / counted.max(1) as f64
    };
    Scores {
        pauses_db,
        voice_db,
        sdr_db: sdr(&|at| f64::from(late(at))),
        input_sdr_db: sdr(&|at| f64::from(input[at])),
        open: stretches(&paused, false),
        cut: stretches(&speaking, true),
    }
}

/// The delay of `output` behind `clean`, up to 60 ms: where they correlate
/// best over the first phrase.
pub(super) fn delay(clean: &[f32], output: &[f32]) -> usize {
    let window = RATE * 6 / 10..RATE * 21 / 10;
    let correlation = |lag: usize| {
        window
            .clone()
            .map(|at| {
                f64::from(clean[at]) * f64::from(output.get(at + lag).copied().unwrap_or(0.0))
            })
            .sum::<f64>()
    };
    (0..RATE * 6 / 100)
        .max_by(|a, b| correlation(*a).total_cmp(&correlation(*b)))
        .unwrap_or(0)
}

/// The call's transmission gate, as the host sets it by default (voice
/// activity, automatic sensitivity), over 20 ms frames: what is not sent is
/// silence for the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Gate {
    /// The processed signal itself, as «Escucharme» plays it.
    Off,
    /// Opens on a frame's level.
    Level,
    /// Opens on its sustained level: a click does not open it.
    Sustained,
    /// After the speech gate: anything it let through.
    Gated,
}

/// One way of processing the microphone, as the capture worker and the
/// call run it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Chain {
    pub name: &'static str,
    /// The microphone's rate; the chain resamples as RUNE's does.
    pub rate: u32,
    /// Resample to 48 kHz before processing, so RNNoise always runs.
    pub resample_first: bool,
    pub mode: ProcessingMode,
    pub automatic_gain: bool,
    /// The unprocessed microphone mixed back this many dB under, aligned.
    pub dry_db: Option<f32>,
    /// The processor's speech gate, with the call's gate set after it.
    pub speech_gate: bool,
    pub gate: Gate,
}

impl Chain {
    pub(super) const fn new(name: &'static str, mode: ProcessingMode) -> Self {
        Self {
            name,
            rate: RATE as u32,
            resample_first: false,
            mode,
            automatic_gain: false,
            dry_db: None,
            speech_gate: false,
            gate: Gate::Level,
        }
    }
}

/// What `chain` makes of `input` (at [`RATE`]), back at [`RATE`], and the
/// processing time of each 10 ms frame.
pub(super) fn run(chain: Chain, input: &[f32]) -> (Vec<f32>, Vec<Duration>) {
    // The device's rate: what the microphone would have given.
    let at_device = resample(input, RATE as u32, chain.rate);
    let rate = if chain.resample_first {
        RATE as u32
    } else {
        chain.rate
    };
    let captured = resample(&at_device, chain.rate, rate);
    let mut processor = CaptureProcessor::new(rate, chain.mode).unwrap();
    processor.set_automatic_gain(chain.automatic_gain);
    processor.set_speech_gate(chain.speech_gate);
    let mut processed = Vec::with_capacity(captured.len());
    let mut times = Vec::new();
    let mut frame = vec![0.0; processor.frame_len()];
    let size = processor.frame_len();
    for (index, chunk) in captured.chunks_exact(size).enumerate() {
        frame.copy_from_slice(chunk);
        let started = Instant::now();
        processor.process(&mut frame).unwrap();
        if let Some(db) = chain.dry_db {
            // RNNoise is one frame late: the dry signal waits as long.
            let gain = 10_f32.powf(-db / 20.0);
            if let Some(at) = index.checked_sub(1) {
                for (sample, dry) in frame.iter_mut().zip(&captured[at * size..at * size + size]) {
                    *sample += gain * dry;
                }
            }
        }
        times.push(started.elapsed());
        processed.extend_from_slice(&frame);
    }
    let mut output = resample(&processed, rate, RATE as u32);
    gate(&mut output, chain.gate);
    (output, times)
}

fn gate(samples: &mut [f32], gate: Gate) {
    if gate == Gate::Off {
        return;
    }
    let start = Instant::now();
    let mut voice = match gate {
        Gate::Gated => VoiceActivity {
            on: VoiceActivity::GATED_ON,
            off: VoiceActivity::GATED_OFF,
            ..VoiceActivity::default()
        },
        _ => VoiceActivity {
            automatic: true,
            ..VoiceActivity::default()
        },
    };
    let frames = samples.len().div_ceil(CALL_FRAME);
    let mut sent = vec![false; frames];
    for (index, frame) in samples.chunks(CALL_FRAME).enumerate() {
        let now = start + Duration::from_millis(20 * index as u64);
        let level = meter_position(dbfs(rms(frame)));
        sent[index] = match gate {
            Gate::Level => voice.update(level, now),
            _ => voice.update_sustained(level, crate::sustained_level(frame), now),
        };
    }
    for (index, frame) in samples.chunks_mut(CALL_FRAME).enumerate() {
        if !sent[index] {
            frame.fill(0.0);
        }
    }
}

/// `input` from `from` Hz to `to` Hz as RUNE's streaming interpolator does.
fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to {
        return input.to_vec();
    }
    let mut linear = Linear::new(from, to);
    let mut samples = input.iter().copied();
    let mut output = Vec::with_capacity(input.len() * to as usize / from as usize + 2);
    while let Some(sample) = linear.next(|| samples.next()) {
        output.push(sample);
    }
    output
}

/// Clean speech with `kind` under it, mixed.
pub(super) fn noisy(clean: &Speech, kind: Noise, seed: u32) -> Vec<f32> {
    noise(kind, clean.samples.len(), seed)
        .iter()
        .zip(&clean.samples)
        .map(|(noise, speech)| noise + speech)
        .collect()
}

/// The chains of the table: before and after the work of 2026-10-17.
const BEFORE: Chain = Chain {
    automatic_gain: true,
    ..Chain::new("Antes: Aislamiento de voz, 48 kHz", ProcessingMode::RnNoise)
};
const AFTER: Chain = Chain {
    automatic_gain: true,
    speech_gate: true,
    gate: Gate::Gated,
    ..Chain::new("Ahora: Aislamiento de voz, 48 kHz", ProcessingMode::RnNoise)
};

/// The echo of `far` (someone in the call, played) under `near` at the
/// microphone, 10 ms later at half its level with a little room after:
/// what is left of it, in dB, once the default chain cancels it.
fn echo_left(far: &[f32]) -> f64 {
    let mut processor = CaptureProcessor::new(RATE as u32, ProcessingMode::RnNoise).unwrap();
    processor.set_automatic_gain(true);
    processor.set_speech_gate(true);
    processor.set_echo_cancellation(true);
    let late = |at: usize, by: usize| at.checked_sub(by).map_or(0.0, |at| far[at]);
    let heard: Vec<f32> = (0..far.len())
        .map(|at| 0.5 * late(at, 480) + 0.15 * late(at, 1_900) + 0.05 * late(at, 4_100))
        .collect();
    let (mut before, mut after) = (0.0_f64, 0.0_f64);
    let mut frame = vec![0.0; FRAME];
    for (index, (played, mic)) in far
        .chunks_exact(FRAME)
        .zip(heard.chunks_exact(FRAME))
        .enumerate()
    {
        processor.render(played, 0).unwrap();
        frame.copy_from_slice(mic);
        processor.process(&mut frame).unwrap();
        // After 10 s to adapt, as a call's first seconds.
        if index >= 1_000 {
            before += mic.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
            after += frame.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
        }
    }
    10.0 * (after.max(1e-12) / before.max(1e-12)).log10()
}

const FRAME: usize = RATE / 100;

#[test]
#[ignore]
fn suppression_table() {
    let clean = speech(30.0, false, 7);
    let high = speech(30.0, true, 11);
    let chains = [
        Chain::new("Ninguna (Estudio), antes", ProcessingMode::Off),
        BEFORE,
        Chain {
            rate: 44_100,
            ..Chain {
                name: "Antes: Aislamiento de voz, micrófono a 44,1 kHz (WebRTC)",
                ..BEFORE
            }
        },
        AFTER,
        Chain {
            name: "Ahora: Aislamiento de voz, micrófono a 44,1 kHz (remuestreado, RNNoise)",
            rate: 44_100,
            resample_first: true,
            ..AFTER
        },
        Chain {
            gate: Gate::Sustained,
            ..Chain::new("Ninguna (Estudio), ahora", ProcessingMode::Off)
        },
        Chain {
            gate: Gate::Off,
            ..Chain::new(
                "RNNoise solo, sin ninguna compuerta",
                ProcessingMode::RnNoise,
            )
        },
        Chain {
            name: "Ahora: lo que se oye en «Escucharme» (sin la compuerta de la llamada)",
            gate: Gate::Off,
            ..AFTER
        },
    ];
    println!(
        "eco que queda con la cadena por defecto (AEC3 + RNNoise + compuerta): {:.1} dB",
        echo_left(&speech(20.0, true, 13).samples)
    );
    for chain in chains {
        println!("\n{}", chain.name);
        for (voice, label) in [(&clean, "grave"), (&high, "aguda")] {
            let (dry, times) = run(chain, &voice.samples);
            let lag = delay(&voice.samples, &dry);
            let dry = score(voice, &voice.samples, &dry, lag);
            let mut sorted = times.clone();
            sorted.sort_unstable();
            println!(
                "  voz {label} sola: voz {:+.1} dB, SDR {:.1} dB, cortes {:.1} %, demora {:.1} ms, CPU p50 {:?} p95 {:?}",
                dry.voice_db,
                dry.sdr_db,
                dry.cut * 100.0,
                lag as f64 / 48.0,
                sorted[sorted.len() / 2],
                sorted[sorted.len() * 95 / 100],
            );
            for kind in Noise::ALL {
                let input = noisy(voice, kind, 3 + kind as u32);
                let (output, _) = run(chain, &input);
                let scores = score(voice, &input, &output, lag);
                println!(
                    "  voz {label} + {:<12} pausas {:+6.1} dB, abierto {:5.1} %, voz {:+5.1} dB, SDR {:5.1} dB (entrada {:5.1}), cortes {:4.1} %",
                    kind.name(),
                    scores.pauses_db,
                    scores.open * 100.0,
                    scores.voice_db,
                    scores.sdr_db,
                    scores.input_sdr_db,
                    scores.cut * 100.0,
                );
            }
        }
    }
}

#[test]
fn the_default_chain_keeps_typing_and_the_room_out_and_the_voice_in() {
    let voice = speech(12.0, false, 7);
    let typing = noisy(&voice, Noise::Keyboard, 4);
    let fan = noisy(&voice, Noise::Fan, 3);
    let (before, _) = run(BEFORE, &typing);
    let (after, _) = run(AFTER, &typing);
    let lag = delay(&voice.samples, &run(AFTER, &voice.samples).0);
    let (before, after) = (
        score(&voice, &typing, &before, lag),
        score(&voice, &typing, &after, lag),
    );
    assert!(
        after.open <= 0.05 && after.open * 4.0 < before.open,
        "typing opens it {:.1} % (before {:.1} %)",
        after.open * 100.0,
        before.open * 100.0
    );
    let (output, _) = run(AFTER, &fan);
    let room = score(&voice, &fan, &output, lag);
    assert!(room.pauses_db < -60.0, "{room:?}");
    assert!(room.voice_db.abs() < 1.5 && room.cut < 0.03, "{room:?}");
    assert!(echo_left(&speech(12.0, true, 13).samples) < -20.0);
}
