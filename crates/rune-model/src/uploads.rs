//! Ephemeral attachment references, independent of files and transports.

/// A process-local attachment grant. It never identifies a remote attachment.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UploadToken(pub u64);

/// The only selected-file data retained by presentation and domain state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAttachment {
    pub token: UploadToken,
    pub filename: String,
    pub size_bytes: u64,
}
