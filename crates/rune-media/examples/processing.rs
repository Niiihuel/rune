//! Measures local DSP with deterministic synthetic audio; no device is opened.
//! `cargo run --release -p rune-media --example processing`

use std::hint::black_box;

use rune_media::{CaptureProcessor, ProcessingMode};

const WARMUP_FRAMES: usize = 100;
const MEASURED_FRAMES: usize = 1_000;
const FRAME_BUDGET_MICROS: u64 = 10_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("DSP local · señal sintética determinista · sin dispositivos ni grabación");
    println!("Cuadros mono de 10 ms; 100 de calentamiento + 1000 medidos por caso.");
    println!("Tiempos de procesamiento, sin callback, cola, transporte ni reproducción.");
    for rate in [44_100, 48_000, 96_000] {
        for mode in [
            ProcessingMode::Off,
            ProcessingMode::WebRtc,
            ProcessingMode::RnNoise,
        ] {
            measure(rate, mode)?;
        }
    }
    Ok(())
}

fn measure(rate: u32, mode: ProcessingMode) -> Result<(), rune_media::MediaError> {
    let mut processor = CaptureProcessor::new(rate, mode)?;
    let mut frame = vec![0.0; processor.frame_len()];
    let mut elapsed = Vec::with_capacity(MEASURED_FRAMES);
    let mut random = 0x981a_9203_u32;
    let mut samples = 0_u64;
    let (mut raw_energy, mut processed_energy) = (0.0_f64, 0.0_f64);
    for index in 0..(WARMUP_FRAMES + MEASURED_FRAMES) {
        for sample in &mut frame {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            let noise = (random as f64 / u32::MAX as f64 * 2.0 - 1.0) * 0.06;
            // A gated 220 Hz tone plus harmonics is only a synthetic workload,
            // not a speech-quality or intelligibility test.
            let phase = samples as f64 / f64::from(rate) * std::f64::consts::TAU * 220.0;
            let tone = if index % 100 < 60 {
                0.14 * phase.sin() + 0.04 * (phase * 2.0).sin()
            } else {
                0.0
            };
            *sample = (noise + tone) as f32;
            samples += 1;
        }
        if index >= WARMUP_FRAMES {
            raw_energy += energy(&frame);
        }
        let metrics = processor.process(black_box(&mut frame))?;
        if index >= WARMUP_FRAMES {
            elapsed.push(metrics.elapsed_micros);
            processed_energy += energy(black_box(&frame));
        }
    }
    elapsed.sort_unstable();
    let p50 = elapsed[(MEASURED_FRAMES - 1) * 50 / 100];
    let p95 = elapsed[(MEASURED_FRAMES - 1) * 95 / 100];
    let max = elapsed[MEASURED_FRAMES - 1];
    let overruns = elapsed
        .iter()
        .filter(|micros| **micros >= FRAME_BUDGET_MICROS)
        .count();
    println!(
        "{rate} Hz {mode:?}: p50={p50} µs p95={p95} µs máx={max} µs; p95/presupuesto={:.2}%; ≥10 ms={overruns}/{MEASURED_FRAMES}; RMS salida/entrada={:.3}",
        p95 as f64 / FRAME_BUDGET_MICROS as f64 * 100.0,
        (processed_energy / raw_energy).sqrt(),
    );
    Ok(())
}

fn energy(frame: &[f32]) -> f64 {
    frame.iter().map(|sample| f64::from(*sample).powi(2)).sum()
}
