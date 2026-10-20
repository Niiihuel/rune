//! Lists this machine's audio devices and, with `--meter`, prints the
//! default microphone's raw/processed levels for three seconds. Add `--denoise`
//! (WebRTC) or `--rnnoise`, `--agc`, and `--gate` (RNNoise's speech gate,
//! what the default profile sends), to opt into local processing. This
//! never records or plays samples.
//! `cargo run -p rune-media --example devices -- --meter`

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let devices = rune_media::devices()?;
    println!("backend: {}", devices.backend);
    for (kind, list) in [("entrada", &devices.inputs), ("salida", &devices.outputs)] {
        for device in list {
            println!(
                "{kind}{} {} [{}]",
                if device.default {
                    " (predeterminado)"
                } else {
                    ""
                },
                device.name,
                device.id
            );
        }
    }
    if std::env::args().any(|argument| argument == "--meter") {
        let mut capture = rune_media::Capture::new()?;
        let flag = |name: &str| std::env::args().any(|argument| argument == name);
        let mode = if flag("--rnnoise") {
            rune_media::ProcessingMode::RnNoise
        } else if flag("--denoise") {
            rune_media::ProcessingMode::WebRtc
        } else {
            rune_media::ProcessingMode::Off
        };
        capture.configure(Some(rune_media::CaptureRequest {
            device: None,
            mode,
            muted: false,
            // Cancels what this machine plays, through the default
            // output's loopback; nothing of it is kept or printed but levels.
            echo_cancellation: flag("--system-echo"),
            automatic_gain: flag("--agc"),
            monitor: false,
            speech_gate: flag("--gate"),
            system_echo: flag("--system-echo"),
            output: None,
        }));
        println!("midiendo el micrófono predeterminado ({mode:?})");
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let sample = capture.snapshot();
            println!(
                "{:?}: entrada {:.2}, procesado {:.2}, descartadas {}, eco {} (referencia del sistema: {})",
                sample.state,
                sample.raw_level,
                sample.processed_level,
                sample.dropped_samples,
                sample.echo_cancelling,
                sample.system_echo
            );
        }
    }
    Ok(())
}
