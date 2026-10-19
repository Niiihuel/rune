//! Installing a release for this person, without administrator rights.
//!
//! Linux: the installation is a folder, `$XDG_DATA_HOME/rune/app`
//! (`~/.local/share/rune/app`), with each version in `versions/{version}/rune`
//! and `current`, a link to the version that starts. A release is unpacked
//! beside the others under a temporary name, renamed into place, and made
//! current by renaming a new link over `current`: a start finds the old
//! version or the new one, never neither. The version before stays, so the
//! one still running is never deleted under it; older ones go.
//!
//! Windows: the release is a per-user installer, run silently with
//! [`INSTALLER_ARGUMENTS`] while RUNE exits; it installs into
//! `%LOCALAPPDATA%\Programs\RUNE` and starts RUNE again
//! (`docs/UPDATES.md#windows`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

// What the Linux installation unpacks (windows-base: unused on Windows).
#[cfg(unix)]
use crate::Version;

/// The program's file name in a version's folder.
#[cfg(not(windows))]
pub const PROGRAM: &str = "rune";
#[cfg(windows)]
pub const PROGRAM: &str = "rune.exe";

/// What the Windows installer is run with: silent, no prompts, no reboot,
/// closing a RUNE that still holds its files. The installer is per-user
/// (no administrator rights) and starts RUNE when it is done.
pub const INSTALLER_ARGUMENTS: [&str; 4] = [
    "/VERYSILENT",
    "/SUPPRESSMSGBOXES",
    "/NORESTART",
    "/CLOSEAPPLICATIONS",
];

/// Where RUNE is installed for this person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Install {
    root: PathBuf,
}

impl Install {
    /// `RUNE_UPDATE_ROOT` when set (a test installation), or this
    /// person's: `$XDG_DATA_HOME/rune/app` on Linux,
    /// `%LOCALAPPDATA%\Programs\RUNE` on Windows.
    pub fn locate() -> Option<Self> {
        if let Some(root) = std::env::var_os("RUNE_UPDATE_ROOT").filter(|root| !root.is_empty()) {
            return Some(Self::at(root));
        }
        #[cfg(windows)]
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
            .join("Programs")
            .join("RUNE");
        #[cfg(not(windows))]
        let root = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|base| base.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })?
            .join("rune")
            .join("app");
        Some(Self::at(root))
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether `program`, this one as `current_exe` names it (links
    /// resolved), runs from this installation.
    pub fn runs(&self, program: &Path) -> bool {
        // Resolved as `program` is (and on Windows with its `\\?\` prefix).
        let root = fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        if cfg!(windows) {
            program.parent() == Some(root.as_path())
        } else {
            program.starts_with(root.join("versions"))
        }
    }

    /// Where downloads wait until they are installed.
    pub fn downloads(&self) -> PathBuf {
        if cfg!(windows) {
            std::env::temp_dir().join("rune-updates")
        } else {
            self.root.join("downloads")
        }
    }

    /// What a start runs: a path that stays the same across updates, for
    /// launchers and the session's autostart.
    pub fn program(&self) -> PathBuf {
        if cfg!(windows) {
            self.root.join(PROGRAM)
        } else {
            self.root.join("current").join(PROGRAM)
        }
    }

    /// The version `current` names, if any.
    pub fn current(&self) -> Option<String> {
        let target = fs::read_link(self.root.join("current")).ok()?;
        Some(target.file_name()?.to_str()?.to_owned())
    }

    /// Install `version` from the gzip-compressed program at `artifact`
    /// (the Linux release, verified already) and make it current. Returns
    /// [`Self::program`].
    #[cfg(unix)]
    pub fn apply(&self, artifact: &Path, version: &Version) -> io::Result<PathBuf> {
        self.place(version, |file| {
            let mut program = flate2::read::GzDecoder::new(fs::File::open(artifact)?);
            io::copy(&mut program, file)?;
            Ok(())
        })
    }

    /// Install `program`, this very file, as `version` (`rune --install`).
    #[cfg(unix)]
    pub fn adopt(&self, program: &Path, version: &Version) -> io::Result<PathBuf> {
        self.place(version, |file| {
            io::copy(&mut fs::File::open(program)?, file)?;
            Ok(())
        })
    }

    /// Write `version`'s program with `fill` under a temporary name, move
    /// it into place and make it current, or leave everything as it was.
    #[cfg(unix)]
    fn place(
        &self,
        version: &Version,
        fill: impl FnOnce(&mut fs::File) -> io::Result<()>,
    ) -> io::Result<PathBuf> {
        use std::io::{Read, Seek};
        use std::os::unix::fs::OpenOptionsExt;

        let name = version.to_string();
        if self.current().as_deref() == Some(name.as_str()) {
            return Ok(self.program());
        }
        let versions = self.root.join("versions");
        fs::create_dir_all(&versions)?;
        let staging = versions.join(format!(".{name}.{}", std::process::id()));
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir(&staging)?;
        let staged = (|| {
            let mut file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(0o755)
                .open(staging.join(PROGRAM))?;
            fill(&mut file)?;
            // A release for another system would never start: refuse it
            // before it replaces one that does.
            let mut magic = [0; 4];
            file.rewind()?;
            file.read_exact(&mut magic)?;
            if &magic != b"\x7fELF" {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "the release is not a Linux program",
                ));
            }
            file.sync_all()
        })();
        let target = versions.join(&name);
        let moved = staged.and_then(|()| {
            // A copy an earlier try left, never current.
            let _ = fs::remove_dir_all(&target);
            fs::rename(&staging, &target)
        });
        if let Err(error) = moved {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        let previous = self.current();
        let link = self.root.join("current.new");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(Path::new("versions").join(&name), &link)?;
        fs::rename(&link, self.root.join("current"))?;
        // The rename reaches the disk before anything is deleted.
        if let Ok(folder) = fs::File::open(&self.root) {
            let _ = folder.sync_all();
        }
        self.prune(&name, previous.as_deref());
        Ok(self.program())
    }

    /// Delete every version but `current` and `previous`, and what earlier
    /// tries left. Whatever cannot be deleted stays.
    #[cfg(unix)]
    fn prune(&self, current: &str, previous: Option<&str>) {
        let Ok(entries) = fs::read_dir(self.root.join("versions")) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name != current && Some(name.as_ref()) != previous {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
}

/// Start the Windows installer at `artifact` with [`INSTALLER_ARGUMENTS`],
/// once RUNE has closed; the caller exits right after.
pub fn run_installer(artifact: &Path) -> io::Result<()> {
    std::process::Command::new(artifact)
        .args(INSTALLER_ARGUMENTS)
        .spawn()
        .map(drop)
}
