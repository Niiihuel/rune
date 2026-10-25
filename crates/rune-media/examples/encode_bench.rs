//! What encoding the shared screen costs: synthetic RGBA frames, all of
//! them changing, through `video::encode` (OpenH264, or the GPU's encoder
//! with `--hw`), at the size and rate asked, for a few seconds. Prints the
//! frames encoded, their pace and the CPU each thread of the process took
//! meanwhile, read from `/proc` (GStreamer's and the driver's threads too),
//! and their sum; then what watching them costs: the same units through
//! `video::decode` at the same pace, with the same engines, and its CPU.
//!
//! `cargo run --release -p rune-media --example encode_bench -- [height] [fps] [seconds] [kbit/s] [--hw|--va|--nvidia]`
//! (1080, 60, 10 and the quality's most by default; the shared screen
//! starts at the most too since 2026-10-17, at 600 before). `--hw` tries
//! VA-API and then NVENC/NVDEC, as RUNE does; `--va` and `--nvidia` only
//! that one; none of them, OpenH264 alone. Also prints what the encoder
//! counted: rate-control skips, key frames, ms a frame. Linux only, like
//! the encoder.

#[cfg(target_os = "linux")]
fn main() {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    use rune_media::video::encode::{Encoding, Engine, Settings, HARDWARE, MIN_BITRATE};
    use rune_media::video::Frame;

    let (flags, numbers): (Vec<String>, Vec<String>) = std::env::args()
        .skip(1)
        .partition(|arg| arg.starts_with("--"));
    let engines: &'static [Engine] = match flags.first().map(String::as_str) {
        Some("--hw") => HARDWARE,
        Some("--va") => &[Engine::VaApi],
        Some("--nvidia") => &[Engine::Nvidia],
        _ => &[],
    };
    let mut arguments = numbers.iter().map(|value| value.parse::<u32>());
    let height = arguments.next().and_then(Result::ok).unwrap_or(1080);
    let fps = arguments.next().and_then(Result::ok).unwrap_or(60).max(1);
    let seconds = arguments.next().and_then(Result::ok).unwrap_or(10).max(1);
    let start = arguments
        .next()
        .and_then(Result::ok)
        .map(|kbits| kbits * 1000);
    let width = (height * 16).div_ceil(9) & !1;
    // Made before the clock starts, so drawing them is not counted: bands
    // moving sideways and a bar sweeping, every pixel changing between two.
    let frames: Vec<Frame> = (0..30u32)
        .map(|at| {
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for y in 0..height {
                for x in 0..width {
                    let band = ((x + at * 7) / 80) % 6;
                    let shade = (40 + (y + at) * 150 / height) as u8;
                    let bar = (x + y + at * 13) % 97 < 4;
                    let pixel = match (bar, band) {
                        (true, _) => [240, 240, 240],
                        (_, 0) => [shade, 40, 60],
                        (_, 1) => [40, shade, 80],
                        (_, 2) => [50, 70, shade],
                        (_, 3) => [shade, shade, 40],
                        (_, 4) => [40, shade, shade],
                        _ => [shade, 50, shade],
                    };
                    rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                }
            }
            Frame {
                width,
                height,
                rgba,
            }
        })
        .collect();
    let pace = Duration::from_secs(1) / fps;
    let before = threads();
    let started = Instant::now();
    let mut next = started;
    let mut index = 0;
    let (sent, received) = std::sync::mpsc::channel();
    // The most a stream of this quality sends: as `StreamQuality::max_bitrate`.
    let max_bitrate = if height > 720 || (height == 720 && fps > 30) {
        9_000_000
    } else {
        3_500_000
    };
    let encoding = Encoding::start(
        Settings {
            fps,
            bitrate: start.unwrap_or(max_bitrate),
            min_bitrate: MIN_BITRATE,
            max_bitrate,
            camera: false,
            engines,
        },
        move || {
            let now = Instant::now();
            if now < next {
                return None;
            }
            next += pace;
            index += 1;
            // A copy of its own, as the capture makes one: counted in this thread.
            Some(frames[index % frames.len()].clone())
        },
        || true,
        move |unit| sent.send(unit).is_ok(),
        || None,
    );
    std::thread::sleep(Duration::from_secs(u64::from(seconds)));
    let after = threads();
    let wall = started.elapsed();
    let stats = encoding.stats();
    drop(encoding);
    let encoded: Vec<_> = received.try_iter().collect();
    let units = encoded.len() as f64;
    let bytes: usize = encoded.iter().map(|unit| unit.data.len()).sum();
    println!(
        "{width}×{height} a {fps} fps, {seconds} s, codificador {}{}: {units} unidades, {:.1} por segundo, {:.2} Mbit/s",
        stats.engine.encoder(),
        stats
            .failed
            .map(|failed| format!(" (falló {})", failed.encoder()))
            .unwrap_or_default(),
        units / wall.as_secs_f64(),
        bytes as f64 * 8.0 / wall.as_secs_f64() / 1e6,
    );
    let average = |key: bool| {
        let sizes: Vec<usize> = encoded
            .iter()
            .filter(|unit| unit.keyframe == key)
            .map(|unit| unit.data.len())
            .collect();
        sizes.iter().sum::<usize>() as f64 / 1000.0 / sizes.len().max(1) as f64
    };
    println!(
        "cuadros clave de {:.1} KB en promedio, los demás de {:.1} KB",
        average(true),
        average(false)
    );
    println!(
        "a {} kbit/s: {} codificadas, {} salteadas por el control de tasa, {} cuadros clave, {:.1} ms por cuadro (de entregarlo a tener su unidad)",
        start.unwrap_or(max_bitrate) / 1000,
        stats.encoded,
        stats.skipped,
        stats.keyframes,
        stats.encoding_micros as f64 / 1000.0 / (stats.encoded + stats.skipped).max(1) as f64,
    );
    cpu("Codificar", &before, &after, wall, units);

    // Watching it: the units handed over at the pace they were made, each
    // fresh when it arrives, decoded and converted to RGBA as a stream is.
    let mut pending = encoded.into_iter();
    let mut next = Instant::now();
    let before = threads();
    let decoding = rune_media::video::decode::Decoding::start(
        engines,
        move || {
            let now = Instant::now();
            if now < next {
                return None;
            }
            next += pace;
            pending.next().map(|mut unit| {
                unit.at = now;
                unit
            })
        },
        || {},
        || {},
    );
    let started = Instant::now();
    let mut shown = 0u32;
    while started.elapsed() < wall {
        shown += u32::from(decoding.take_frame().is_some());
        std::thread::sleep(Duration::from_millis(2));
    }
    let after = threads();
    let watched = started.elapsed();
    let decoder = decoding.engine();
    println!(
        "Ver la transmisión, decodificador {}{}: {shown} cuadros mostrados",
        decoder.engine.decoder(),
        decoder
            .failed
            .map(|failed| format!(" (falló {})", failed.decoder()))
            .unwrap_or_default(),
    );
    cpu("Decodificar", &before, &after, watched, f64::from(shown));
    drop(decoding);
    rune_media::video::gpu::settle(Duration::from_secs(1));

    /// Each thread of this process by id: its name (the first 15 bytes, as
    /// `/proc` keeps them) and its user and system time, in clock ticks of 10 ms.
    fn threads() -> HashMap<u32, (String, u64)> {
        let mut found = HashMap::new();
        let Ok(entries) = std::fs::read_dir("/proc/self/task") else {
            return found;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let id = path
                .file_name()
                .and_then(|name| name.to_str()?.parse().ok());
            let comm = std::fs::read_to_string(path.join("comm"));
            let stat = std::fs::read_to_string(path.join("stat"));
            let (Some(id), Ok(comm), Ok(stat)) = (id, comm, stat) else {
                continue;
            };
            // After the name in parentheses, utime and stime are the 12th
            // and 13th fields.
            let Some((_, fields)) = stat.rsplit_once(')') else {
                continue;
            };
            let fields: Vec<&str> = fields.split_whitespace().collect();
            let ticks = |at: usize| fields.get(at).and_then(|value| value.parse::<u64>().ok());
            if let (Some(user), Some(system)) = (ticks(11), ticks(12)) {
                found.insert(id, (comm.trim().to_owned(), user + system));
            }
        }
        found
    }

    /// The CPU each thread took between `before` and `after` (the main one
    /// left out: it only waits), most first, and their sum.
    fn cpu(
        what: &str,
        before: &HashMap<u32, (String, u64)>,
        after: &HashMap<u32, (String, u64)>,
        wall: Duration,
        pictures: f64,
    ) {
        let main = std::process::id();
        let mut spent: Vec<(String, u64)> = after
            .iter()
            .filter(|(id, _)| **id != main)
            .map(|(id, (name, ticks))| {
                let then = before.get(id).map_or(0, |(_, then)| *then);
                (name.clone(), ticks.saturating_sub(then))
            })
            .filter(|(_, ticks)| *ticks > 0)
            .collect();
        spent.sort_by_key(|(_, ticks)| std::cmp::Reverse(*ticks));
        let share = |ticks: u64| ticks as f64 * 10.0 / wall.as_millis().max(1) as f64 * 100.0;
        let total: u64 = spent.iter().map(|(_, ticks)| ticks).sum();
        let each: Vec<String> = spent
            .iter()
            .map(|(name, ticks)| format!("{name} {:.0} %", share(*ticks)))
            .collect();
        println!(
            "{what}: {:.0} % de un núcleo entre todos los hilos, {:.1} ms por cuadro; por hilo: {}",
            share(total),
            total as f64 * 10.0 / pictures.max(1.0),
            if each.is_empty() {
                "ninguno".to_owned()
            } else {
                each.join(", ")
            },
        );
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    println!("El codificador de pantalla sólo existe en Linux por ahora.");
}
