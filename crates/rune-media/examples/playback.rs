//! Plays RUNE's test chime on an output and prints its counters, so the owner
//! can listen without opening the window. Nothing is recorded or saved.
//!
//! `cargo run -p rune-media --example playback` plays the chime on the default
//! output for five seconds. Options:
//!
//! - `--device ID`: an output id as `--example devices` lists it.
//! - `--volume 0.5`: linear gain, 0 to 2.
//! - `--seconds 10`: how long.
//! - `--deafen-after 2`: deafen after that many seconds; the chime must stop.
//! - `--loopback`: instead of the chime, play the default microphone back
//!   (use headphones). Add `--denoise` (WebRTC) or `--rnnoise`, `--echo`,
//!   `--agc` (automatic gain) and `--gate` (RNNoise's speech gate, as the
//!   default profile sends) to process it.

use std::time::{Duration, Instant};

use rune_media::{
    Capture, CaptureRequest, CaptureState, Playback, PlaybackRequest, PlaybackSource,
    ProcessingMode,
};

fn value(name: &str) -> Option<String> {
    let mut arguments = std::env::args().skip_while(|argument| argument != name);
    arguments.next()?;
    arguments.next()
}

fn flag(name: &str) -> bool {
    std::env::args().any(|argument| argument == name)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds: f32 = value("--seconds").map_or(Ok(5.0), |text| text.parse())?;
    let volume: f32 = value("--volume").map_or(Ok(1.0), |text| text.parse())?;
    let deafen_after: Option<f32> = value("--deafen-after")
        .map(|text| text.parse())
        .transpose()?;
    let loopback = flag("--loopback");
    let mut playback = Playback::new()?;
    let mut capture = None;
    if loopback {
        let mut microphone = Capture::new()?;
        microphone.set_monitor(playback.feed());
        microphone.set_echo_reference(playback.echo_reference());
        microphone.configure(Some(CaptureRequest {
            device: None,
            mode: if flag("--rnnoise") {
                ProcessingMode::RnNoise
            } else if flag("--denoise") {
                ProcessingMode::WebRtc
            } else {
                ProcessingMode::Off
            },
            muted: false,
            echo_cancellation: flag("--echo"),
            automatic_gain: flag("--agc"),
            monitor: true,
            speech_gate: flag("--gate"),
            system_echo: flag("--system-echo"),
            output: value("--device"),
        }));
        capture = Some(microphone);
    }
    let mut request = PlaybackRequest {
        device: value("--device"),
        source: if loopback {
            PlaybackSource::Feed
        } else {
            PlaybackSource::Tone
        },
        volume,
        deafened: false,
    };
    playback.configure(Some(request.clone()));
    println!(
        "{} en {}",
        if loopback {
            "escuchando el micrófono"
        } else {
            "tocando el tono de prueba"
        },
        request
            .device
            .as_deref()
            .unwrap_or("la salida predeterminada")
    );
    let started = Instant::now();
    while started.elapsed().as_secs_f32() < seconds {
        std::thread::sleep(Duration::from_millis(250));
        let elapsed = started.elapsed().as_secs_f32();
        if deafen_after.is_some_and(|after| elapsed >= after) && !request.deafened {
            request.deafened = true;
            playback.configure(Some(request.clone()));
            println!("ensordecido: no debería oírse nada más");
        }
        let output = playback.snapshot();
        let input = capture.as_ref().map(Capture::snapshot);
        println!(
            "{elapsed:4.1}s {:?} {} Hz × {} | latencia {:.1} ms + cola {:.1} ms | underrun {} | overrun {}{}",
            output.state,
            output.rate,
            output.channels,
            output.output_latency_micros as f32 / 1_000.0,
            output.queued_micros as f32 / 1_000.0,
            output.underruns,
            output.overruns,
            input
                .map(|input| format!(
                    " | micrófono {:?} {:.2}, eco {}",
                    input.state, input.processed_level, input.echo_cancelling
                ))
                .unwrap_or_default(),
        );
        if matches!(output.state, CaptureState::Failed(_)) {
            break;
        }
    }
    playback.configure(None);
    Ok(())
}
