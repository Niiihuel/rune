//! Opens the system's camera through the portal for a few seconds and
//! prints what PipeWire negotiated and how many frames arrived; pass a
//! camera id to pick one. Frames are counted and dropped: nothing is saved
//! or shown. `cargo run -p rune-media --example camera`

fn main() {
    let video = rune_media::video::Video::camera(std::env::args().nth(1));
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(8) {
        std::thread::sleep(std::time::Duration::from_millis(250));
        while video.take_frame().is_some() {}
        if let rune_media::video::VideoState::Ended(reason) = video.snapshot().state {
            println!("terminó: {reason:?}");
            break;
        }
    }
    let snapshot = video.snapshot();
    for camera in &snapshot.cameras {
        println!("cámara: {} [{}]", camera.name, camera.id);
    }
    println!("cuadros: {}", snapshot.frames);
    println!("{:?}", video.capture_stats());
}
