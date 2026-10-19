//! A group's new picture, chosen by the person: one image file through the
//! system's selector, read with a bound, cropped to a square and made a small
//! PNG on a service thread. Only that PNG travels, inside
//! `Command::SetGroupIcon`; neither the file nor its bytes reach the
//! interface. A refusal comes back as the group's failure.

use std::io::{Cursor, Read};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use image::{imageops::FilterType, ImageFormat, ImageReader};
use rune_i18n::{es, t};
use rune_model::ChannelId;
use rune_provider::{Command, Event, Failure, GroupIcon};
use tokio::{runtime::Handle, sync::mpsc, sync::watch, task::JoinHandle};

/// The largest picture file read.
const MAX_INPUT: u64 = 8 * 1024 * 1024;
/// The largest side decoded, and the most memory a decode may take.
const MAX_SIDE: u32 = 8192;
const MAX_ALLOC: u64 = 128 * 1024 * 1024;
/// The sides tried for the icon, largest first, until it fits.
const EDGES: [u32; 3] = [256, 160, 96];

pub(crate) struct GroupIcons {
    runtime: Handle,
    commands: mpsc::Sender<Command>,
    wake: Arc<dyn Fn() + Send + Sync>,
    /// The choice under way, and how to stop it.
    choosing: Option<(JoinHandle<()>, watch::Sender<bool>)>,
    failures: Arc<Mutex<Vec<Event>>>,
}

impl GroupIcons {
    pub fn new(
        runtime: Handle,
        commands: mpsc::Sender<Command>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            runtime,
            commands,
            wake,
            choosing: None,
            failures: Arc::default(),
        }
    }

    /// Let the person choose a picture for `channel`, the dialog over
    /// `parent`; one choice at a time.
    pub fn pick(&mut self, channel: ChannelId, parent: Option<crate::ParentWindow>) {
        if self
            .choosing
            .as_ref()
            .is_some_and(|(task, _)| !task.is_finished())
        {
            self.fail(channel, t("Ya se está eligiendo una imagen."));
            return;
        }
        let (cancel, cancelled) = watch::channel(false);
        let (commands, failures, wake) = (
            self.commands.clone(),
            self.failures.clone(),
            self.wake.clone(),
        );
        let task = self.runtime.spawn(async move {
            let result = match choose(cancelled, parent).await {
                Ok(Some(icon)) => commands
                    .try_send(Command::SetGroupIcon {
                        channel,
                        icon: Some(icon),
                    })
                    .map_err(|_| t("La cola de comandos está llena.")),
                Ok(None) => Ok(()),
                Err(reason) => Err(reason),
            };
            if let Err(reason) = result {
                if let Ok(mut failures) = failures.lock() {
                    failures.push(failure(channel, reason));
                }
            }
            wake();
        });
        self.choosing = Some((task, cancel));
    }

    /// Stop a choice under way: what it finds is dropped.
    pub fn cancel(&mut self) {
        if let Some((task, cancel)) = self.choosing.take() {
            cancel.send_replace(true);
            task.abort();
        }
    }

    /// What went wrong since the last call.
    pub fn take_failures(&mut self) -> Vec<Event> {
        self.failures
            .lock()
            .map(|mut failures| std::mem::take(&mut *failures))
            .unwrap_or_default()
    }

    fn fail(&self, channel: ChannelId, reason: &'static str) {
        if let Ok(mut failures) = self.failures.lock() {
            failures.push(failure(channel, reason));
        }
        (self.wake)();
    }
}

impl Drop for GroupIcons {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn failure(channel: ChannelId, reason: &str) -> Event {
    Event::Failed {
        failure: Failure::Group { channel },
        reason: reason.into(),
        ambiguous: false,
    }
}

/// The chosen picture as an icon; `None` when the person cancelled.
async fn choose(
    cancel: watch::Receiver<bool>,
    parent: Option<crate::ParentWindow>,
) -> Result<Option<GroupIcon>, &'static str> {
    let Some(bytes) = chosen_bytes(cancel, parent).await? else {
        return Ok(None);
    };
    tokio::task::spawn_blocking(move || prepare(&bytes).map(Some))
        .await
        .map_err(|_| t("No se pudo preparar la imagen."))?
}

/// The bytes of one picture file the person chooses through the system's
/// selector over `parent`, read with a bound; `None` when the person
/// cancelled.
#[cfg(any(target_os = "linux", windows))]
pub(crate) async fn chosen_bytes(
    cancel: watch::Receiver<bool>,
    parent: Option<crate::ParentWindow>,
) -> Result<Option<Vec<u8>>, &'static str> {
    use rune_platform::file_dialogs::{FileChooser, PickError, PickId, PickRequest};
    let mut chooser = FileChooser::session().await.map_err(PickError::label)?;
    let request = PickRequest {
        id: PickId(1),
        max_files: 1,
        max_file_bytes: MAX_INPUT,
        max_total_bytes: MAX_INPUT,
        parent,
        current_folder: None,
    };
    let mut files = match chooser.choose_files(&request, cancel).await {
        Ok(files) => files,
        Err(PickError::Cancelled) => return Ok(None),
        Err(error) => return Err(error.label()),
    };
    let file = files.pop().ok_or(t("No se eligió ninguna imagen."))?;
    // Reading blocks: off the runtime's workers.
    tokio::task::spawn_blocking(move || {
        let (file, _, _) = file.into_parts();
        let mut bytes = Vec::new();
        file.take(MAX_INPUT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| t("No se pudo leer la imagen."))?;
        if bytes.len() as u64 > MAX_INPUT {
            return Err(t("La imagen pesa más de 8 MB."));
        }
        Ok(Some(bytes))
    })
    .await
    .map_err(|_| t("No se pudo leer la imagen."))?
}

#[cfg(not(any(target_os = "linux", windows)))]
pub(crate) async fn chosen_bytes(
    _cancel: watch::Receiver<bool>,
    _parent: Option<crate::ParentWindow>,
) -> Result<Option<Vec<u8>>, &'static str> {
    Err(t(
        "El selector de archivos no está disponible en este equipo.",
    ))
}

/// `bytes`, a PNG, JPEG, GIF or WebP picture, decoded within bounds.
pub(crate) fn decode(bytes: &[u8]) -> Result<image::DynamicImage, &'static str> {
    const UNREADABLE: &str = es("Elegí una imagen PNG, JPEG, GIF o WebP.");
    let format = image::guess_format(bytes).map_err(|_| t(UNREADABLE))?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::WebP
    ) {
        return Err(t(UNREADABLE));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|_| t("La imagen es demasiado grande o está dañada."))
}

/// `bytes`, a PNG, JPEG, GIF or WebP picture, as a group's icon: decoded
/// within bounds, cropped to its middle square and made a small PNG.
pub(crate) fn prepare(bytes: &[u8]) -> Result<GroupIcon, &'static str> {
    let decoded = decode(bytes)?;
    for edge in EDGES {
        let square = decoded.resize_to_fill(edge, edge, FilterType::Triangle);
        let mut png = Cursor::new(Vec::new());
        square
            .write_to(&mut png, ImageFormat::Png)
            .map_err(|_| t("No se pudo preparar la imagen."))?;
        if let Some(icon) = GroupIcon::new(Bytes::from(png.into_inner())) {
            return Ok(icon);
        }
    }
    Err(t("La imagen no entra en el tamaño de un ícono."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let picture = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x * y) % 256) as u8])
        });
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(picture)
            .write_to(&mut bytes, format)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn a_picture_becomes_a_square_png_within_the_icon_limit() {
        for format in [ImageFormat::Png, ImageFormat::Jpeg] {
            let icon = prepare(&encoded(640, 360, format)).unwrap();
            let png = icon.png();
            assert!(png.len() <= GroupIcon::MAX_BYTES);
            let (width, height) = ImageReader::with_format(Cursor::new(png), ImageFormat::Png)
                .into_dimensions()
                .unwrap();
            assert_eq!((width, height), (256, 256));
        }
    }

    #[test]
    fn anything_else_is_refused_with_a_reason() {
        assert_eq!(
            prepare(b"%PDF-1.7 not a picture").unwrap_err(),
            "Elegí una imagen PNG, JPEG, GIF o WebP."
        );
        let mut broken = encoded(64, 64, ImageFormat::Png);
        broken.truncate(40);
        assert!(prepare(&broken).is_err());
    }
}
