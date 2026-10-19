//! The local store's key of each account (owner decision 2026-10-18): made
//! at random the first time, kept in the system keyring, never in the file,
//! and deleted when the account is forgotten. Asked off the interface's
//! thread; without a keyring the account's data stays in memory.

use std::sync::Arc;
use std::time::Duration;

use rune_platform::secrets::{SecretError, SecretStore, CACHE_KEYS};
use rune_storage::Key;

/// Bounds a keyring that does not answer; an unlock prompt waits this long.
const TIMEOUT: Duration = Duration::from_secs(30);

/// What the keyring answered about an account, by its name in the store.
pub(crate) enum KeyEvent {
    /// The key, and whether it was made just now.
    Fetched(String, Result<(Key, bool), SecretError>),
    Deleted(Result<(), SecretError>),
}

/// `account`'s key, made and saved the first time, and whether it was made
/// now. A keyring that fails gives none: a new one would make the rows kept
/// so far unreadable.
pub(crate) async fn fetch(
    keyring: Arc<dyn SecretStore>,
    account: String,
) -> Result<(Key, bool), SecretError> {
    let fetch = async {
        let saved = keyring.load(CACHE_KEYS, &account).await?;
        if let Some(key) = saved.and_then(|saved| Key::from_text(&saved.secret)) {
            return Ok((key, false));
        }
        // None yet, or one that no longer reads: what it sealed is lost
        // either way, and the store drops it on unlocking with this one.
        let key = Key::generate().ok_or(SecretError::FAILED)?;
        keyring
            .save(CACHE_KEYS, &account, "", "", &key.to_text())
            .await?;
        Ok((key, true))
    };
    tokio::time::timeout(TIMEOUT, fetch)
        .await
        .unwrap_or(Err(SecretError::FAILED))
}

pub(crate) async fn delete(
    keyring: Arc<dyn SecretStore>,
    account: &str,
) -> Result<(), SecretError> {
    tokio::time::timeout(TIMEOUT, keyring.delete(CACHE_KEYS, account))
        .await
        .unwrap_or(Err(SecretError::FAILED))
}
