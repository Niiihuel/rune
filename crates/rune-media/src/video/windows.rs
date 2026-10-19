//! The screen and the cameras on Windows (windows-medios). Windows has no
//! portal: RUNE's own picker offers the screens and windows ([`sources`]),
//! Windows.Graphics.Capture reads the one chosen ([`screen`]) and Media
//! Foundation a camera ([`camera`]), each on a thread of its own, into the
//! same queues, at the same sizes and rates as on Linux. Nothing is read
//! before the person asks; dropping the [`Video`] stops the thread within
//! a frame or a check.

use std::sync::Arc;
use std::time::Duration;

use rune_i18n::{es, t};
use rune_platform::file_dialogs::ParentWindow;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use super::{sending, Shared, Video};

mod camera;
/// No GPU's encoder or decoder on Windows yet: OpenH264 does it.
pub mod gpu;
mod screen;

pub use screen::sources;

/// How often the stop flag and the deadlines are checked.
const CHECK: Duration = Duration::from_millis(50);
/// A source that has not sent a frame by then is not going to.
const FIRST_FRAME: Duration = Duration::from_secs(15);

const UNAVAILABLE: &str = es("No se pudo iniciar la vista previa.");
const NO_FRAMES: &str = es("La fuente no envió imágenes.");
const GONE: &str = es("La fuente dejó de estar disponible.");

impl Video {
    /// Share a screen or a window: the one RUNE's picker chose
    /// ([`sources`]), by its id, or the main screen when none was.
    /// Windows has no dialog of its own to parent.
    pub fn screen(_parent: Option<ParentWindow>, chosen: Option<String>) -> Self {
        spawn(None, move |shared| screen::read(chosen.as_deref(), shared))
    }

    /// [`Video::screen`], to send: kept within `max` pixels, at most `fps`
    /// a second, for [`Video::frames`]; previewed as well.
    pub fn share(
        _parent: Option<ParentWindow>,
        chosen: Option<String>,
        max: (u32, u32),
        fps: u32,
    ) -> Self {
        spawn(Some(sending(max, fps)), move |shared| {
            screen::read(chosen.as_deref(), shared)
        })
    }

    /// List the cameras and preview `device`, one of the
    /// [`super::CameraInfo::id`]s listed before, or the first one.
    pub fn camera(device: Option<String>) -> Self {
        spawn(None, move |shared| camera::read(device.as_deref(), shared))
    }

    /// [`Video::camera`], to send: kept within `max` pixels, at most `fps`
    /// a second, for [`Video::frames`]; previewed as well.
    pub fn send_camera(device: Option<String>, max: (u32, u32), fps: u32) -> Self {
        spawn(Some(sending(max, fps)), move |shared| {
            camera::read(device.as_deref(), shared)
        })
    }
}

/// `read` on a thread of its own until it returns: its error is why the
/// source ended.
fn spawn(
    sending: Option<((u32, u32), Duration)>,
    read: impl FnOnce(&Arc<Shared>) -> Result<(), String> + Send + 'static,
) -> Video {
    let shared = Arc::new(Shared {
        sending,
        ..Shared::new()
    });
    let worker = shared.clone();
    let spawned = std::thread::Builder::new()
        .name("rune-video".into())
        .spawn(move || {
            let result = read(&worker);
            worker.end(result.err());
        });
    if spawned.is_err() {
        shared.end(Some(t(UNAVAILABLE).into()));
    }
    Video { shared }
}

/// COM on this thread, multithreaded, as Media Foundation, WASAPI and
/// Windows.Graphics.Capture want it, until dropped.
pub(super) struct Com;

impl Com {
    pub(super) fn start() -> Result<Self, String> {
        // SAFETY: once on this thread, undone once by the drop.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map(|()| Self)
            .map_err(|_| t(UNAVAILABLE).to_owned())
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: after every COM object of this thread was let go.
        unsafe { CoUninitialize() };
    }
}

/// Up to `limit` characters of `name`, without control characters.
fn bounded(name: &str, limit: usize) -> String {
    name.chars()
        .filter(|character| !character.is_control())
        .take(limit)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::super::{VideoSnapshot, VideoState};
    use super::*;

    /// What `video` got to within a while: running with a frame, or ended.
    fn settled(video: &Video) -> (VideoSnapshot, bool) {
        let started = Instant::now();
        loop {
            let snapshot = video.snapshot();
            let framed = video.take_frame().is_some();
            if framed || matches!(snapshot.state, VideoState::Ended(_)) {
                return (snapshot, framed);
            }
            assert!(started.elapsed() < Duration::from_secs(20), "{snapshot:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    // This computer's screen, camera and sound: a frame, a picture, or why
    // not (Wine has no Windows.Graphics.Capture, cameras or loopback).
    #[test]
    #[ignore = "captures this computer's screen and camera"]
    fn the_screen_and_the_camera_give_a_frame_or_say_why() {
        for video in [Video::screen(None, None), Video::camera(None)] {
            let (snapshot, framed) = settled(&video);
            eprintln!("{snapshot:?}, frame: {framed}");
            assert!(framed || matches!(snapshot.state, VideoState::Ended(Some(_))));
        }
        let sound = super::super::sound::ScreenSound::start(|_| true);
        std::thread::sleep(Duration::from_secs(1));
        eprintln!("sound: {:?}", sound.state());
    }

    #[test]
    fn names_are_bounded_and_printable() {
        assert_eq!(bounded(" Juego\u{7} — Nivel 3 ", 64), "Juego — Nivel 3");
        assert_eq!(bounded(&"x".repeat(300), 128).len(), 128);
        assert_eq!(bounded("ñandú", 3), "ñan");
    }
}
