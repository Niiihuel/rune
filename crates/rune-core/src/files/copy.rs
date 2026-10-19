//! A picture or a video of a message copied to the system clipboard:
//! fetched as a download is, into a folder only this user enters, then its
//! pixels or the file itself handed to the clipboard (contenido-de-mensajes).

use std::path::{Path, PathBuf};

use rune_i18n::t;

/// What a copy puts on the clipboard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Copied {
    /// A picture's pixels, for the programs that paste images.
    Image,
    /// The file itself, for those that paste files.
    File,
}

/// The widest and tallest picture copied, as previews are bounded.
const MAX_SIDE: u32 = 4096;

pub(super) async fn copy_to(
    client: &reqwest::Client,
    url: url::Url,
    filename: &str,
    size: u64,
    copied: Copied,
) -> Result<PathBuf, String> {
    let directory = tokio::task::spawn_blocking(rune_platform::clipboard::copies_dir)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| t("No hay una carpeta privada para el archivo copiado.").to_owned())?;
    let partial = super::fetch(client, url, &directory, size, &|_| {}).await?;
    let filename = filename.to_owned();
    let clipboard = |reason: &'static str| t(reason).to_owned();
    // The partial file goes with `partial`, at the end; a kept file stays.
    tokio::task::spawn_blocking(move || match copied {
        Copied::Image => {
            let (width, height, rgba) = pixels(&partial.0)?;
            rune_platform::clipboard::write_image(width, height, &rgba).map_err(clipboard)?;
            Ok(directory.join(&filename))
        }
        Copied::File => {
            let kept = keep(&directory, &partial.0, &filename)?;
            rune_platform::clipboard::write_file(&kept).map_err(clipboard)?;
            Ok(kept)
        }
    })
    .await
    .map_err(|_| t("No se pudo copiar el archivo.").to_owned())?
}

/// The pixels of the picture at `path`, as the clipboard takes them; one
/// past [`MAX_SIDE`] a side, damaged or of another kind is refused.
pub(super) fn pixels(path: &Path) -> Result<(usize, usize, Vec<u8>), String> {
    let mut reader = image::ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|_| t("No se pudo leer la imagen.").to_owned())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(80 * 1024 * 1024);
    reader.limits(limits);
    let picture = reader
        .decode()
        .map_err(|_| {
            t("No se pudo copiar la imagen: es demasiado grande, está dañada o no es una imagen.")
                .to_owned()
        })?
        .into_rgba8();
    let (width, height) = (picture.width() as usize, picture.height() as usize);
    Ok((width, height, picture.into_raw()))
}

/// Keep the fetched file at `directory/filename` while it is on the
/// clipboard, in place of the file copied before; files still arriving
/// (`.rune-*.part`, as `fetch` names them) stay.
pub(super) fn keep(directory: &Path, partial: &Path, filename: &str) -> Result<PathBuf, String> {
    for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with(".rune-") && name.ends_with(".part")) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let kept = directory.join(filename);
    std::fs::hard_link(partial, &kept)
        .map_err(|_| t("No se pudo preparar el archivo copiado.").to_owned())?;
    Ok(kept)
}
