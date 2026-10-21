//! The echo the whole capture path leaves, measured as the devices would run
//! it: the output's callback (RUNE's own, [`Speaker`]) and the microphone's
//! every PipeWire quantum, the output's worker moving what played to the
//! reference every 4 ms, the capture worker stepping every 4 ms on its own
//! timer, and the sound going from the headphones into the microphone
//! through a filtered, delayed path. No device, file or recording:
//! deterministic synthetic signals only.
//!
//! `cargo test --release -p rune-media echo_table -- --ignored --nocapture`
//! prints the table of docs/ARCHITECTURE.md; the other tests hold the default
//! path to it.

use super::*;
use crate::playback::Speaker;
use crate::processing::eval::{speech, Rng, Speech, RATE};

/// The graph's period: PipeWire's default quantum at 48 kHz, about 21 ms.
const QUANTUM: usize = 1_024;
/// Callback to speaker: one quantum in the device's buffer and the DAC.
const OUTPUT_LATENCY: usize = QUANTUM + 96;
/// Microphone to its callback: the ADC.
const INPUT_LATENCY: usize = 96;
/// Where each periodic thing happens within its period, in samples: not
/// in step, as on a real machine.
const CAPTURE_PHASE: usize = 300;
const PLAYBACK_WORKER_PHASE: usize = 37;
const CAPTURE_WORKER_PHASE: usize = 101;
/// The scene's sample the microphone's stream starts with.
const MIC_START: usize = CAPTURE_PHASE - INPUT_LATENCY;
/// The workers' poll, 4 ms.
const POLL: usize = RATE / 250;
/// The reference's ring as the output makes it: 200 ms.
const REFERENCE_RING: usize = RATE / 5;
/// An underrun: the call's audio comes 15 ms late, once every 2.5 s.
const UNDERRUN: usize = 720;
const UNDERRUN_EVERY: usize = RATE * 5 / 2;

/// What the headset carries from its speakers into its microphone.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EchoPath {
    /// Linear gain of the direct path.
    pub gain: f32,
    /// Acoustic delay, in samples.
    pub delay: usize,
    /// A one-pole low-pass's coefficient: cheap earcups lose the treble.
    pub smoothing: f32,
    /// A reflection: how much later, and how loud against the direct path.
    pub reflection: (usize, f32),
}

/// Loud leakage: −12 dB, 1 ms, dull, with a reflection 2 ms later.
pub(crate) const HEADSET: EchoPath = EchoPath {
    gain: 0.25,
    delay: 48,
    smoothing: 0.35,
    reflection: (96, 0.3),
};

/// Faint leakage, −34 dB: what a closed headset lets through.
pub(crate) const FAINT: EchoPath = EchoPath {
    gain: 0.02,
    ..HEADSET
};

/// Which reference the canceller gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reference {
    /// What RUNE's output played, through its worker.
    Rune,
    /// The output's monitor: everything the system played there.
    System,
}

/// One simulated stretch of call.
pub(crate) struct Scene<'a> {
    /// What RUNE plays: the others in the call.
    pub call: &'a [f32],
    /// What other applications play on the same output: a game, music.
    pub pc: &'a [f32],
    /// The person's own voice at the microphone.
    pub near: &'a [f32],
    pub path: EchoPath,
    pub reference: Reference,
    pub request: CaptureRequest,
    /// The call's audio now and then arrives late, and the output plays
    /// silence meanwhile.
    pub underruns: bool,
    /// Muted from one second to another.
    pub mute: Option<(f32, f32)>,
}

impl<'a> Scene<'a> {
    pub(crate) fn new(call: &'a [f32], pc: &'a [f32], near: &'a [f32]) -> Self {
        Self {
            call,
            pc,
            near,
            path: HEADSET,
            reference: Reference::Rune,
            request: voice_isolation(),
            underruns: false,
            mute: None,
        }
    }
}

/// The microphone's stream, the echo in it, and what the capture sent, at
/// [`RATE`] from the microphone's first sample ([`MIC_START`] into the
/// scene). While muted, `sent` is silence.
pub(crate) struct Heard {
    pub microphone: Vec<f32>,
    pub echo: Vec<f32>,
    pub sent: Vec<f32>,
}

/// The speakers' sound through `path`.
fn through(path: EchoPath, speaker: &[f32]) -> Vec<f32> {
    let mut smoothed = 0.0_f32;
    let dull: Vec<f32> = speaker
        .iter()
        .map(|sample| {
            smoothed += (1.0 - path.smoothing) * (sample - smoothed);
            smoothed
        })
        .collect();
    let at = |n: usize, by: usize| n.checked_sub(by).map_or(0.0, |n| dull[n]);
    (0..speaker.len())
        .map(|n| {
            path.gain
                * (at(n, path.delay) + path.reflection.1 * at(n, path.delay + path.reflection.0))
        })
        .collect()
}

pub(crate) fn simulate(scene: &Scene) -> Heard {
    let length = scene.call.len().min(scene.pc.len()).min(scene.near.len());
    // The output: each callback plays a quantum of the call (less when its
    // audio is late) and hands on what it played.
    let mut speaker = Speaker::new();
    let mut cycles = Vec::new();
    let mut from = 0;
    let mut out = vec![0.0; QUANTUM];
    for start in (0..length).step_by(QUANTUM) {
        let late = scene.underruns && start % UNDERRUN_EVERY < QUANTUM && start > 0;
        let take = if late { QUANTUM - UNDERRUN } else { QUANTUM };
        let to = (from + take).min(scene.call.len());
        speaker.queue(&scene.call[from..to]);
        from = to;
        speaker.play(&mut out);
        let mut played = Vec::new();
        speaker.played(&mut played);
        cycles.push((out.clone(), played));
    }
    let output: Vec<f32> = cycles.iter().flat_map(|(out, _)| out.clone()).collect();
    // What the sink sends the device: RUNE's and the others' sound.
    let mix: Vec<f32> = (0..length)
        .map(|n| output.get(n).copied().unwrap_or(0.0) + scene.pc[n])
        .collect();
    let speakers: Vec<f32> = (0..length)
        .map(|n| n.checked_sub(OUTPUT_LATENCY).map_or(0.0, |n| mix[n]))
        .collect();
    let echo = through(scene.path, &speakers);
    let acoustic: Vec<f32> = echo
        .iter()
        .zip(scene.near)
        .map(|(echo, near)| (echo + near).clamp(-1.0, 1.0))
        .collect();

    let shared = Arc::new(Shared::new());
    shared.requested.store(1, Ordering::Release);
    shared.ready.store(1, Ordering::Release);
    shared.muted.store(false, Ordering::Release);
    let (mut ingress, consumer) = ring(RATE as u32, 1, shared.clone()).unwrap();
    let opened = Opened {
        stream: (),
        consumer,
        signals: ingress.signals.clone(),
        rate: RATE as u32,
    };
    let mut active = Active::new(opened, &scene.request).unwrap();
    let (mut played, reference) = EchoReference::pair(REFERENCE_RING);
    let (mut monitor, system) = EchoReference::pair(REFERENCE_RING);
    if scene.reference == Reference::System {
        active.system = Some((
            Loopback {
                _stream: Box::new(()),
                failed: Arc::new(AtomicBool::new(false)),
                reference: system,
            },
            None,
            0,
        ));
    }
    let (feed, mut tap) = crate::tap(length + RATE);
    let mut links = Links {
        reference: Some(reference),
        monitor: None,
        transmit: Some(feed),
    };
    let mute = scene
        .mute
        .map(|(from, to)| ((from * RATE as f32) as usize, (to * RATE as f32) as usize));
    let muted_request = CaptureRequest {
        muted: true,
        ..scene.request.clone()
    };
    // What the output's callback played, until its worker moves it.
    let mut pending: Vec<f32> = Vec::new();
    let (mut microphone, mut echoed, mut sent) = (Vec::new(), Vec::new(), Vec::new());
    let start = Instant::now();
    let mut jitter = Rng::new(5);
    let mut next_poll = PLAYBACK_WORKER_PHASE;
    let mut next_step = CAPTURE_WORKER_PHASE;
    let mut muted = false;
    for now in 0..length {
        if now % QUANTUM == 0 {
            pending.extend_from_slice(&cycles[now / QUANTUM].1);
            // The monitor hands the cycle's mix straight to the capture.
            for sample in &mix[now..(now + QUANTUM).min(length)] {
                let _ = monitor.push(*sample);
            }
        }
        if now == next_poll {
            for sample in pending.drain(..) {
                let _ = played.push(sample);
            }
            // The worker wakes a little late now and then.
            next_poll += POLL + (jitter.next() % 48) as usize;
        }
        if let Some((from, to)) = mute {
            if now == from || now == to {
                muted = now == from;
                shared.muted.store(muted, Ordering::Release);
                active.reconfigure(if muted {
                    &muted_request
                } else {
                    &scene.request
                });
            }
        }
        if now % QUANTUM == CAPTURE_PHASE && now >= QUANTUM + INPUT_LATENCY {
            let end = now - INPUT_LATENCY;
            let chunk = &acoustic[end - QUANTUM..end];
            microphone.extend_from_slice(chunk);
            echoed.extend_from_slice(&echo[end - QUANTUM..end]);
            if muted {
                // Not admitted; the others hear nothing meanwhile.
                sent.extend(std::iter::repeat_n(0.0, QUANTUM));
            }
            ingress.write(chunk);
        }
        if now == next_step {
            if !muted {
                let at = start + Duration::from_micros(now as u64 * 1_000_000 / RATE as u64);
                active.step(&shared, 1, at, &mut links).unwrap();
                let mut frames = vec![0.0; tap.available()];
                tap.read(&mut frames);
                sent.extend_from_slice(&frames);
            }
            // Its own timer: the two workers drift against each other.
            next_step += POLL + (jitter.next() % 48) as usize;
        }
    }
    Heard {
        microphone,
        echo: echoed,
        sent,
    }
}

/// Energy of `samples` within `seconds` of the microphone's stream.
fn energy(samples: &[f32], seconds: std::ops::Range<f32>) -> f64 {
    let range = (seconds.start * RATE as f32) as usize..(seconds.end * RATE as f32) as usize;
    samples
        .get(range.start.min(samples.len())..range.end.min(samples.len()))
        .unwrap_or_default()
        .iter()
        .map(|sample| f64::from(*sample).powi(2))
        .sum()
}

fn decibels(ratio: f64) -> f64 {
    10.0 * ratio.max(1e-12).log10()
}

/// Echo return loss enhancement within `seconds`, in dB: how much quieter
/// what was sent is than what the microphone heard, with only echo.
pub(crate) fn erle(heard: &Heard, seconds: std::ops::Range<f32>) -> f64 {
    decibels(energy(&heard.microphone, seconds.clone()) / energy(&heard.sent, seconds))
}

/// 20 ms stretches within `seconds` where echo reached the microphone and
/// something went out: someone hearing themselves, once the call's gate
/// has decided (sent is non-zero).
pub(crate) fn echo_sent(heard: &Heard, seconds: std::ops::Range<f32>) -> f64 {
    let frame = RATE / 50;
    let loud = 0.05 * crate::rms(&heard.echo);
    let (mut echoing, mut sent) = (0, 0);
    let range = (seconds.start * RATE as f32) as usize..(seconds.end * RATE as f32) as usize;
    for begin in range.step_by(frame) {
        let (Some(echo), Some(out)) = (
            heard.echo.get(begin..begin + frame),
            heard.sent.get(begin..begin + frame),
        ) else {
            break;
        };
        if crate::rms(echo) < loud {
            continue;
        }
        echoing += 1;
        sent += usize::from(crate::rms(out) > 1e-4);
    }
    sent as f64 / f64::from(echoing.max(1))
}

/// Music: a chord progression of soft saws, a bass line, a kick and hi-hats,
/// about −20 dBFS.
pub(crate) fn music(seconds: f32, seed: u32) -> Vec<f32> {
    let length = (seconds * RATE as f32) as usize;
    let mut rng = Rng::new(seed);
    let beat = RATE / 2;
    let chords: [[f32; 3]; 4] = [
        [261.6, 329.6, 392.0],
        [220.0, 261.6, 329.6],
        [174.6, 220.0, 261.6],
        [196.0, 246.9, 293.7],
    ];
    let saw = |frequency: f32, t: f32| -> f32 {
        (1..=8)
            .map(|k| (std::f32::consts::TAU * frequency * k as f32 * t).sin() / k as f32)
            .sum::<f32>()
            * 0.5
    };
    let mut samples: Vec<f32> = (0..length)
        .map(|n| {
            let t = n as f32 / RATE as f32;
            let chord = chords[(n / (4 * beat)) % 4];
            let in_beat = (n % beat) as f32 / RATE as f32;
            let pad: f32 = chord.iter().map(|f| saw(*f, t)).sum::<f32>() * 0.08;
            let bass = saw(chord[0] / 2.0, t) * 0.25 * (-in_beat * 3.0).exp();
            let kick = (std::f32::consts::TAU * (60.0 + 90.0 * (-in_beat * 30.0).exp()) * in_beat)
                .sin()
                * (-in_beat * 12.0).exp()
                * 0.6;
            pad + bass + kick
        })
        .collect();
    // Hi-hats on the off-beats.
    for at in (beat / 2..length).step_by(beat) {
        for n in 0..RATE / 40 {
            if let Some(sample) = samples.get_mut(at + n) {
                *sample += rng.signed() * 0.15 * (-(n as f32) / (RATE as f32 * 0.008)).exp();
            }
        }
    }
    let gain = 0.1 / f64::from(crate::rms(&samples)).max(1e-9);
    samples
        .iter()
        .map(|sample| (f64::from(*sample) * gain) as f32)
        .collect()
}

/// The canceller alone: no suppressor, gain or gate.
pub(crate) fn echo_only() -> CaptureRequest {
    CaptureRequest {
        device: None,
        mode: ProcessingMode::Off,
        muted: false,
        echo_cancellation: true,
        automatic_gain: false,
        monitor: false,
        speech_gate: false,
        system_echo: false,
        output: None,
    }
}

/// Voice isolation, the default profile: RNNoise, the canceller, the gain
/// and the speech gate.
pub(crate) fn voice_isolation() -> CaptureRequest {
    CaptureRequest {
        mode: ProcessingMode::RnNoise,
        automatic_gain: true,
        speech_gate: true,
        ..echo_only()
    }
}

/// The scenes of the table and of the tests: someone in the call, a game's
/// voices over music on the same output, and the person's own voice.
pub(crate) struct Sounds {
    pub others: Vec<f32>,
    pub pc: Vec<f32>,
    pub quiet: Vec<f32>,
    pub mine: Speech,
}

pub(crate) fn sounds(seconds: f32) -> Sounds {
    let game = speech(seconds, false, 17).samples;
    Sounds {
        others: speech(seconds, true, 13).samples,
        pc: music(seconds, 3)
            .iter()
            .zip(&game)
            .map(|(music, voice)| 0.7 * music + 0.7 * voice)
            .collect(),
        quiet: vec![0.0; (seconds * RATE as f32) as usize],
        mine: speech(seconds, false, 7),
    }
}

/// While the person talks over the echo, after the first 3 s: the voice
/// sent against the same voice sent with no echo at all, in dB. Energies
/// only: the canceller's band filters shift the phase.
pub(crate) fn voice_kept(talking: &Heard, alone: &Heard, mine: &Speech) -> f64 {
    let speaking: Vec<bool> = mine.speaking.iter().skip(MIC_START).copied().collect();
    let within = |samples: &[f32]| -> f64 {
        samples
            .iter()
            .zip(&speaking)
            .skip(3 * RATE)
            .filter(|(_, speaking)| **speaking)
            .map(|(sample, _)| f64::from(*sample).powi(2))
            .sum()
    };
    decibels(within(&talking.sent) / within(&alone.sent))
}

#[test]
#[ignore]
fn echo_table() {
    let seconds = 20.0;
    let Sounds {
        others,
        pc,
        quiet,
        mine,
    } = sounds(seconds);
    type Row<'a> = (
        &'a str,
        &'a [f32],
        &'a [f32],
        Reference,
        EchoPath,
        bool,
        bool,
    );
    let rows: [Row; 7] = [
        (
            "voces de la llamada",
            &others,
            &quiet,
            Reference::Rune,
            HEADSET,
            false,
            false,
        ),
        (
            "voces de la llamada, eco débil (−34 dB)",
            &others,
            &quiet,
            Reference::Rune,
            FAINT,
            false,
            false,
        ),
        (
            "voces de la llamada, con faltantes en la salida",
            &others,
            &quiet,
            Reference::Rune,
            HEADSET,
            true,
            false,
        ),
        (
            "voces de la llamada, silenciado de 8 a 10 s",
            &others,
            &quiet,
            Reference::Rune,
            HEADSET,
            false,
            true,
        ),
        (
            "juego y música, referencia de RUNE",
            &quiet,
            &pc,
            Reference::Rune,
            HEADSET,
            false,
            false,
        ),
        (
            "juego y música, referencia del sistema",
            &quiet,
            &pc,
            Reference::System,
            HEADSET,
            false,
            false,
        ),
        (
            "llamada + juego y música, referencia del sistema",
            &others,
            &pc,
            Reference::System,
            HEADSET,
            false,
            false,
        ),
    ];
    for (name, call, pc, reference, path, underruns, mute) in rows {
        let scene = |near: &[f32], path: EchoPath, request: CaptureRequest| {
            simulate(&Scene {
                path,
                reference,
                request,
                underruns,
                mute: mute.then_some((8.0, 10.0)),
                ..Scene::new(call, pc, near)
            })
        };
        let echo = scene(&quiet, path, echo_only());
        let isolated = scene(&quiet, path, voice_isolation());
        let talking = scene(&mine.samples, path, echo_only());
        let alone = scene(&mine.samples, EchoPath { gain: 0.0, ..path }, echo_only());
        let after = if mute { 10.0..12.0 } else { 3.0..seconds };
        println!(
            "{name}: ERLE {:5.1} dB ({:.0}–{:.0} s), {:5.1} dB los primeros 3 s; Aislamiento de voz manda eco en {:4.1} % de los cuadros ({:4.1} % los primeros 3 s); hablando encima la voz sale {:+.1} dB",
            erle(&echo, after.clone()),
            after.start,
            after.end,
            erle(&echo, 0.0..3.0),
            echo_sent(&isolated, after.clone()) * 100.0,
            echo_sent(&isolated, 0.0..3.0) * 100.0,
            voice_kept(&talking, &alone, &mine),
        );
    }
}

#[test]
fn the_others_voices_stay_out_through_late_audio_and_a_mute() {
    let Sounds { others, quiet, .. } = sounds(12.0);
    let scene = |underruns: bool, mute: Option<(f32, f32)>, request: CaptureRequest| {
        simulate(&Scene {
            request,
            underruns,
            mute,
            ..Scene::new(&others, &quiet, &quiet)
        })
    };
    // Late audio played silence: the reference kept it, so the canceller
    // kept the echo's delay (before 2026-10-19: 27 dB, 4.2 % sent).
    let late = scene(true, None, echo_only());
    assert!(erle(&late, 3.0..12.0) > 45.0, "{}", erle(&late, 3.0..12.0));
    let late = scene(true, None, voice_isolation());
    assert!(echo_sent(&late, 3.0..12.0) < 0.01);
    // Unmuted, once it learned the echo path, it still knows it: the first
    // half second still lets some through, then nothing (before 2026-10-19
    // it started over: 7 to 28 dB for seconds).
    let unmuted = scene(false, Some((8.0, 9.0)), echo_only());
    let left = erle(&unmuted, 9.25..11.0);
    assert!(left > 40.0, "{left}");
    let unmuted = scene(false, Some((8.0, 9.0)), voice_isolation());
    assert!(echo_sent(&unmuted, 9.5..11.0) < 0.01, "before: 74 %");
}

#[test]
fn what_other_applications_play_is_cancelled_with_the_system_reference() {
    let Sounds {
        pc, quiet, mine, ..
    } = sounds(10.0);
    let scene = |reference: Reference, near: &[f32], path: EchoPath, request| {
        simulate(&Scene {
            path,
            reference,
            request,
            ..Scene::new(&quiet, &pc, near)
        })
    };
    // RUNE's own reference knows nothing of it: a game's voices go out.
    let unknown = scene(Reference::Rune, &quiet, HEADSET, voice_isolation());
    assert!(echo_sent(&unknown, 3.0..10.0) > 0.9);
    let known = scene(Reference::System, &quiet, HEADSET, voice_isolation());
    assert!(echo_sent(&known, 3.0..10.0) < 0.01);
    let known = scene(Reference::System, &quiet, HEADSET, echo_only());
    assert!(
        erle(&known, 3.0..10.0) > 40.0,
        "{}",
        erle(&known, 3.0..10.0)
    );
    // Talking over it, the voice goes out as it would without it.
    let talking = scene(Reference::System, &mine.samples, HEADSET, echo_only());
    let silent = EchoPath {
        gain: 0.0,
        ..HEADSET
    };
    let alone = scene(Reference::System, &mine.samples, silent, echo_only());
    let kept = voice_kept(&talking, &alone, &mine);
    assert!(kept.abs() < 2.0, "{kept}");
}
