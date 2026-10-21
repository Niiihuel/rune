//! The operating system's credential store, behind a small boundary so a
//! provider can remember a session across starts and tests use a fake.
//!
//! Values never appear in `Debug`, errors or logs. Without a store the
//! caller keeps its credential in memory only.

use rune_i18n::{es, t};
use std::{fmt, future::Future, pin::Pin, sync::Arc};

use zeroize::Zeroizing;

pub type SecretFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, SecretError>> + Send + 'a>>;

/// The service of the local store's keys, one entry per account it keeps
/// (owner decision 2026-10-18), apart from any provider's sessions.
pub const CACHE_KEYS: &str = "rune-cache";

/// One saved entry: the account it belongs to and the value that restores it.
pub struct Saved {
    pub account: String,
    pub secret: Zeroizing<String>,
}

impl fmt::Debug for Saved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Saved")
            .field("account", &self.account)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

/// A fixed, user-facing reason; never the store's own message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecretError(&'static str);

impl SecretError {
    pub const UNAVAILABLE: Self = Self(es("El llavero del sistema no está disponible."));
    pub const DENIED: Self = Self(es("No se desbloqueó el llavero del sistema."));
    pub const FAILED: Self = Self(es("El llavero del sistema no respondió."));
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(t(self.0))
    }
}

impl std::error::Error for SecretError {}

/// A remembered account without its secret: what a list of them shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub account: String,
    /// The account's shown name when it was saved; empty for older entries.
    pub name: String,
    /// What names the account's picture when it was saved (a provider's
    /// public image key, never a credential); empty when none or older.
    pub avatar: String,
}

/// Entries are keyed by `service` (a provider) and `account`; a service keeps
/// one entry per account.
pub trait SecretStore: Send + Sync {
    /// The accounts saved for `service`, the most recently saved first.
    /// Never reads a secret.
    fn list<'a>(&'a self, service: &'a str) -> SecretFuture<'a, Vec<Entry>>;
    /// The entry saved for `account` of `service`, if there is one.
    fn load<'a>(&'a self, service: &'a str, account: &'a str) -> SecretFuture<'a, Option<Saved>>;
    /// Save `secret`, the shown `name` and the `avatar` key for `account`,
    /// replacing only its own entry; it becomes the most recently saved. The
    /// name and the avatar are attributes anyone with the store may read.
    fn save<'a>(
        &'a self,
        service: &'a str,
        account: &'a str,
        name: &'a str,
        avatar: &'a str,
        secret: &'a str,
    ) -> SecretFuture<'a, ()>;
    /// Delete `account`'s entry of `service`; a missing entry is not an error.
    fn delete<'a>(&'a self, service: &'a str, account: &'a str) -> SecretFuture<'a, ()>;
}

/// No credential store: everything fails, so callers stay in memory.
pub struct Unavailable;

impl SecretStore for Unavailable {
    fn list<'a>(&'a self, _: &'a str) -> SecretFuture<'a, Vec<Entry>> {
        Box::pin(async { Err(SecretError::UNAVAILABLE) })
    }

    fn load<'a>(&'a self, _: &'a str, _: &'a str) -> SecretFuture<'a, Option<Saved>> {
        Box::pin(async { Err(SecretError::UNAVAILABLE) })
    }

    fn save<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> SecretFuture<'a, ()> {
        Box::pin(async { Err(SecretError::UNAVAILABLE) })
    }

    fn delete<'a>(&'a self, _: &'a str, _: &'a str) -> SecretFuture<'a, ()> {
        Box::pin(async { Err(SecretError::UNAVAILABLE) })
    }
}

/// The store of this system: Secret Service on Linux, the Credential
/// Manager on Windows, none elsewhere.
pub fn system() -> Arc<dyn SecretStore> {
    #[cfg(target_os = "linux")]
    {
        Arc::new(linux::SecretService::session())
    }
    #[cfg(windows)]
    {
        Arc::new(self::windows::CredentialManager)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        Arc::new(Unavailable)
    }
}

#[cfg(windows)]
pub use self::windows::CredentialManager;
#[cfg(target_os = "linux")]
pub use linux::SecretService;

#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
mod linux {
    use std::{collections::HashMap, time::Duration};

    use rune_i18n::tr;
    use secret_service::{EncryptionType, Error, Item};
    use zeroize::Zeroizing;

    use super::{Entry, Saved, SecretError, SecretFuture, SecretStore};

    const APPLICATION: &str = "rune";
    // Calls are bounded; an unlock prompt waits for the person.
    const CALL_TIMEOUT: Duration = Duration::from_secs(10);

    /// The freedesktop Secret Service, over an encrypted (DH) session.
    pub struct SecretService {
        /// `None` is the session bus, read at each call.
        address: Option<String>,
    }

    impl SecretService {
        pub fn session() -> Self {
            Self { address: None }
        }

        /// A specific bus, for tests on a private one.
        #[cfg(test)]
        pub fn at(address: impl Into<String>) -> Self {
            Self {
                address: Some(address.into()),
            }
        }

        async fn connect(&self) -> Result<secret_service::SecretService<'static>, SecretError> {
            let builder = match &self.address {
                Some(address) => zbus::connection::Builder::address(address.as_str()),
                None => zbus::connection::Builder::session(),
            }
            .map_err(|_| SecretError::UNAVAILABLE)?
            .method_timeout(CALL_TIMEOUT);
            let connection = tokio::time::timeout(CALL_TIMEOUT, builder.build())
                .await
                .map_err(|_| SecretError::UNAVAILABLE)?
                .map_err(|_| SecretError::UNAVAILABLE)?;
            secret_service::SecretService::connect_with_existing(EncryptionType::Dh, connection)
                .await
                .map_err(error)
        }
    }

    fn error(error: Error) -> SecretError {
        match error {
            Error::Prompt | Error::PromptDisconnected | Error::Locked => SecretError::DENIED,
            Error::Unavailable => SecretError::UNAVAILABLE,
            // A missing service name on the bus is "unavailable", not a fault.
            Error::Zbus(zbus::Error::MethodError(..)) | Error::ZbusFdo(_) => {
                SecretError::UNAVAILABLE
            }
            _ => SecretError::FAILED,
        }
    }

    async fn unlocked(item: &Item<'_>) -> Result<(), SecretError> {
        if item.is_locked().await.map_err(error)? {
            item.unlock().await.map_err(error)?;
        }
        Ok(())
    }

    async fn search<'a>(
        service: &'a secret_service::SecretService<'_>,
        attributes: HashMap<&str, &str>,
    ) -> Result<Vec<Item<'a>>, SecretError> {
        let found = service.search_items(attributes).await.map_err(error)?;
        Ok(found.unlocked.into_iter().chain(found.locked).collect())
    }

    impl SecretStore for SecretService {
        fn list<'a>(&'a self, service: &'a str) -> SecretFuture<'a, Vec<Entry>> {
            Box::pin(async move {
                let bus = self.connect().await?;
                let mut entries = Vec::new();
                for item in search(
                    &bus,
                    HashMap::from([("application", APPLICATION), ("service", service)]),
                )
                .await?
                {
                    // Attributes of a locked item may not be readable.
                    unlocked(&item).await?;
                    let mut attributes = item.get_attributes().await.map_err(error)?;
                    let Some(account) = attributes.remove("account") else {
                        continue;
                    };
                    // Every save creates the entry anew: its creation is its last use.
                    let created = item.get_created().await.map_err(error)?;
                    let name = attributes.remove("name").unwrap_or_default();
                    let avatar = attributes.remove("avatar").unwrap_or_default();
                    entries.push((
                        created,
                        Entry {
                            account,
                            name,
                            avatar,
                        },
                    ));
                }
                entries.sort_by_key(|(created, _)| std::cmp::Reverse(*created));
                Ok(entries.into_iter().map(|(_, entry)| entry).collect())
            })
        }

        fn load<'a>(
            &'a self,
            service: &'a str,
            account: &'a str,
        ) -> SecretFuture<'a, Option<Saved>> {
            Box::pin(async move {
                let bus = self.connect().await?;
                let items = search(
                    &bus,
                    HashMap::from([
                        ("application", APPLICATION),
                        ("service", service),
                        ("account", account),
                    ]),
                )
                .await?;
                let Some(item) = items.first() else {
                    return Ok(None);
                };
                unlocked(item).await?;
                let bytes = Zeroizing::new(item.get_secret().await.map_err(error)?);
                let secret = std::str::from_utf8(&bytes).map_err(|_| SecretError::FAILED)?;
                Ok(Some(Saved {
                    account: account.to_owned(),
                    secret: Zeroizing::new(secret.to_owned()),
                }))
            })
        }

        fn save<'a>(
            &'a self,
            service: &'a str,
            account: &'a str,
            name: &'a str,
            avatar: &'a str,
            secret: &'a str,
        ) -> SecretFuture<'a, ()> {
            Box::pin(async move {
                let bus = self.connect().await?;
                let collection = bus.get_default_collection().await.map_err(error)?;
                if collection.is_locked().await.map_err(error)? {
                    collection.unlock().await.map_err(error)?;
                }
                let lookup = HashMap::from([
                    ("application", APPLICATION),
                    ("service", service),
                    ("account", account),
                ]);
                let mut attributes = lookup.clone();
                attributes.insert("name", name);
                if !avatar.is_empty() {
                    attributes.insert("avatar", avatar);
                }
                // Shown by keyring managers, in the language it was saved in.
                let label = if service == super::CACHE_KEYS {
                    tr!("RUNE: clave de los datos locales ({account})", account)
                } else {
                    tr!("RUNE: sesión guardada de {name} ({service})", name, service)
                };
                // Created before the old entry goes: a failure keeps the old one.
                let created = collection
                    .create_item(&label, attributes, secret.as_bytes(), true, "text/plain")
                    .await
                    .map_err(error)?;
                for item in search(&bus, lookup).await? {
                    if item.item_path != created.item_path {
                        unlocked(&item).await?;
                        item.delete().await.map_err(error)?;
                    }
                }
                Ok(())
            })
        }

        fn delete<'a>(&'a self, service: &'a str, account: &'a str) -> SecretFuture<'a, ()> {
            Box::pin(async move {
                let bus = self.connect().await?;
                for item in search(
                    &bus,
                    HashMap::from([
                        ("application", APPLICATION),
                        ("service", service),
                        ("account", account),
                    ]),
                )
                .await?
                {
                    unlocked(&item).await?;
                    item.delete().await.map_err(error)?;
                }
                Ok(())
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::portal::tests::bus::Bus;

        // A private bus without a Secret Service: never the person's keyring.
        #[tokio::test]
        async fn a_bus_without_a_secret_service_is_unavailable() {
            let mut bus = Bus::start().await;
            let store = SecretService::at(bus.address.clone());
            assert_eq!(
                store.list("test").await.unwrap_err(),
                SecretError::UNAVAILABLE
            );
            assert_eq!(
                store.load("test", "1").await.unwrap_err(),
                SecretError::UNAVAILABLE
            );
            assert_eq!(
                store
                    .save("test", "1", "Nombre", "0123abcd", "SYNTHETIC_SECRET_VALUE")
                    .await
                    .unwrap_err(),
                SecretError::UNAVAILABLE
            );
            assert_eq!(
                store.delete("test", "1").await.unwrap_err(),
                SecretError::UNAVAILABLE
            );
            bus.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_entries_are_redacted_in_diagnostics() {
        let saved = Saved {
            account: "42".into(),
            secret: Zeroizing::new("SYNTHETIC_SECRET_MARKER".into()),
        };
        let shown = format!("{saved:?}");
        assert!(shown.contains("42") && !shown.contains("SYNTHETIC_SECRET_MARKER"));
    }

    #[tokio::test]
    async fn without_a_store_every_call_fails() {
        let store = Unavailable;
        assert!(store.list("test").await.is_err());
        assert!(store.load("test", "1").await.is_err());
        assert!(store.save("test", "1", "n", "", "x").await.is_err());
        assert!(store.delete("test", "1").await.is_err());
    }
}
