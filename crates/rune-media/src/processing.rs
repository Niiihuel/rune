//! Local, mono capture processing. The caller owns framing and scheduling.
//!
//! This module never opens a device, records audio, or connects to a provider.
//! Echo cancellation runs only while the caller turns it on and feeds what the
//! output played, through [`CaptureProcessor::render`].
//! Construct, reconfigure, reset and process on the capture worker, not in an
//! audio callback or on the UI thread. The only retained audio is bounded DSP
//! history; the reusable buffers are cleared after each processed frame.
//!
//! The chain, each stage only when chosen: input volume, then Sonora's
//! high-pass filter, WebRTC noise suppression and echo cancellation, then
//! RNNoise, then automatic gain, then the speech gate. RNNoise comes after
//! the canceller, which needs the microphone linear in what the output
//! played, and before the gain, so the gain is not set by noise RNNoise
//! removes. The gate goes last, on what would be sent: it silences what
//! RNNoise does not hear as speech (a key, a breath, the room), so it is
//! only on with RNNoise; see [`CaptureProcessor::set_speech_gate`].

use std::time::Instant;

use nnnoiseless::DenoiseState;
use rune_i18n::t;
use sonora::config::{
    AdaptiveDigital, EchoCanceller, GainController2, HighPassFilter, MaxProcessingRate,
    NoiseSuppression, NoiseSuppressionLevel,
};
use sonora::{AudioProcessing, Config, StreamConfig};

use crate::{dbfs, meter_position, rms, MediaError, PLAYBACK_RATE};

/// Samples of echo reference per call to [`CaptureProcessor::render`]: 10 ms
/// of mono at [`PLAYBACK_RATE`].
pub const REFERENCE_FRAME: usize = PLAYBACK_RATE as usize / 100;

/// The only rate RNNoise works at; its frames are 10 ms there.
pub const RNNOISE_RATE: u32 = 48_000;

/// The loudest input volume: twice what the microphone gives, as Discord's.
pub const MAX_INPUT_VOLUME: f32 = 2.0;

/// RNNoise's speech probability that opens the speech gate…
const GATE_OPEN: f32 = 0.6;
/// …and that keeps it open: between the two it stays as it was.
const GATE_KEEP: f32 = 0.5;
/// Frames under [`GATE_KEEP`] before it closes: 300 ms, so the pauses
/// between words go through.
const GATE_HOLD: u32 = 30;
/// Samples (at 48 kHz) its gain takes to open, 5 ms, and to close, 20 ms:
/// no click either way.
const GATE_ATTACK: f32 = 240.0;
const GATE_RELEASE: f32 = 960.0;

/// Noise suppression of local capture samples, independent of voice transport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcessingMode {
    /// Preserve finite, normalized samples without running a denoiser.
    #[default]
    Off,
    /// WebRTC's suppressor (Sonora): high-pass filter and moderate
    /// suppression of stationary noise.
    WebRtc,
    /// RNNoise (nnnoiseless), a small neural network that also takes
    /// keyboard and other non-stationary noise. It runs at [`RNNOISE_RATE`]
    /// only; a microphone at another rate gets [`Self::WebRtc`] instead.
    RnNoise,
}

/// Levels use RUNE's logarithmic meter scale (0 = floor, 1 = 0 dBFS).
/// Timing covers this frame's sanitization, DSP and level calculation only.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProcessingMetrics {
    /// After the input volume, before any processing.
    pub raw_level: f32,
    pub processed_level: f32,
    pub elapsed_micros: u64,
    /// RNNoise's probability that this frame is speech, when it ran.
    pub speech: Option<f32>,
}

/// Processes exactly 10 ms of normalized, mono audio in place.
///
/// Rates from 8 through 192 kHz divisible by 100 are accepted. Sonora handles
/// any conversion to its internal processing rate and returns the original
/// rate. There is no recording, playback or network.
pub struct CaptureProcessor {
    sample_rate: u32,
    mode: ProcessingMode,
    echo: bool,
    gain: bool,
    volume: f32,
    engine: Option<AudioProcessing>,
    denoiser: Option<Box<DenoiseState<'static>>>,
    leveler: Option<AudioProcessing>,
    gate: Option<SpeechGate>,
    output: Vec<f32>,
    rendered: Vec<f32>,
}

/// Silence unless RNNoise hears speech: open from [`GATE_OPEN`], kept
/// while over [`GATE_KEEP`] and for [`GATE_HOLD`] frames after, with gain
/// ramps instead of steps.
// shortcut: no look-ahead; 10 ms of it cut speech lost at onsets only from
// 1.7 to 1.4 % (the fricative before a phrase's first vowel), for 10 ms
// more delay. Revisit if real voices lose more.
#[derive(Clone, Copy, Debug, Default)]
struct SpeechGate {
    open: bool,
    quiet: u32,
    gain: f32,
    /// The frame before was speech: two in a row open it.
    heard: bool,
}

/// How far, in meter positions (12 dB), a frame's second quietest quarter
/// may fall under its whole level and still be steady: speech fills the
/// four quarters of 10 ms; a key's click, one.
const IMPULSIVE: f32 = 0.2;

impl SpeechGate {
    /// `steady`: the microphone's frame, before processing, is not one
    /// click. RNNoise sometimes takes typing for speech, never for long and
    /// never steady.
    fn apply(&mut self, frame: &mut [f32], speech: f32, steady: bool) {
        let heard = std::mem::replace(&mut self.heard, steady && speech >= GATE_OPEN);
        if self.heard && (heard || self.open) {
            (self.open, self.quiet) = (true, 0);
        } else if steady && speech >= GATE_KEEP {
            self.quiet = 0;
        } else if self.open {
            self.quiet += 1;
            self.open = self.quiet < GATE_HOLD;
        }
        let step = if self.open {
            1.0 / GATE_ATTACK
        } else {
            -1.0 / GATE_RELEASE
        };
        for sample in frame {
            self.gain = (self.gain + step).clamp(0.0, 1.0);
            *sample *= self.gain;
        }
    }
}

impl CaptureProcessor {
    pub fn new(sample_rate: u32, mode: ProcessingMode) -> Result<Self, MediaError> {
        if !(8_000..=192_000).contains(&sample_rate) || !sample_rate.is_multiple_of(100) {
            return Err(MediaError(
                t("el procesamiento admite frecuencias de 8 a 192 kHz divisibles por 100").into(),
            ));
        }
        Ok(Self {
            sample_rate,
            mode,
            echo: false,
            gain: false,
            volume: 1.0,
            engine: engine(sample_rate, mode, false),
            denoiser: denoiser(sample_rate, mode),
            leveler: None,
            gate: None,
            output: vec![0.0; (sample_rate / 100) as usize],
            rendered: vec![0.0; REFERENCE_FRAME],
        })
    }

    /// Exact number of mono samples required by each call to [`Self::process`].
    pub fn frame_len(&self) -> usize {
        self.output.len()
    }

    /// Changes the processing mode and discards prior DSP history. Setting the
    /// current mode is a no-op. This may allocate; run it on the worker.
    pub fn set_mode(&mut self, mode: ProcessingMode) {
        if self.mode != mode {
            self.mode = mode;
            self.reset();
        }
    }

    /// Whether RNNoise runs: chosen, and the microphone is at [`RNNOISE_RATE`].
    pub fn rnnoise(&self) -> bool {
        self.denoiser.is_some()
    }

    /// Turns echo cancellation on or off, discarding the canceller's
    /// history. Turn it on only while a played reference reaches
    /// [`Self::render`]. RNNoise, the gain and the gate keep theirs: the
    /// output starting or stopping must not start the voice over. This may
    /// allocate; run it on the worker.
    pub fn set_echo_cancellation(&mut self, on: bool) {
        if self.echo != on {
            self.echo = on;
            self.engine = engine(self.sample_rate, self.mode, on);
        }
    }

    /// Turns the speech gate on or off: what RNNoise does not hear as speech
    /// is silenced, with ramps. For voice activity with automatic
    /// sensitivity, where it decides what goes out instead of a level; it
    /// acts only while RNNoise runs.
    pub fn set_speech_gate(&mut self, on: bool) {
        if self.gate.is_some() != on {
            self.gate = on.then(SpeechGate::default);
        }
    }

    pub fn echo_cancellation(&self) -> bool {
        self.echo
    }

    /// Turns automatic gain on or off: a digital gain that brings speech to
    /// a steady level. It never changes the system's microphone volume.
    /// Only the gain's own history is discarded. This may allocate.
    pub fn set_automatic_gain(&mut self, on: bool) {
        if self.gain != on {
            self.gain = on;
            self.leveler = leveler(self.sample_rate, on);
        }
    }

    /// The input volume, a linear gain applied before anything else, from 0
    /// to [`MAX_INPUT_VOLUME`]; nonsense is 1. It keeps all DSP history.
    pub fn set_input_volume(&mut self, volume: f32) {
        self.volume = if volume.is_finite() {
            volume.clamp(0.0, MAX_INPUT_VOLUME)
        } else {
            1.0
        };
    }

    /// Analyze 10 ms of what the output played (mono, [`PLAYBACK_RATE`])
    /// before the capture frame it may have leaked into. `delay_ms` is the
    /// output's latency, a hint for the canceller's own delay estimate.
    pub fn render(&mut self, reference: &[f32], delay_ms: u32) -> Result<(), MediaError> {
        if reference.len() != REFERENCE_FRAME {
            return Err(MediaError(
                t("la referencia de eco necesita exactamente 10 ms").into(),
            ));
        }
        let Some(engine) = self.engine.as_mut().filter(|_| self.echo) else {
            return Err(MediaError(t("la cancelación de eco está apagada").into()));
        };
        // Out-of-range delays are clamped by the canceller, which is enough.
        let _ = engine.set_stream_delay_ms(delay_ms.min(500) as i32);
        let result = engine.process_render_f32(&[reference], &mut [self.rendered.as_mut_slice()]);
        self.rendered.fill(0.0);
        result.map_err(|_| MediaError(t("falló el análisis de la referencia de eco").into()))
    }

    /// Discards adaptive/filter history, e.g. after a gap or a mute transition.
    /// This may allocate; run it on the worker before consuming new capture.
    pub fn reset(&mut self) {
        self.engine = engine(self.sample_rate, self.mode, self.echo);
        self.denoiser = denoiser(self.sample_rate, self.mode);
        self.leveler = leveler(self.sample_rate, self.gain);
        if let Some(gate) = self.gate.as_mut() {
            *gate = SpeechGate::default();
        }
        self.output.fill(0.0);
    }

    /// Sanitizes and processes one frame without allocating.
    ///
    /// Non-finite samples become silence and samples outside [-1, 1] are clamped
    /// in every mode. Incorrect frame lengths return an error without changing
    /// the frame or the DSP state. A DSP error silences the frame and clears the
    /// scratch buffer; callers should report it and stop or reset the worker.
    pub fn process(&mut self, frame: &mut [f32]) -> Result<ProcessingMetrics, MediaError> {
        if frame.len() != self.frame_len() {
            return Err(MediaError(
                t("el procesamiento necesita un cuadro mono de exactamente 10 ms").into(),
            ));
        }
        let started = Instant::now();
        sanitize(frame);
        if self.volume != 1.0 {
            for sample in frame.iter_mut() {
                *sample = (*sample * self.volume).clamp(-1.0, 1.0);
            }
        }
        let raw_level = level(frame);
        let steady = crate::sustained_level(frame) >= raw_level - IMPULSIVE;
        if let Some(engine) = self.engine.as_mut() {
            run(engine, frame, &mut self.output)?;
        }
        let mut speech = None;
        if let Some(denoiser) = self.denoiser.as_mut() {
            // RNNoise reads the 16-bit PCM scale, from a buffer of its own.
            for (pcm, sample) in self.output.iter_mut().zip(frame.iter()) {
                *pcm = sample * 32_768.0;
            }
            let probability = denoiser.process_frame(frame, &self.output);
            speech = Some(if probability.is_finite() {
                probability.clamp(0.0, 1.0)
            } else {
                0.0
            });
            for sample in frame.iter_mut() {
                *sample /= 32_768.0;
            }
            sanitize(frame);
            self.output.fill(0.0);
        }
        if let Some(leveler) = self.leveler.as_mut() {
            run(leveler, frame, &mut self.output)?;
        }
        if let (Some(gate), Some(speech)) = (self.gate.as_mut(), speech) {
            gate.apply(frame, speech, steady);
        }
        let processed = self.engine.is_some() || self.denoiser.is_some() || self.leveler.is_some();
        Ok(ProcessingMetrics {
            raw_level,
            processed_level: if processed { level(frame) } else { raw_level },
            elapsed_micros: started.elapsed().as_micros().min(u64::MAX as u128) as u64,
            speech,
        })
    }
}

/// One Sonora pass over `frame`, through `scratch`, which is left cleared.
fn run(
    engine: &mut AudioProcessing,
    frame: &mut [f32],
    scratch: &mut [f32],
) -> Result<(), MediaError> {
    if engine
        .process_capture_f32(&[frame], &mut [scratch])
        .is_err()
    {
        frame.fill(0.0);
        scratch.fill(0.0);
        return Err(MediaError(
            t("falló el procesamiento local del audio").into(),
        ));
    }
    sanitize(scratch);
    frame.copy_from_slice(scratch);
    scratch.fill(0.0);
    Ok(())
}

fn engine(sample_rate: u32, mode: ProcessingMode, echo: bool) -> Option<AudioProcessing> {
    // shortcut: RNNoise only runs at 48 kHz, other rates get WebRTC's
    // suppressor; resample around it if such microphones turn out common.
    let denoise = mode == ProcessingMode::WebRtc
        || (mode == ProcessingMode::RnNoise && sample_rate != RNNOISE_RATE);
    if !denoise && !echo {
        return None;
    }
    let config = Config {
        pipeline: sonora::config::Pipeline {
            maximum_internal_processing_rate: MaxProcessingRate::Rate48kHz,
            ..Default::default()
        },
        high_pass_filter: denoise.then(HighPassFilter::default),
        noise_suppression: denoise.then(|| NoiseSuppression {
            level: NoiseSuppressionLevel::Moderate,
            ..Default::default()
        }),
        // AEC only with a played reference, which the caller gates; gain is
        // its own stage, after RNNoise.
        echo_canceller: echo.then(EchoCanceller::default),
        gain_controller2: None,
        ..Default::default()
    };
    Some(sonora_with(config, sample_rate))
}

/// RNNoise, primed with one silent frame: its first output fades in.
fn denoiser(sample_rate: u32, mode: ProcessingMode) -> Option<Box<DenoiseState<'static>>> {
    (mode == ProcessingMode::RnNoise && sample_rate == RNNOISE_RATE).then(|| {
        let mut state = DenoiseState::new();
        let mut primed = [0.0; DenoiseState::FRAME_SIZE];
        state.process_frame(&mut primed, &[0.0; DenoiseState::FRAME_SIZE]);
        state
    })
}

/// AGC2's adaptive digital gain alone: up to 20 dB, starting from none, so
/// turning it on never jumps the level and a quiet room is not pumped up.
fn leveler(sample_rate: u32, on: bool) -> Option<AudioProcessing> {
    on.then(|| {
        sonora_with(
            Config {
                gain_controller2: Some(GainController2 {
                    adaptive_digital: Some(AdaptiveDigital {
                        max_gain_db: 20.0,
                        initial_gain_db: 0.0,
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            sample_rate,
        )
    })
}

fn sonora_with(config: Config, sample_rate: u32) -> AudioProcessing {
    AudioProcessing::builder()
        .config(config)
        .capture_config(StreamConfig::new(sample_rate, 1))
        .render_config(StreamConfig::new(PLAYBACK_RATE, 1))
        .build()
}

fn sanitize(frame: &mut [f32]) {
    for sample in frame {
        *sample = if sample.is_finite() {
            sample.clamp(-1.0, 1.0)
        } else {
            0.0
        };
    }
}

fn level(frame: &[f32]) -> f32 {
    meter_position(dbfs(rms(frame)))
}

/// Synthetic speech and noises through the chain, measured.
#[cfg(test)]
mod eval;

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(frame: &mut [f32], state: &mut u32) {
        for sample in frame {
            *state ^= *state << 13;
            *state ^= *state >> 17;
            *state ^= *state << 5;
            *sample = (*state as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32 * 0.15;
        }
    }

    #[test]
    fn bypass_preserves_normalized_audio_and_reports_its_level() {
        let mut processor = CaptureProcessor::new(48_000, ProcessingMode::Off).unwrap();
        let mut frame = (0..processor.frame_len())
            .map(|i| if i % 2 == 0 { -0.25_f32 } else { 0.25 })
            .collect::<Vec<_>>();
        frame[0] = -0.0;
        let original_bits = frame
            .iter()
            .map(|sample| sample.to_bits())
            .collect::<Vec<_>>();
        let expected = level(&frame);
        let result = processor.process(&mut frame).unwrap();
        assert_eq!(
            original_bits,
            frame
                .iter()
                .map(|sample| sample.to_bits())
                .collect::<Vec<_>>()
        );
        assert_eq!(result.raw_level, expected);
        assert_eq!(result.processed_level, expected);
        assert!(processor.engine.is_none());
        assert_eq!(ProcessingMode::default(), ProcessingMode::Off);
    }

    #[test]
    fn silence_stays_silent_in_both_modes() {
        for mode in [
            ProcessingMode::Off,
            ProcessingMode::WebRtc,
            ProcessingMode::RnNoise,
        ] {
            let mut processor = CaptureProcessor::new(48_000, mode).unwrap();
            let mut frame = vec![0.0; processor.frame_len()];
            for _ in 0..20 {
                let metrics = processor.process(&mut frame).unwrap();
                assert_eq!(metrics.raw_level, 0.0);
                assert_eq!(metrics.processed_level, 0.0);
                assert!(frame.iter().all(|sample| sample.abs() <= 1e-7));
            }
        }
    }

    #[test]
    fn stationary_noise_is_attenuated_after_adaptation() {
        let mut processor = CaptureProcessor::new(48_000, ProcessingMode::WebRtc).unwrap();
        let mut state = 0x9234_5637;
        let mut frame = vec![0.0; processor.frame_len()];
        let (mut raw_energy, mut processed_energy) = (0.0_f64, 0.0_f64);
        for index in 0..400 {
            noise(&mut frame, &mut state);
            if index >= 300 {
                raw_energy += frame
                    .iter()
                    .map(|sample| f64::from(*sample).powi(2))
                    .sum::<f64>();
            }
            processor.process(&mut frame).unwrap();
            if index >= 300 {
                processed_energy += frame
                    .iter()
                    .map(|sample| f64::from(*sample).powi(2))
                    .sum::<f64>();
            }
        }
        let rms_ratio = (processed_energy / raw_energy).sqrt();
        assert!(
            rms_ratio > 0.0 && rms_ratio < 0.85,
            "stationary noise RMS ratio: {rms_ratio}"
        );
    }

    #[test]
    fn device_rates_keep_their_frame_size_and_finite_output() {
        for rate in [8_000, 44_100, 48_000, 96_000, 192_000] {
            let mut processor = CaptureProcessor::new(rate, ProcessingMode::WebRtc).unwrap();
            assert_eq!(processor.frame_len(), (rate / 100) as usize);
            let mut frame = vec![0.0; processor.frame_len()];
            let mut state = 1;
            for _ in 0..12 {
                noise(&mut frame, &mut state);
                let metrics = processor.process(&mut frame).unwrap();
                assert!(frame
                    .iter()
                    .all(|sample| sample.is_finite() && sample.abs() <= 1.0));
                assert!((0.0..=1.0).contains(&metrics.raw_level));
                assert!((0.0..=1.0).contains(&metrics.processed_level));
            }
        }
    }

    #[test]
    fn invalid_rates_and_partial_frames_are_rejected_before_mutation() {
        for rate in [0, 7_999, 44_101, 192_100, u32::MAX] {
            assert!(CaptureProcessor::new(rate, ProcessingMode::Off).is_err());
        }
        let mut processor = CaptureProcessor::new(48_000, ProcessingMode::WebRtc).unwrap();
        for len in [0, 479, 481, 960] {
            let mut frame = vec![2.0; len];
            assert!(processor.process(&mut frame).is_err());
            assert!(frame.iter().all(|sample| *sample == 2.0));
        }
        let mut frame = vec![0.1; processor.frame_len()];
        let mut expected = frame.clone();
        CaptureProcessor::new(48_000, ProcessingMode::WebRtc)
            .unwrap()
            .process(&mut expected)
            .unwrap();
        processor.process(&mut frame).unwrap();
        assert_eq!(frame, expected);
    }

    #[test]
    fn reset_and_mode_changes_discard_adaptive_history() {
        let mut processor = CaptureProcessor::new(48_000, ProcessingMode::WebRtc).unwrap();
        let mut state = 23;
        let mut frame = vec![0.0; processor.frame_len()];
        for _ in 0..30 {
            noise(&mut frame, &mut state);
            processor.process(&mut frame).unwrap();
        }
        noise(&mut frame, &mut state);
        let input = frame.clone();
        let mut expected = input.clone();
        CaptureProcessor::new(48_000, ProcessingMode::WebRtc)
            .unwrap()
            .process(&mut expected)
            .unwrap();
        processor.reset();
        processor.process(&mut frame).unwrap();
        assert_eq!(frame, expected);
        processor.set_mode(ProcessingMode::Off);
        frame.copy_from_slice(&input);
        processor.process(&mut frame).unwrap();
        assert_eq!(frame, input);
        processor.set_mode(ProcessingMode::WebRtc);
        processor.process(&mut frame).unwrap();
        assert_eq!(frame, expected);
    }

    #[test]
    fn selecting_current_mode_preserves_filter_history() {
        let mut first = CaptureProcessor::new(48_000, ProcessingMode::WebRtc).unwrap();
        let mut second = CaptureProcessor::new(48_000, ProcessingMode::WebRtc).unwrap();
        let mut state = 65;
        let mut frame = vec![0.0; first.frame_len()];
        for _ in 0..20 {
            noise(&mut frame, &mut state);
            let mut expected = frame.clone();
            first.set_mode(ProcessingMode::WebRtc);
            first.process(&mut frame).unwrap();
            second.process(&mut expected).unwrap();
            assert_eq!(frame, expected);
        }
    }

    #[test]
    fn nonfinite_samples_are_silenced_and_clipping_is_bounded_in_both_modes() {
        for mode in [
            ProcessingMode::Off,
            ProcessingMode::WebRtc,
            ProcessingMode::RnNoise,
        ] {
            let mut processor = CaptureProcessor::new(48_000, mode).unwrap();
            let mut frame = vec![0.0; processor.frame_len()];
            frame[..7].copy_from_slice(&[
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                5.0,
                -3.0,
                0.25,
                -0.0,
            ]);
            let mut expected = vec![0.0; processor.frame_len()];
            expected[..7].copy_from_slice(&[0.0, 0.0, 0.0, 1.0, -1.0, 0.25, -0.0]);
            let metrics = processor.process(&mut frame).unwrap();
            assert_eq!(metrics.raw_level, level(&expected));
            assert!(metrics.processed_level.is_finite());
            assert!(frame
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 1.0));
            assert!(processor.output.iter().all(|sample| *sample == 0.0));
            if mode == ProcessingMode::Off {
                assert_eq!(frame, expected);
            }
            // Invalid input must not poison adaptive state for subsequent frames.
            for _ in 0..5 {
                frame.fill(0.1);
                processor.process(&mut frame).unwrap();
                assert!(frame.iter().all(|sample| sample.is_finite()));
            }
        }
    }

    #[test]
    fn echo_cancellation_needs_its_switch_and_whole_reference_frames() {
        let mut processor = CaptureProcessor::new(44_100, ProcessingMode::Off).unwrap();
        assert!(processor.engine.is_none());
        assert!(processor.render(&[0.0; REFERENCE_FRAME], 0).is_err(), "off");
        processor.set_echo_cancellation(true);
        let config = processor.engine.as_ref().unwrap().config();
        assert!(config.echo_canceller.is_some());
        assert!(
            config.noise_suppression.is_none(),
            "echo alone adds no denoiser"
        );
        assert!(processor.render(&[0.0; 441], 0).is_err());
        let mut frame = vec![0.0; processor.frame_len()];
        for _ in 0..5 {
            processor.render(&[0.1; REFERENCE_FRAME], 30).unwrap();
            processor.process(&mut frame).unwrap();
        }
        assert!(processor.rendered.iter().all(|sample| *sample == 0.0));
        processor.set_echo_cancellation(false);
        assert!(processor.engine.is_none());
    }

    #[test]
    fn a_played_reference_attenuates_its_own_echo() {
        let mut processor = CaptureProcessor::new(48_000, ProcessingMode::Off).unwrap();
        processor.set_echo_cancellation(true);
        let mut state = 0x1234_5678;
        let mut reference = vec![0.0; REFERENCE_FRAME];
        let mut history = std::collections::VecDeque::from(vec![0.0_f32; 480]);
        let (mut raw_energy, mut processed_energy) = (0.0_f64, 0.0_f64);
        for index in 0..600 {
            noise(&mut reference, &mut state);
            processor.render(&reference, 0).unwrap();
            // The microphone hears the speaker 10 ms later, at half level.
            let mut frame: Vec<f32> = reference
                .iter()
                .map(|sample| {
                    history.push_back(*sample);
                    history.pop_front().unwrap() * 0.5
                })
                .collect();
            if index >= 500 {
                raw_energy += frame.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
            }
            processor.process(&mut frame).unwrap();
            if index >= 500 {
                processed_energy += frame.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
            }
        }
        let ratio = (processed_energy / raw_energy).sqrt();
        assert!(ratio < 0.3, "echo RMS ratio after adaptation: {ratio}");
    }

    #[test]
    fn noise_suppression_does_not_enable_echo_cancellation_or_gain_control() {
        let processor = CaptureProcessor::new(48_000, ProcessingMode::WebRtc).unwrap();
        let config = processor.engine.as_ref().unwrap().config();
        assert!(config.high_pass_filter.is_some());
        assert_eq!(
            config.noise_suppression.as_ref().unwrap().level,
            NoiseSuppressionLevel::Moderate
        );
        assert_eq!(
            config.pipeline.maximum_internal_processing_rate,
            MaxProcessingRate::Rate48kHz
        );
        assert!(config.echo_canceller.is_none());
        assert!(config.gain_controller2.is_none());
        assert!(config.pre_amplifier.is_none());
        assert!(config.capture_level_adjustment.is_none());
        assert!(processor.leveler.is_none() && processor.denoiser.is_none());
    }

    /// The RMS ratio of output to input over the last 100 of 400 frames of
    /// stationary noise.
    fn noise_ratio(processor: &mut CaptureProcessor) -> f64 {
        let mut state = 0x9234_5637;
        let mut frame = vec![0.0; processor.frame_len()];
        let (mut raw_energy, mut processed_energy) = (0.0_f64, 0.0_f64);
        for index in 0..400 {
            noise(&mut frame, &mut state);
            let energy = |frame: &[f32]| frame.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
            if index >= 300 {
                raw_energy += energy(&frame);
            }
            processor.process(&mut frame).unwrap();
            assert!(frame
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 1.0));
            if index >= 300 {
                processed_energy += energy(&frame);
            }
        }
        (processed_energy / raw_energy).sqrt()
    }

    #[test]
    fn rnnoise_takes_whole_ten_millisecond_frames_at_48_khz_and_removes_noise() {
        let mut processor = CaptureProcessor::new(48_000, ProcessingMode::RnNoise).unwrap();
        assert!(processor.rnnoise());
        assert_eq!(processor.frame_len(), DenoiseState::FRAME_SIZE);
        assert!(processor.engine.is_none(), "RNNoise alone, no WebRTC stage");
        let ratio = noise_ratio(&mut processor);
        assert!(ratio < 0.75, "RNNoise stationary noise RMS ratio: {ratio}");
        assert!(processor.output.iter().all(|sample| *sample == 0.0));
        // Partial frames are refused before RNNoise sees them.
        let mut short = vec![0.5; 479];
        assert!(processor.process(&mut short).is_err());
        assert!(short.iter().all(|sample| *sample == 0.5));
    }

    #[test]
    fn rnnoise_hands_other_rates_to_webrtc_and_switches_in_place() {
        let processor = CaptureProcessor::new(44_100, ProcessingMode::RnNoise).unwrap();
        assert!(!processor.rnnoise());
        let config = processor.engine.as_ref().unwrap().config();
        assert!(config.noise_suppression.is_some(), "WebRTC stands in");
        let mut processor = CaptureProcessor::new(48_000, ProcessingMode::WebRtc).unwrap();
        for (mode, rnnoise, webrtc) in [
            (ProcessingMode::RnNoise, true, false),
            (ProcessingMode::Off, false, false),
            (ProcessingMode::WebRtc, false, true),
        ] {
            processor.set_mode(mode);
            assert_eq!(processor.rnnoise(), rnnoise, "{mode:?}");
            assert_eq!(processor.engine.is_some(), webrtc, "{mode:?}");
        }
    }

    #[test]
    fn automatic_gain_is_its_own_bounded_stage_and_turns_off_alone() {
        let mut processor = CaptureProcessor::new(48_000, ProcessingMode::RnNoise).unwrap();
        processor.set_automatic_gain(true);
        assert!(processor.engine.is_none(), "no WebRTC suppression with it");
        let config = processor.leveler.as_ref().unwrap().config();
        let digital = config
            .gain_controller2
            .as_ref()
            .unwrap()
            .adaptive_digital
            .clone()
            .unwrap();
        // It only moves on speech, which its own detector hears: synthetic
        // signals leave it at its start, 0 dB, so turning it on never jumps.
        assert_eq!((digital.initial_gain_db, digital.max_gain_db), (0.0, 20.0));
        assert!(config.noise_suppression.is_none() && config.echo_canceller.is_none());
        let mut frame = vec![0.0; processor.frame_len()];
        for index in 0..50 {
            frame.fill(if index % 2 == 0 { 1.0 } else { -1.0 });
            processor.process(&mut frame).unwrap();
            assert!(frame
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 1.0));
        }
        processor.set_automatic_gain(false);
        assert!(processor.leveler.is_none());
        assert!(processor.rnnoise(), "the suppressor keeps going");
    }

    #[test]
    fn the_input_volume_scales_before_everything_and_clips() {
        let mut processor = CaptureProcessor::new(48_000, ProcessingMode::Off).unwrap();
        let run = |processor: &mut CaptureProcessor, volume: f32| {
            processor.set_input_volume(volume);
            let mut frame = vec![0.6; processor.frame_len()];
            processor.process(&mut frame).unwrap();
            frame[0]
        };
        assert_eq!(run(&mut processor, 0.5), 0.3);
        assert_eq!(run(&mut processor, 0.0), 0.0);
        assert_eq!(run(&mut processor, 2.0), 1.0, "clipped, not wrapped");
        assert_eq!(run(&mut processor, 9.0), 1.0, "at most twice");
        assert_eq!(run(&mut processor, f32::NAN), 0.6, "nonsense is unity");
    }
}
