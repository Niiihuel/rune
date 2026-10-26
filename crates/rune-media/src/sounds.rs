//! The person's own sounds: a folder they chose, one file per name
//! (`join.ogg`, `mute.wav`…), to play instead of RUNE's. Each is read and
//! decoded whole once, bounded in size and length, so what plays later is
//! known to play. This reads the disk: run it off the interface's thread.

use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rune_i18n::{t, tr};

use crate::clip::sound_length;
use crate::MediaError;

/// The extensions looked for, in this order: Ogg (Vorbis or Opus), Opus,
/// MP3 and WAV, which RUNE's decoders read.
pub const EXTENSIONS: [&str; 4] = ["ogg", "opus", "mp3", "wav"];
/// The largest file taken: 1 MiB, far more than a few seconds of sound.
pub const MAX_BYTES: u64 = 1 << 20;
/// The longest sound taken.
pub const MAX_LENGTH: Duration = Duration::from_secs(5);

/// What the folder has for one name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// No file by that name: RUNE's own plays.
    Missing,
    /// `file` (its name, no path), to play.
    Sound { file: String, bytes: Arc<[u8]> },
    /// `file` is there but does not do, and why: RUNE's own plays.
    Refused { file: String, reason: String },
}

/// For each of `names`, its file in `folder`: the first extension of
/// [`EXTENSIONS`] that is there. Fails only when the folder itself is not
/// one RUNE can read.
pub fn load(folder: &Path, names: &[&str]) -> Result<Vec<Found>, MediaError> {
    let unusable = || MediaError(t("RUNE no puede leer esa carpeta.").into());
    if !folder.is_absolute() {
        return Err(MediaError(
            t("Escribe la ruta completa de la carpeta, desde /.").into(),
        ));
    }
    if !std::fs::metadata(folder).map_err(|_| unusable())?.is_dir() {
        return Err(MediaError(t("Esa ruta no es una carpeta.").into()));
    }
    Ok(names
        .iter()
        .map(|name| {
            EXTENSIONS
                .iter()
                .map(|extension| format!("{name}.{extension}"))
                .find(|file| folder.join(file).exists())
                .map_or(Found::Missing, |file| match read(&folder.join(&file)) {
                    Ok(bytes) => Found::Sound { file, bytes },
                    Err(error) => Found::Refused {
                        file,
                        reason: error.0,
                    },
                })
        })
        .collect())
}

/// One file, if it is a short sound RUNE plays.
fn read(path: &Path) -> Result<Arc<[u8]>, MediaError> {
    let unreadable = || MediaError(t("no se pudo leer").into());
    let metadata = std::fs::metadata(path).map_err(|_| unreadable())?;
    if !metadata.is_file() {
        return Err(MediaError(t("no es un archivo").into()));
    }
    let too_big = || MediaError(tr!("pesa más de {size} KB", size = MAX_BYTES / 1024));
    if metadata.len() > MAX_BYTES {
        return Err(too_big());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| unreadable())?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unreadable())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(too_big());
    }
    let bytes: Arc<[u8]> = bytes.into();
    // A decoder that panics on a strange file refuses that file, as a clip
    // does, instead of ending the thread that reads the folder.
    let length = std::panic::catch_unwind(|| sound_length(&bytes, MAX_LENGTH))
        .unwrap_or_else(|_| Err(unreadable()));
    match length {
        Ok(Some(length)) if !length.is_zero() => Ok(bytes),
        Ok(Some(_)) => Err(MediaError(t("no tiene sonido").into())),
        Ok(None) => Err(MediaError(tr!(
            "dura más de {seconds} s",
            seconds = MAX_LENGTH.as_secs()
        ))),
        Err(_) => Err(MediaError(t("no es un sonido que RUNE reproduzca").into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own under the system's temporary one, gone after.
    struct Folder(std::path::PathBuf);

    impl Folder {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("rune-sounds-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A mono 16-bit WAV of `seconds` of a tone at 48 kHz.
    fn wav(seconds: f32) -> Vec<u8> {
        let samples = (seconds * 48_000.0) as u32;
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend((36 + samples * 2).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(48_000u32.to_le_bytes());
        bytes.extend((48_000u32 * 2).to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend((samples * 2).to_le_bytes());
        for n in 0..samples {
            let value = ((n as f32 * 0.06).sin() * 8_000.0) as i16;
            bytes.extend(value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn each_name_takes_its_first_playable_file_and_says_why_others_do_not() {
        let folder = Folder::new("load");
        let dir = &folder.0;
        std::fs::write(dir.join("join.wav"), wav(0.3)).unwrap();
        // The MP3 comes first when both are there.
        let mp3 = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/sounds/discord/mute.mp3"
        ))
        .unwrap();
        std::fs::write(dir.join("mute.mp3"), &mp3).unwrap();
        std::fs::write(dir.join("mute.wav"), wav(0.2)).unwrap();
        std::fs::write(dir.join("leave.wav"), wav(6.0)).unwrap();
        std::fs::write(dir.join("deafen.mp3"), b"not a sound at all").unwrap();
        std::fs::write(dir.join("message.wav"), vec![0; (MAX_BYTES + 1) as usize]).unwrap();
        std::fs::create_dir(dir.join("unmute.ogg")).unwrap();
        let names = [
            "join",
            "mute",
            "leave",
            "deafen",
            "message",
            "unmute",
            "user_join",
        ];
        let found = load(dir, &names).unwrap();
        assert!(matches!(&found[0], Found::Sound { file, .. } if file == "join.wav"));
        assert!(
            matches!(&found[1], Found::Sound { file, bytes } if file == "mute.mp3" && **bytes == *mp3)
        );
        let refused = |at: usize| match &found[at] {
            Found::Refused { file, reason } => format!("{file}: {reason}"),
            other => panic!("{other:?}"),
        };
        assert_eq!(refused(2), "leave.wav: dura más de 5 s");
        assert_eq!(
            refused(3),
            "deafen.mp3: no es un sonido que RUNE reproduzca"
        );
        assert_eq!(refused(4), "message.wav: pesa más de 1024 KB");
        assert_eq!(refused(5), "unmute.ogg: no es un archivo");
        assert_eq!(found[6], Found::Missing);
    }

    #[test]
    fn a_folder_that_is_not_one_is_refused_whole() {
        assert!(load(Path::new("relative/sounds"), &["join"]).is_err());
        let folder = Folder::new("file");
        let file = folder.0.join("join.wav");
        std::fs::write(&file, wav(0.2)).unwrap();
        assert!(load(&file, &["join"]).is_err());
        assert!(load(&folder.0.join("missing"), &["join"]).is_err());
    }
}
