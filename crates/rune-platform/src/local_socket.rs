//! Unix sockets only this user reaches: in `$XDG_RUNTIME_DIR`, mode 0600,
//! and every peer checked to be the same user. Shared by the single
//! instance and the local Rich Presence server; never a TCP port.

use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

/// `$XDG_RUNTIME_DIR` when it is what the specification promises: an
/// absolute directory of this user that nobody else may enter. RUNE never
/// falls back to a shared directory such as `/tmp`.
pub(crate) fn runtime_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
    private_dir(&dir).then_some(dir)
}

pub(crate) fn private_dir(dir: &Path) -> bool {
    dir.is_absolute()
        && std::fs::metadata(dir)
            .is_ok_and(|meta| meta.is_dir() && meta.uid() == uid() && meta.mode() & 0o077 == 0)
}

pub(crate) fn uid() -> u32 {
    rustix::process::getuid().as_raw()
}

/// Whether the other end of `stream` runs as this user.
pub(crate) fn same_user(stream: &UnixStream) -> bool {
    rustix::net::sockopt::socket_peercred(stream).is_ok_and(|peer| peer.uid.as_raw() == uid())
}

/// What binding `path` found.
pub(crate) enum Bound {
    Listening(UnixListener),
    /// Something answers there already.
    Taken,
}

/// Listen at `path`, 0600. A socket of this user that nobody answers is a
/// leftover of a run that crashed: it is replaced. Anything else stays.
pub(crate) fn bind(path: &Path) -> io::Result<Bound> {
    match UnixListener::bind(path) {
        Ok(listener) => {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            Ok(Bound::Listening(listener))
        }
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => match UnixStream::connect(path) {
            Ok(_) => Ok(Bound::Taken),
            Err(refused) if refused.kind() == io::ErrorKind::ConnectionRefused => {
                let meta = std::fs::symlink_metadata(path)?;
                if !meta.file_type().is_socket() || meta.uid() != uid() {
                    return Err(error);
                }
                std::fs::remove_file(path)?;
                let listener = UnixListener::bind(path)?;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
                Ok(Bound::Listening(listener))
            }
            Err(_) => Err(error),
        },
        Err(error) => Err(error),
    }
}
