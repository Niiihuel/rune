//! RUNE's local store, in SQLite: the newest messages of each conversation,
//! so a conversation opens at once while the provider is asked; the last
//! session's snapshot, so the window shows servers and channels before the
//! next session is ready; drafts, with where their files are (never the
//! files themselves); and settings. Every row but the settings
//! belongs to an account, so two providers never mix. It is synchronous: the
//! core runs it on a thread of its own, away from the interface.
//!
//! An account's rows are sealed with that account's [`Key`], which the
//! caller keeps in the system keyring: messages, drafts and where their
//! files are, the snapshot and the reading marks. Without [`Storage::unlock`] they can be neither
//! written nor read ([`Error::Locked`]). What stays in plain text is what
//! finds the rows: the account's name in the store, conversation and
//! message ids, and the settings, which hold no content. On Unix the file
//! and its directory are readable by their owner only. Deleted rows are
//! overwritten (`secure_delete`), and the file cannot grow past
//! [`MAX_BYTES`]. See docs/SECURITY.md.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rune_i18n::{t, tr};
use rune_model::{ChannelId, Message, MessageId, MessagePatch, Tally};
use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;
use zeroize::Zeroizing;

mod message_search;
mod read_marks;
mod sealed;
pub use message_search::SearchChunk;
pub use sealed::Key;
// Pictures on disk (imagenes-y-lectura): what sealing adds to a file.
use sealed::Place;
pub use sealed::OVERHEAD as SEALED_OVERHEAD;
// Sends and uploads (envios-y-subidas).
mod draft_files;
pub use draft_files::DraftFile;

/// The schema this code writes. Older files are migrated forward on open;
/// a newer one is left alone, so an older RUNE never writes plain text
/// into a sealed store.
const SCHEMA: i32 = 6;
/// Recent explicit deletions kept per conversation, just like the live timeline.
const TOMBSTONES: usize = 256;
/// The most the file may hold. Writes past it fail, and the store keeps
/// working with what it has.
pub const MAX_BYTES: i64 = 128 * 1024 * 1024;
/// Bound the account-local reading choices restored at startup.
pub const MAX_READ_MARKS: usize = 65_536;
/// Settings read back at startup: the device's and a few per account.
pub const MAX_SETTINGS: usize = 4096;

/// Confirmed message changes, applied atomically in the order received.
#[derive(Clone, Debug)]
pub enum MessageMutation {
    Upsert(Box<Message>),
    /// A partial change only applies to an already retained confirmed row.
    Patch(Box<MessagePatch>),
    /// A provider page is authoritative for the range it covers, oldest first.
    History {
        channel: ChannelId,
        before: Option<MessageId>,
        messages: Vec<Message>,
        reached_start: bool,
    },
    Delete {
        channel: ChannelId,
        message: MessageId,
    },
    /// Messages a search of the whole history found. They join no
    /// conversation's run of history: they are kept apart, so a page's
    /// bounds never take them for neighbors, and only searches read them.
    Found(Vec<Message>),
    // Reactions, pins and search (2026-10-18).
    /// A live reaction or vote, applied like a patch to a retained row.
    Tally {
        channel: ChannelId,
        message: MessageId,
        change: Tally,
    },
}

/// Why the store could not be used.
#[derive(Debug)]
pub enum Error {
    Sqlite(rusqlite::Error),
    /// Local reading choices exceed the bounded startup restore.
    ReadMarksTooLarge,
    /// A stored local reading choice cannot be decoded safely.
    InvalidReadMark,
    /// The file was written by a newer RUNE. It is left untouched.
    Newer {
        found: i32,
        supported: i32,
    },
    /// The account's key is not here: nothing of it is written or read.
    Locked,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(error) => error.fmt(formatter),
            Self::ReadMarksTooLarge => formatter.write_str(t(
                "Los marcadores de lectura superan el límite de restauración local.",
            )),
            Self::InvalidReadMark => {
                formatter.write_str(t("No se pudieron leer los marcadores locales de lectura."))
            }
            Self::Newer { found, supported } => formatter.write_str(&tr!(
                "la base local es del esquema {found} y esta versión de RUNE entiende hasta el {supported}",
                found,
                supported
            )),
            Self::Locked => formatter.write_str(t(
                "la clave de los datos locales de esta cuenta no está disponible",
            )),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

fn to_json(value: &impl Serialize) -> Result<String, Error> {
    serde_json::to_string(value)
        .map_err(|error| Error::Sqlite(rusqlite::Error::ToSqlConversionFailure(error.into())))
}

/// `value` as JSON, sealed for `place`; the plain JSON is erased.
fn seal_json(key: &Key, place: &Place, value: &impl Serialize) -> Result<Vec<u8>, Error> {
    let json = Zeroizing::new(to_json(value)?);
    key.seal(place, json.as_bytes()).ok_or_else(sealing_failed)
}

/// What `value` sealed for `place`, if it opens and still reads as a `T`.
fn open_json<T: DeserializeOwned>(key: &Key, place: &Place, value: ValueRef<'_>) -> Option<T> {
    key.open(place, value)
        .and_then(|json| serde_json::from_str(&json).ok())
}

fn sealing_failed() -> Error {
    Error::Sqlite(rusqlite::Error::ToSqlConversionFailure(
        "the system gave no randomness to seal a value".into(),
    ))
}

/// The local store.
pub struct Storage {
    connection: Connection,
    /// The keys of the accounts unlocked now, by their name in the store.
    keys: HashMap<String, Key>,
}

impl Storage {
    /// Open or create the store at `path`.
    pub fn open(path: &Path) -> Result<Self, Error> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }
        let connection = Connection::open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        Self::migrate(connection)
    }

    /// A store that lives in memory, for tests and for running without a disk.
    pub fn in_memory() -> Result<Self, Error> {
        Self::migrate(Connection::open_in_memory()?)
    }

    fn migrate(mut connection: Connection) -> Result<Self, Error> {
        let version: i32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA {
            return Err(Error::Newer {
                found: version,
                supported: SCHEMA,
            });
        }
        if version == 0 {
            // Only takes effect before the first table exists.
            connection.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
        }
        connection.pragma_update(None, "secure_delete", true)?;
        let page_size: i64 = connection.pragma_query_value(None, "page_size", |row| row.get(0))?;
        connection.pragma_update(None, "max_page_count", MAX_BYTES / page_size.max(512))?;
        // Each step and its version change commit together, or not at all.
        let transaction = connection.transaction()?;
        if version < 1 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS messages (
                     account TEXT NOT NULL,
                     channel INTEGER NOT NULL,
                     id INTEGER NOT NULL,
                     body TEXT NOT NULL,
                     PRIMARY KEY (account, channel, id)
                 ) WITHOUT ROWID;
                 CREATE TABLE IF NOT EXISTS settings (
                     key TEXT PRIMARY KEY,
                     value TEXT NOT NULL
                 );",
            )?;
        }
        if version < 2 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS drafts (
                     account TEXT NOT NULL,
                     channel INTEGER NOT NULL,
                     text TEXT NOT NULL,
                     PRIMARY KEY (account, channel)
                 ) WITHOUT ROWID;
                 CREATE TABLE IF NOT EXISTS snapshots (
                     account TEXT PRIMARY KEY,
                     body TEXT NOT NULL
                 );",
            )?;
        }
        if version < 3 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS tombstones (
                     position INTEGER PRIMARY KEY,
                     account TEXT NOT NULL,
                     channel INTEGER NOT NULL,
                     id INTEGER NOT NULL,
                     UNIQUE (account, channel, id)
                 );
                 UPDATE snapshots SET body = json_replace(
                     body, '$.presences', json('[]'), '$.voice', json('[]'))
                 WHERE CASE WHEN json_valid(body)
                     THEN json_type(body) = 'object' ELSE 0 END;",
            )?;
        }
        if version < 4 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS read_marks (
                     account TEXT NOT NULL,
                     channel INTEGER NOT NULL,
                     body TEXT NOT NULL,
                     PRIMARY KEY (account, channel)
                 ) WITHOUT ROWID;",
            )?;
        }
        // v5 seals each account's rows. Rows an older version kept in plain
        // text need that account's key, which only `unlock` brings: they are
        // sealed then, and never read as they are.
        if version < 5 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS found (
                     account TEXT NOT NULL,
                     channel INTEGER NOT NULL,
                     id INTEGER NOT NULL,
                     body BLOB NOT NULL,
                     PRIMARY KEY (account, channel, id)
                 ) WITHOUT ROWID;",
            )?;
        }
        // v6: the files of each draft (envios-y-subidas), where each is and
        // its size, sealed by account as the draft's text is.
        if version < 6 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS draft_files (
                     account TEXT NOT NULL,
                     channel INTEGER NOT NULL,
                     body BLOB NOT NULL,
                     PRIMARY KEY (account, channel)
                 ) WITHOUT ROWID;",
            )?;
        }
        transaction.pragma_update(None, "user_version", SCHEMA)?;
        transaction.commit()?;
        Ok(Self {
            connection,
            keys: HashMap::new(),
        })
    }

    /// Let `account`'s rows be written and read with `key`, which replaces
    /// the one it had. Rows an older RUNE kept in plain text are sealed now,
    /// in one transaction, and the write-ahead log is emptied so their text
    /// does not linger there.
    pub fn unlock(&mut self, account: &str, key: Key) -> Result<(), Error> {
        self.unlock_with(account, key, false)
    }

    /// [`Storage::unlock`] with a key made just now, because the account had
    /// none or the one it had no longer reads. Whatever of it was sealed
    /// before was sealed with a key that is lost and can never open again:
    /// it goes, instead of failing every read that meets it.
    pub fn unlock_new(&mut self, account: &str, key: Key) -> Result<(), Error> {
        self.unlock_with(account, key, true)
    }

    fn unlock_with(&mut self, account: &str, key: Key, new: bool) -> Result<(), Error> {
        let transaction = self.connection.transaction()?;
        if new {
            // Fixed names from `CONTENT`, never input; `found` is never plain.
            for (table, _, content) in CONTENT {
                transaction.execute(
                    &format!(
                        "DELETE FROM {table} WHERE account = ?1 AND typeof({content}) = 'blob'"
                    ),
                    params![account],
                )?;
            }
            transaction.execute("DELETE FROM found WHERE account = ?1", params![account])?;
        }
        seal_plain(&transaction, &key, account)?;
        transaction.commit()?;
        self.checkpoint()?;
        self.keys.insert(account.to_owned(), key);
        Ok(())
    }

    /// Forget every key: no account is written or read until unlocked again.
    pub fn lock(&mut self) {
        self.keys.clear();
    }

    fn key(&self, account: &str) -> Result<&Key, Error> {
        self.keys.get(account).ok_or(Error::Locked)
    }

    /// A snapshot a version before 5 kept in plain text, read only to know
    /// whose it is before [`Storage::migrate_account`] moves it; a sealed
    /// one is never read here.
    pub fn legacy_snapshot<T: DeserializeOwned>(&self, account: &str) -> Result<Option<T>, Error> {
        let body: Option<Option<String>> = self
            .connection
            .query_row(
                "SELECT body FROM snapshots WHERE account = ?1",
                params![account],
                |row| Ok(row.get_ref(0)?.as_str().ok().map(str::to_owned)),
            )
            .optional()?;
        Ok(body
            .flatten()
            .and_then(|body| serde_json::from_str(&Zeroizing::new(body)).ok()))
    }

    /// Move what the write-ahead log holds into the file, where deleted
    /// rows are already overwritten, and empty the log.
    fn checkpoint(&self) -> Result<(), Error> {
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    /// Keep confirmed messages without undoing newer edits or recent deletions.
    pub fn save_messages(&mut self, account: &str, messages: &[Message]) -> Result<(), Error> {
        let key = self.keys.get(account).ok_or(Error::Locked)?;
        let transaction = self.connection.transaction()?;
        for message in messages {
            upsert(&transaction, key, account, message)?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Apply a provider burst without losing page boundaries or event order.
    pub fn apply_messages(
        &mut self,
        account: &str,
        changes: &[MessageMutation],
    ) -> Result<(), Error> {
        let key = self.keys.get(account).ok_or(Error::Locked)?;
        let transaction = self.connection.transaction()?;
        for change in changes {
            match change {
                MessageMutation::Upsert(message) => upsert(&transaction, key, account, message)?,
                MessageMutation::Patch(patch) => patch_message(&transaction, key, account, patch)?,
                MessageMutation::History {
                    channel,
                    before,
                    messages,
                    reached_start,
                } => reconcile(
                    &transaction,
                    key,
                    account,
                    *channel,
                    *before,
                    messages,
                    *reached_start,
                )?,
                MessageMutation::Delete { channel, message } => {
                    delete(&transaction, account, *channel, *message)?;
                }
                MessageMutation::Found(messages) => {
                    for message in messages {
                        keep_found(&transaction, key, account, message)?;
                    }
                }
                MessageMutation::Tally {
                    channel,
                    message,
                    change,
                } => update_row(&transaction, key, account, *channel, *message, |held| {
                    change.apply(held)
                })?,
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Remove a message and remember its deletion across restarts.
    pub fn delete_message(
        &mut self,
        account: &str,
        channel: ChannelId,
        id: MessageId,
    ) -> Result<(), Error> {
        self.key(account)?;
        let transaction = self.connection.transaction()?;
        delete(&transaction, account, channel, id)?;
        transaction.commit()?;
        Ok(())
    }

    /// The newest `limit` messages kept for `channel`, oldest first. A row that
    /// no longer reads as a message is skipped.
    pub fn recent(
        &self,
        account: &str,
        channel: ChannelId,
        limit: usize,
    ) -> Result<Vec<Message>, Error> {
        let key = self.key(account)?;
        let mut query = self.connection.prepare_cached(
            "SELECT id, body FROM messages WHERE account = ?1 AND channel = ?2 ORDER BY id DESC LIMIT ?3",
        )?;
        let mut rows = query.query(params![account, channel.0 as i64, limit as i64])?;
        let mut messages = Vec::new();
        while let Some(row) = rows.next()? {
            let place = Place::message(account, channel.0 as i64, row.get(0)?);
            if let Some(message) = open_json::<Message>(key, &place, row.get_ref(1)?) {
                messages.push(message);
            }
        }
        messages.reverse();
        Ok(messages)
    }

    /// Keep at most `keep` messages per conversation of `account`, the newest,
    /// of its history and of what searches found, and give the freed pages
    /// back to the file system.
    pub fn prune(&mut self, account: &str, keep: usize) -> Result<usize, Error> {
        let mut removed = 0;
        for table in ["messages", "found"] {
            removed += self.connection.execute(
                &format!(
                    "DELETE FROM {table} WHERE account = ?1 AND (channel, id) IN (
                         SELECT channel, id FROM (
                             SELECT channel, id, ROW_NUMBER() OVER (PARTITION BY channel ORDER BY id DESC) AS newest
                             FROM {table} WHERE account = ?1
                         ) WHERE newest > ?2
                     )"
                ),
                params![account, keep as i64],
            )?;
        }
        self.connection
            .execute_batch("PRAGMA incremental_vacuum;")?;
        Ok(removed)
    }

    /// How many messages are kept for `account`.
    pub fn count(&self, account: &str) -> Result<usize, Error> {
        Ok(self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE account = ?1",
                params![account],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count as usize)?)
    }

    /// Keep the last session's `snapshot` of `account`, replacing the one before.
    pub fn save_snapshot(&self, account: &str, snapshot: &impl Serialize) -> Result<(), Error> {
        let body = seal_json(self.key(account)?, &Place::snapshot(account), snapshot)?;
        self.connection.execute(
            "INSERT INTO snapshots (account, body) VALUES (?1, ?2)
             ON CONFLICT(account) DO UPDATE SET body = excluded.body",
            params![account, body],
        )?;
        Ok(())
    }

    /// The last snapshot kept for `account`, if it still reads as one.
    pub fn snapshot<T: DeserializeOwned>(&self, account: &str) -> Result<Option<T>, Error> {
        let key = self.key(account)?;
        Ok(self
            .connection
            .query_row(
                "SELECT body FROM snapshots WHERE account = ?1",
                params![account],
                |row| Ok(open_json(key, &Place::snapshot(account), row.get_ref(0)?)),
            )
            .optional()?
            .flatten())
    }

    /// Keep what was being written in `channel`, or forget it with `None`.
    pub fn set_draft(
        &self,
        account: &str,
        channel: ChannelId,
        text: Option<&str>,
    ) -> Result<(), Error> {
        let key = self.key(account)?;
        match text.filter(|text| !text.is_empty()) {
            Some(text) => self.connection.execute(
                "INSERT INTO drafts (account, channel, text) VALUES (?1, ?2, ?3)
                 ON CONFLICT(account, channel) DO UPDATE SET text = excluded.text",
                params![
                    account,
                    channel.0 as i64,
                    key.seal(&Place::draft(account, channel.0 as i64), text.as_bytes())
                        .ok_or_else(sealing_failed)?
                ],
            )?,
            None => self.connection.execute(
                "DELETE FROM drafts WHERE account = ?1 AND channel = ?2",
                params![account, channel.0 as i64],
            )?,
        };
        Ok(())
    }

    /// Every draft kept for `account` that still opens.
    pub fn drafts(&self, account: &str) -> Result<Vec<(ChannelId, String)>, Error> {
        let key = self.key(account)?;
        let mut query = self
            .connection
            .prepare_cached("SELECT channel, text FROM drafts WHERE account = ?1")?;
        let mut rows = query.query(params![account])?;
        let mut drafts = Vec::new();
        while let Some(row) = rows.next()? {
            let channel: i64 = row.get(0)?;
            if let Some(text) = key.open(&Place::draft(account, channel), row.get_ref(1)?) {
                drafts.push((ChannelId(channel as u64), text.to_string()));
            }
        }
        Ok(drafts)
    }

    /// Move a verified legacy namespace without replacing rows already kept
    /// under the stable identity. Either every table moves or none does.
    /// A legacy namespace predates sealing: what it moves is sealed under
    /// the destination's key, now if it is unlocked, else when it is.
    pub fn migrate_account(&self, from: &str, to: &str) -> Result<(), Error> {
        if from == to {
            return Ok(());
        }
        let transaction = self.connection.unchecked_transaction()?;
        for (table, fields) in [
            ("messages", "channel, id, body"),
            ("drafts", "channel, text"),
            ("snapshots", "body"),
            ("read_marks", "channel, body"),
            ("draft_files", "channel, body"),
        ] {
            transaction.execute(
                &format!(
                    "INSERT OR IGNORE INTO {table} (account, {fields})
                          SELECT ?2, {fields} FROM {table} WHERE account = ?1"
                ),
                params![from, to],
            )?;
        }
        let legacy_deletions = {
            let mut query = transaction.prepare(
                "SELECT channel, id FROM tombstones WHERE account = ?1 ORDER BY position DESC",
            )?;
            let deletions = query
                .query_map(params![from], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            deletions
        };
        for (channel, message) in legacy_deletions {
            // Legacy deletions precede every deletion already kept under the
            // destination, including after repeated namespace migrations.
            transaction.execute(
                "INSERT OR IGNORE INTO tombstones (position, account, channel, id)
                 SELECT COALESCE(MIN(position), 0) - 1, ?1, ?2, ?3 FROM tombstones",
                params![to, channel, message],
            )?;
        }
        transaction.execute(
            "DELETE FROM messages WHERE account = ?1 AND EXISTS (
                 SELECT 1 FROM tombstones WHERE account = messages.account
                 AND channel = messages.channel AND id = messages.id)",
            params![to],
        )?;
        transaction.execute(
            "DELETE FROM tombstones WHERE account = ?1 AND position NOT IN (
                 SELECT kept.position FROM tombstones AS kept
                 WHERE kept.account = tombstones.account AND kept.channel = tombstones.channel
                 ORDER BY kept.position DESC LIMIT ?2)",
            params![to, TOMBSTONES as i64],
        )?;
        for table in [
            "messages",
            "drafts",
            "snapshots",
            "tombstones",
            "read_marks",
            "draft_files",
            "found",
        ] {
            transaction.execute(
                &format!("DELETE FROM {table} WHERE account = ?1"),
                params![from],
            )?;
        }
        if let Some(key) = self.keys.get(to) {
            seal_plain(&transaction, key, to)?;
        }
        transaction.commit()?;
        self.checkpoint()
    }

    /// Delete everything kept for `account`: its messages, the ones searches
    /// found, drafts, snapshot and marks. Deleted rows are overwritten, and
    /// the write-ahead log is emptied, so none of it stays in the files.
    pub fn forget_account(&self, account: &str) -> Result<(), Error> {
        let transaction = self.connection.unchecked_transaction()?;
        for table in [
            "messages",
            "drafts",
            "snapshots",
            "tombstones",
            "read_marks",
            "draft_files",
            "found",
        ] {
            transaction.execute(
                &format!("DELETE FROM {table} WHERE account = ?1"),
                params![account],
            )?;
        }
        transaction.commit()?;
        self.connection
            .execute_batch("PRAGMA incremental_vacuum;")?;
        self.checkpoint()
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>, Error> {
        Ok(self
            .connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Every setting kept, by key, at most [`MAX_SETTINGS`]: the host reads
    /// them once at startup, so it can hand any account's back later.
    pub fn settings(&self) -> Result<Vec<(String, String)>, Error> {
        let mut statement = self
            .connection
            .prepare("SELECT key, value FROM settings ORDER BY key LIMIT ?1")?;
        let rows = statement.query_map(params![MAX_SETTINGS as i64], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Keep `value` under `key`, or forget the key with `None`.
    pub fn set_setting(&self, key: &str, value: Option<&str>) -> Result<(), Error> {
        match value {
            Some(value) => self.connection.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?,
            None => self
                .connection
                .execute("DELETE FROM settings WHERE key = ?1", params![key])?,
        };
        Ok(())
    }
}

/// The message kept in `table` at this place, if it still opens.
fn held(
    connection: &Connection,
    key: &Key,
    table: &str,
    account: &str,
    channel: ChannelId,
    id: MessageId,
) -> Result<Option<Message>, Error> {
    let (channel, id) = (channel.0 as i64, id.0 as i64);
    // `table` is one of this file's own names, never input.
    Ok(connection
        .query_row(
            &format!("SELECT body FROM {table} WHERE account = ?1 AND channel = ?2 AND id = ?3"),
            params![account, channel, id],
            |row| {
                Ok(open_json(
                    key,
                    &Place::message(account, channel, id),
                    row.get_ref(0)?,
                ))
            },
        )
        .optional()?
        .flatten())
}

fn deleted(
    connection: &Connection,
    account: &str,
    channel: ChannelId,
    id: MessageId,
) -> Result<bool, Error> {
    Ok(connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM tombstones WHERE account = ?1 AND channel = ?2 AND id = ?3)",
        params![account, channel.0 as i64, id.0 as i64],
        |row| row.get(0),
    )?)
}

/// Seal `message` into `table` at its place.
fn write(
    connection: &Connection,
    key: &Key,
    table: &str,
    account: &str,
    message: &Message,
) -> Result<(), Error> {
    let (channel, id) = (message.channel.0 as i64, message.id.0 as i64);
    let body = seal_json(key, &Place::message(account, channel, id), message)?;
    connection.execute(
        &format!(
            "INSERT OR REPLACE INTO {table} (account, channel, id, body) VALUES (?1, ?2, ?3, ?4)"
        ),
        params![account, channel, id, body],
    )?;
    Ok(())
}

/// Keep a confirmed message in its conversation's history. A copy a search
/// found leaves for it, and the newer edit of the two stays.
fn upsert(
    connection: &Connection,
    key: &Key,
    account: &str,
    message: &Message,
) -> Result<(), Error> {
    let (channel, id) = (message.channel, message.id);
    if message.delivery.is_local() || deleted(connection, account, channel, id)? {
        return Ok(());
    }
    let found = held(connection, key, "found", account, channel, id)?;
    connection.execute(
        "DELETE FROM found WHERE account = ?1 AND channel = ?2 AND id = ?3",
        params![account, channel.0 as i64, id.0 as i64],
    )?;
    if held(connection, key, "messages", account, channel, id)?
        .is_some_and(|held| held.edited > message.edited)
    {
        return Ok(());
    }
    let newest = match &found {
        Some(found) if found.edited > message.edited => found,
        _ => message,
    };
    write(connection, key, "messages", account, newest)
}

/// Keep a message a search found apart from the history, unless the
/// history already holds it: then it is updated there.
fn keep_found(
    connection: &Connection,
    key: &Key,
    account: &str,
    message: &Message,
) -> Result<(), Error> {
    let (channel, id) = (message.channel, message.id);
    if message.delivery.is_local() || deleted(connection, account, channel, id)? {
        return Ok(());
    }
    let in_history: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM messages WHERE account = ?1 AND channel = ?2 AND id = ?3)",
        params![account, channel.0 as i64, id.0 as i64],
        |row| row.get(0),
    )?;
    if in_history {
        return upsert(connection, key, account, message);
    }
    if held(connection, key, "found", account, channel, id)?
        .is_some_and(|held| held.edited > message.edited)
    {
        return Ok(());
    }
    write(connection, key, "found", account, message)
}

fn patch_message(
    connection: &Connection,
    key: &Key,
    account: &str,
    patch: &MessagePatch,
) -> Result<(), Error> {
    update_row(
        connection,
        key,
        account,
        patch.channel,
        patch.id,
        |message| patch.apply(message),
    )
}

/// Change a retained, not deleted message in place, in the history and in
/// what searches found; a missing one stays missing.
fn update_row(
    connection: &Connection,
    key: &Key,
    account: &str,
    channel: ChannelId,
    id: MessageId,
    mut change: impl FnMut(&mut Message) -> bool,
) -> Result<(), Error> {
    if deleted(connection, account, channel, id)? {
        return Ok(());
    }
    for table in ["messages", "found"] {
        let Some(mut message) = held(connection, key, table, account, channel, id)? else {
            continue;
        };
        if change(&mut message) {
            // Update only: a partial event can never materialize a missing baseline.
            let (channel, id) = (channel.0 as i64, id.0 as i64);
            let body = seal_json(key, &Place::message(account, channel, id), &message)?;
            connection.execute(
                &format!(
                    "UPDATE {table} SET body = ?4 WHERE account = ?1 AND channel = ?2 AND id = ?3"
                ),
                params![account, channel, id, body],
            )?;
        }
    }
    Ok(())
}

fn delete(
    connection: &Connection,
    account: &str,
    channel: ChannelId,
    id: MessageId,
) -> Result<(), Error> {
    for table in ["messages", "found"] {
        connection.execute(
            &format!("DELETE FROM {table} WHERE account = ?1 AND channel = ?2 AND id = ?3"),
            params![account, channel.0 as i64, id.0 as i64],
        )?;
    }
    // A replacement gets a new SQLite position, so a repeated deletion becomes recent again.
    connection.execute(
        "INSERT OR REPLACE INTO tombstones (account, channel, id) VALUES (?1, ?2, ?3)",
        params![account, channel.0 as i64, id.0 as i64],
    )?;
    connection.execute(
        "DELETE FROM tombstones WHERE account = ?1 AND channel = ?2 AND position NOT IN (
             SELECT position FROM tombstones WHERE account = ?1 AND channel = ?2
             ORDER BY position DESC LIMIT ?3
         )",
        params![account, channel.0 as i64, TOMBSTONES as i64],
    )?;
    Ok(())
}

fn reconcile(
    connection: &Connection,
    key: &Key,
    account: &str,
    channel: ChannelId,
    before: Option<MessageId>,
    messages: &[Message],
    reached_start: bool,
) -> Result<(), Error> {
    let end = before.unwrap_or(MessageId(u64::MAX));
    let start = match messages.first() {
        Some(first) => first.id,
        None if reached_start => MessageId(0),
        None => end,
    };
    let in_page: HashSet<MessageId> = messages.iter().map(|message| message.id).collect();
    let covered = |id: &MessageId| *id >= start && *id < end && !in_page.contains(id);
    for table in ["messages", "found"] {
        // `table` is one of this file's own names, never input.
        let held: Vec<MessageId> = connection
            .prepare_cached(&format!(
                "SELECT id FROM {table} WHERE account = ?1 AND channel = ?2"
            ))?
            .query_map(params![account, channel.0 as i64], |row| {
                Ok(MessageId(row.get::<_, i64>(0)? as u64))
            })?
            .collect::<Result<_, _>>()?;
        // Only the history is a run a newer page can leave behind; what a
        // search found in the page's range and the page leaves out is gone.
        let disconnected = table == "messages"
            && before.is_none()
            && held.iter().max().is_some_and(|newest| *newest < start);
        let mut remove = connection.prepare_cached(&format!(
            "DELETE FROM {table} WHERE account = ?1 AND channel = ?2 AND id = ?3"
        ))?;
        for id in held {
            if disconnected || covered(&id) {
                remove.execute(params![account, channel.0 as i64, id.0 as i64])?;
            }
        }
    }
    for message in messages {
        upsert(connection, key, account, message)?;
    }
    Ok(())
}

/// What each table keeps of an account: the columns of a row's place, and
/// the one holding its content.
const CONTENT: [(&str, &str, &str); 5] = [
    ("messages", "channel, id", "body"),
    ("drafts", "channel, 0", "text"),
    ("snapshots", "0, 0", "body"),
    ("read_marks", "channel, 0", "body"),
    ("draft_files", "channel, 0", "body"),
];

/// Seal `account`'s rows still in plain text, as versions before 5 kept them.
fn seal_plain(connection: &Connection, key: &Key, account: &str) -> Result<(), Error> {
    for (table, place, content) in CONTENT {
        // Fixed names from `CONTENT`, never input.
        let plain: Vec<(i64, i64, Zeroizing<String>)> = connection
            .prepare(&format!(
                "SELECT {place}, {content} FROM {table}
                 WHERE account = ?1 AND typeof({content}) = 'text'"
            ))?
            .query_map(params![account], |row| {
                Ok((row.get(0)?, row.get(1)?, Zeroizing::new(row.get(2)?)))
            })?
            .collect::<Result<_, _>>()?;
        for (channel, id, text) in plain {
            let (place, row) = match table {
                "messages" => (
                    Place::message(account, channel, id),
                    "channel = ?2 AND id = ?3",
                ),
                "drafts" => (Place::draft(account, channel), "channel = ?2 AND ?3 = 0"),
                "snapshots" => (Place::snapshot(account), "?2 = 0 AND ?3 = 0"),
                "draft_files" => (
                    Place::draft_files(account, channel),
                    "channel = ?2 AND ?3 = 0",
                ),
                _ => (
                    Place::read_mark(account, channel),
                    "channel = ?2 AND ?3 = 0",
                ),
            };
            let sealed = key
                .seal(&place, text.as_bytes())
                .ok_or_else(sealing_failed)?;
            connection.execute(
                &format!("UPDATE {table} SET {content} = ?4 WHERE account = ?1 AND {row}"),
                params![account, channel, id, sealed],
            )?;
        }
    }
    Ok(())
}

/// The one key the tests seal with, for every account they use.
#[cfg(test)]
pub(crate) fn test_key() -> Key {
    Key::from_text(&"5a".repeat(32)).unwrap()
}

#[cfg(test)]
impl Storage {
    /// Unlock the accounts the tests use, all with [`test_key`].
    pub(crate) fn unlocked(mut self) -> Self {
        for account in [
            "a",
            "b",
            "other",
            "missing",
            "simulación",
            "otra cuenta",
            "unknown-account",
            "legacy",
            "scoped",
            "stable",
        ] {
            self.unlock(account, test_key()).unwrap();
        }
        self
    }

    /// Rows as versions before 5 kept them, in plain text: (table,
    /// account, channel, id, content), with `channel` or `id` unused where
    /// the table has no such column.
    pub(crate) fn plain(&self, rows: &[(&str, &str, i64, i64, String)]) {
        for (table, account, channel, id, content) in rows {
            let sql = match *table {
                "messages" => {
                    "INSERT INTO messages (account, channel, id, body) VALUES (?1, ?2, ?3, ?4)"
                }
                "drafts" => "INSERT INTO drafts (account, channel, text) VALUES (?1, ?2, ?4)",
                "snapshots" => "INSERT INTO snapshots (account, body) VALUES (?1, ?4)",
                _ => "INSERT INTO read_marks (account, channel, body) VALUES (?1, ?2, ?4)",
            };
            let mut statement = self.connection.prepare(sql).unwrap();
            for (index, value) in [(1, account.to_string()), (4, content.clone())] {
                statement.raw_bind_parameter(index, value).unwrap();
            }
            if sql.contains("?2") {
                statement.raw_bind_parameter(2, channel).unwrap();
            }
            if sql.contains("?3") {
                statement.raw_bind_parameter(3, id).unwrap();
            }
            statement.raw_execute().unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{Delivery, MessageKind, Reaction, Timestamp, UserId};

    fn message(channel: u64, id: u64) -> Message {
        Message {
            id: MessageId(id),
            channel: ChannelId(channel),
            author: UserId(2),
            content: format!("**hola** {id}"),
            timestamp: Timestamp(id as i64),
            edited: None,
            kind: MessageKind::Default,
            reply_to: Some(MessageId(1)),
            mentions: vec![UserId(1)],
            reactions: vec![Reaction {
                emoji: "🦀".into(),
                count: 2,
                me: true,
                ..Default::default()
            }],
            attachments: Vec::new(),
            stickers: Vec::new(),
            text_references: Vec::new(),
            delivery: Delivery::Sent,
            // Kept as written: a pinned message reads back pinned.
            pinned: id.is_multiple_of(2),
            embeds: Vec::new(),
            forwarded: None,
            poll: None,
            components: Default::default(),
            call: None,
            mention_roles: Vec::new(),
            mention_everyone: false,
        }
    }

    fn temporary(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("rune-storage-{name}-{}", std::process::id()))
            .join("rune.db")
    }

    #[test]
    fn messages_come_back_as_they_were_newest_last() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        let kept: Vec<Message> = (1..=10).map(|id| message(5, id)).collect();
        storage.save_messages("simulación", &kept).unwrap();
        assert_eq!(
            storage.recent("simulación", ChannelId(5), 3).unwrap(),
            kept[7..].to_vec()
        );
        assert!(
            storage
                .recent("otra cuenta", ChannelId(5), 3)
                .unwrap()
                .is_empty(),
            "accounts never mix"
        );
    }

    fn patch(id: u64) -> MessagePatch {
        MessagePatch {
            channel: ChannelId(5),
            id: MessageId(id),
            ..Default::default()
        }
    }

    #[test]
    fn partial_message_changes_preserve_absent_fields_and_survive_restart() {
        let path = temporary("partial-message-change");
        let mut before = message(5, 1);
        before.pinned = true;
        before.text_references = vec![rune_model::TextReference {
            source: before.content.clone(),
            target: rune_model::TextTarget::Invitation,
        }];
        let mut expected = before.clone();
        expected.content = "edited in SQLite".into();
        expected.edited = Some(Timestamp(50));
        expected.reactions.clear();
        expected.text_references.clear();
        expected.pinned = false;
        {
            let mut storage = Storage::open(&path).unwrap().unlocked();
            storage
                .save_messages("a", std::slice::from_ref(&before))
                .unwrap();
            storage
                .save_messages("b", std::slice::from_ref(&before))
                .unwrap();
            storage
                .apply_messages(
                    "a",
                    &[MessageMutation::Patch(Box::new(MessagePatch {
                        content: Some(expected.content.clone()),
                        edited: Some(expected.edited),
                        reactions: Some(Vec::new()),
                        pinned: Some(false),
                        ..patch(1)
                    }))],
                )
                .unwrap();
        }
        let storage = Storage::open(&path).unwrap().unlocked();
        assert_eq!(
            storage.recent("a", ChannelId(5), 10).unwrap(),
            vec![expected]
        );
        assert_eq!(storage.recent("b", ChannelId(5), 10).unwrap(), vec![before]);
        assert_eq!(
            storage
                .connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0))
                .unwrap(),
            SCHEMA
        );
        drop(storage);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn partial_message_changes_never_create_a_baseline_or_revive_a_deletion() {
        let path = temporary("partial-message-tombstone");
        let before = message(5, 1);
        {
            let mut storage = Storage::open(&path).unwrap().unlocked();
            storage
                .save_messages("a", std::slice::from_ref(&before))
                .unwrap();
            storage
                .save_messages("b", std::slice::from_ref(&before))
                .unwrap();
            storage
                .apply_messages(
                    "unknown-account",
                    &[MessageMutation::Patch(Box::new(patch(1)))],
                )
                .unwrap();
            storage
                .apply_messages(
                    "a",
                    &[
                        MessageMutation::Patch(Box::new(MessagePatch {
                            content: Some("unknown".into()),
                            ..patch(2)
                        })),
                        MessageMutation::Delete {
                            channel: ChannelId(5),
                            message: MessageId(1),
                        },
                        MessageMutation::Patch(Box::new(MessagePatch {
                            content: Some("late".into()),
                            ..patch(1)
                        })),
                    ],
                )
                .unwrap();
        }
        let mut storage = Storage::open(&path).unwrap().unlocked();
        storage
            .apply_messages(
                "a",
                &[
                    MessageMutation::Patch(Box::new(MessagePatch {
                        content: Some("after restart".into()),
                        ..patch(1)
                    })),
                    MessageMutation::Upsert(Box::new(before.clone())),
                ],
            )
            .unwrap();
        assert!(storage.recent("a", ChannelId(5), 10).unwrap().is_empty());
        assert!(storage
            .recent("unknown-account", ChannelId(5), 10)
            .unwrap()
            .is_empty());
        assert_eq!(storage.recent("b", ChannelId(5), 10).unwrap(), vec![before]);
        drop(storage);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn partial_message_changes_reject_older_edits_and_author_mismatches_atomically() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        let before = Message {
            edited: Some(Timestamp(50)),
            ..message(5, 1)
        };
        storage
            .save_messages("a", std::slice::from_ref(&before))
            .unwrap();
        for invalid in [
            MessagePatch {
                edited: Some(Some(Timestamp(49))),
                ..patch(1)
            },
            MessagePatch {
                edited: Some(None),
                ..patch(1)
            },
            MessagePatch {
                author: Some(UserId(999)),
                ..patch(1)
            },
        ] {
            storage
                .apply_messages(
                    "a",
                    &[MessageMutation::Patch(Box::new(MessagePatch {
                        content: Some("must not partially apply".into()),
                        reactions: Some(Vec::new()),
                        ..invalid
                    }))],
                )
                .unwrap();
            assert_eq!(
                storage.recent("a", ChannelId(5), 10).unwrap(),
                vec![before.clone()]
            );
        }
        storage
            .apply_messages(
                "a",
                &[MessageMutation::Patch(Box::new(MessagePatch {
                    reply_to: Some(None),
                    mentions: Some(Vec::new()),
                    ..patch(1)
                }))],
            )
            .unwrap();
        let actual = storage
            .recent("a", ChannelId(5), 10)
            .unwrap()
            .pop()
            .unwrap();
        assert!(actual.reply_to.is_none() && actual.mentions.is_empty());
        assert_eq!(actual.content, before.content);
        assert_eq!(actual.edited, before.edited);
    }

    #[test]
    fn partial_message_changes_follow_the_same_transaction_order_as_other_mutations() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        storage
            .apply_messages(
                "a",
                &[
                    MessageMutation::Patch(Box::new(MessagePatch {
                        content: Some("no deferred patch".into()),
                        ..patch(1)
                    })),
                    MessageMutation::Upsert(Box::new(message(5, 1))),
                    MessageMutation::History {
                        channel: ChannelId(5),
                        before: None,
                        messages: vec![message(5, 1), message(5, 2)],
                        reached_start: false,
                    },
                    MessageMutation::Patch(Box::new(MessagePatch {
                        content: Some("after history".into()),
                        ..patch(2)
                    })),
                ],
            )
            .unwrap();
        assert_eq!(
            storage.recent("a", ChannelId(5), 10).unwrap(),
            vec![
                message(5, 1),
                Message {
                    content: "after history".into(),
                    ..message(5, 2)
                }
            ]
        );
        storage
            .apply_messages(
                "a",
                &[
                    MessageMutation::Delete {
                        channel: ChannelId(5),
                        message: MessageId(2),
                    },
                    MessageMutation::Patch(Box::new(MessagePatch {
                        content: Some("no resurrection".into()),
                        ..patch(2)
                    })),
                ],
            )
            .unwrap();
        assert_eq!(
            storage.recent("a", ChannelId(5), 10).unwrap(),
            vec![message(5, 1)]
        );
    }

    #[test]
    fn saving_again_replaces_and_unconfirmed_messages_are_not_kept() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        storage.save_messages("a", &[message(5, 1)]).unwrap();
        let edited = Message {
            content: "editado".into(),
            ..message(5, 1)
        };
        let sending = Message {
            delivery: Delivery::Sending,
            ..message(5, 2)
        };
        let failed = Message {
            delivery: Delivery::Failed {
                reason: "no".into(),
            },
            ..message(5, 3)
        };
        storage
            .save_messages("a", &[edited.clone(), sending, failed])
            .unwrap();
        assert_eq!(storage.recent("a", ChannelId(5), 10).unwrap(), vec![edited]);
        storage
            .delete_message("a", ChannelId(5), MessageId(1))
            .unwrap();
        assert_eq!(storage.count("a").unwrap(), 0);
    }

    #[test]
    fn a_stale_edit_cannot_overwrite_newer_cached_content() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        let newer = Message {
            content: "newer".into(),
            edited: Some(Timestamp(50)),
            ..message(5, 1)
        };
        storage
            .save_messages("a", std::slice::from_ref(&newer))
            .unwrap();
        storage
            .save_messages(
                "a",
                &[Message {
                    edited: Some(Timestamp(10)),
                    ..message(5, 1)
                }],
            )
            .unwrap();
        storage.save_messages("a", &[message(5, 1)]).unwrap();
        assert_eq!(storage.recent("a", ChannelId(5), 10).unwrap(), vec![newer]);
    }

    #[test]
    fn a_deletion_prevents_late_history_from_resurrecting_it_after_reopen() {
        let path = temporary("tombstone");
        {
            let mut storage = Storage::open(&path).unwrap().unlocked();
            storage.save_messages("a", &[message(5, 1)]).unwrap();
            storage
                .delete_message("a", ChannelId(5), MessageId(1))
                .unwrap();
        }
        let mut storage = Storage::open(&path).unwrap().unlocked();
        storage.save_messages("a", &[message(5, 1)]).unwrap();
        assert!(storage.recent("a", ChannelId(5), 10).unwrap().is_empty());
        drop(storage);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    fn history(
        before: Option<u64>,
        ids: impl IntoIterator<Item = u64>,
        reached_start: bool,
    ) -> MessageMutation {
        MessageMutation::History {
            channel: ChannelId(5),
            before: before.map(MessageId),
            messages: ids.into_iter().map(|id| message(5, id)).collect(),
            reached_start,
        }
    }

    fn ids(storage: &Storage) -> Vec<u64> {
        storage
            .recent("a", ChannelId(5), 1000)
            .unwrap()
            .iter()
            .map(|message| message.id.0)
            .collect()
    }

    #[test]
    fn history_removes_missing_rows_only_inside_its_authoritative_range() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        let initial: Vec<_> = (1..=20).map(|id| message(5, id)).collect();
        storage.save_messages("a", &initial).unwrap();
        storage.save_messages("b", &initial).unwrap();
        storage.save_messages("a", &[message(6, 15)]).unwrap();
        storage
            .apply_messages(
                "a",
                &[
                    history(None, (11..=21).filter(|id| *id != 15 && *id != 18), false),
                    history(Some(11), (1..=10).filter(|id| *id != 3), true),
                ],
            )
            .unwrap();
        assert_eq!(
            ids(&storage),
            (1..=21)
                .filter(|id| ![3, 15, 18].contains(id))
                .collect::<Vec<_>>()
        );
        assert_eq!(storage.recent("b", ChannelId(5), 100).unwrap(), initial);
        assert_eq!(
            storage.recent("a", ChannelId(6), 100).unwrap(),
            vec![message(6, 15)]
        );
        storage
            .apply_messages("a", &[history(Some(11), [7, 9], false)])
            .unwrap();
        assert!(
            ids(&storage).contains(&6),
            "rows before the lower bound stay"
        );
        assert!(!ids(&storage).contains(&8));
        assert!(ids(&storage).contains(&11), "the upper bound is exclusive");
    }

    #[test]
    fn a_disconnected_newest_page_replaces_the_old_run() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        storage
            .apply_messages("a", &[history(None, 1..=10, true)])
            .unwrap();
        storage
            .apply_messages("a", &[history(None, 81..=90, false)])
            .unwrap();
        assert_eq!(ids(&storage), (81..=90).collect::<Vec<_>>());
    }

    #[test]
    fn empty_history_respects_its_cursor_and_the_latest_gap_rule() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        storage
            .apply_messages("a", &[history(None, 1..=5, true)])
            .unwrap();
        storage
            .apply_messages("a", &[history(Some(3), [], false)])
            .unwrap();
        assert_eq!(ids(&storage), vec![1, 2, 3, 4, 5]);
        storage
            .apply_messages("a", &[history(Some(3), [], true)])
            .unwrap();
        assert_eq!(ids(&storage), vec![3, 4, 5]);
        storage
            .apply_messages("a", &[history(None, [], true)])
            .unwrap();
        assert!(ids(&storage).is_empty());
        storage.save_messages("a", &[message(5, 1)]).unwrap();
        storage
            .apply_messages("a", &[history(None, [], false)])
            .unwrap();
        assert!(ids(&storage).is_empty());
    }

    #[test]
    fn a_burst_keeps_page_and_delete_order_without_reviving_deleted_messages() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        let deleted = MessageMutation::Delete {
            channel: ChannelId(5),
            message: MessageId(2),
        };
        storage
            .apply_messages(
                "a",
                &[
                    MessageMutation::Upsert(Box::new(message(5, 1))),
                    MessageMutation::Upsert(Box::new(message(5, 2))),
                    history(None, [2, 3], false),
                    deleted,
                    history(None, [2, 3], false),
                    MessageMutation::Upsert(Box::new(message(5, 2))),
                ],
            )
            .unwrap();
        assert_eq!(ids(&storage), vec![1, 3]);
        storage
            .apply_messages("a", &[history(None, [1, 3], true)])
            .unwrap();
        assert_eq!(ids(&storage), vec![1, 3]);
    }

    #[test]
    fn tombstones_keep_the_last_256_deletions_per_account_and_conversation() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        for id in 1..=256 {
            storage
                .delete_message("a", ChannelId(5), MessageId(id))
                .unwrap();
        }
        storage
            .delete_message("a", ChannelId(5), MessageId(1))
            .unwrap();
        storage
            .delete_message("a", ChannelId(5), MessageId(257))
            .unwrap();
        storage
            .save_messages(
                "a",
                &[message(5, 1), message(5, 2), message(5, 257), message(6, 1)],
            )
            .unwrap();
        storage.save_messages("b", &[message(5, 1)]).unwrap();
        assert_eq!(ids(&storage), vec![2], "the repeated deletion stays recent");
        assert_eq!(
            storage.recent("a", ChannelId(6), 10).unwrap(),
            vec![message(6, 1)]
        );
        assert_eq!(
            storage.recent("b", ChannelId(5), 10).unwrap(),
            vec![message(5, 1)]
        );
        let count: i64 = storage
            .connection
            .query_row("SELECT COUNT(*) FROM tombstones", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, TOMBSTONES as i64);
        storage.forget_account("a").unwrap();
        storage.save_messages("a", &[message(5, 1)]).unwrap();
        assert_eq!(ids(&storage), vec![1], "forgetting removes tombstones too");
    }

    #[test]
    fn a_failed_burst_rolls_back_messages_and_tombstones_together() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        storage.save_messages("a", &[message(5, 1)]).unwrap();
        storage
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_message BEFORE INSERT ON messages WHEN NEW.id = 99
             BEGIN SELECT RAISE(ABORT, 'test write failure'); END;",
            )
            .unwrap();
        assert!(storage
            .apply_messages(
                "a",
                &[
                    MessageMutation::Upsert(Box::new(message(5, 2))),
                    MessageMutation::Delete {
                        channel: ChannelId(5),
                        message: MessageId(1)
                    },
                    MessageMutation::Upsert(Box::new(message(5, 99))),
                ]
            )
            .is_err());
        assert_eq!(ids(&storage), vec![1]);
        let updated = Message {
            content: "still writable".into(),
            ..message(5, 1)
        };
        storage
            .save_messages("a", std::slice::from_ref(&updated))
            .unwrap();
        assert_eq!(
            storage.recent("a", ChannelId(5), 10).unwrap(),
            vec![updated]
        );
    }

    #[test]
    fn pruning_keeps_the_newest_of_each_conversation() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        let messages: Vec<Message> = (1..=50).map(|id| message(id % 2, id)).collect();
        storage.save_messages("a", &messages).unwrap();
        storage.prune("a", 5).unwrap();
        assert_eq!(storage.count("a").unwrap(), 10);
        let newest: Vec<u64> = storage
            .recent("a", ChannelId(0), 10)
            .unwrap()
            .iter()
            .map(|m| m.id.0)
            .collect();
        assert_eq!(newest, vec![42, 44, 46, 48, 50]);
    }

    #[test]
    fn pruning_scopes_repeated_message_ids_to_their_conversation_and_account() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        storage
            .save_messages("a", &[message(5, 1), message(5, 2), message(6, 1)])
            .unwrap();
        storage.save_messages("b", &[message(5, 1)]).unwrap();

        assert_eq!(storage.prune("a", 1).unwrap(), 1);
        assert_eq!(
            storage.recent("a", ChannelId(5), 10).unwrap(),
            vec![message(5, 2)]
        );
        assert_eq!(
            storage.recent("a", ChannelId(6), 10).unwrap(),
            vec![message(6, 1)]
        );
        assert_eq!(
            storage.recent("b", ChannelId(5), 10).unwrap(),
            vec![message(5, 1)]
        );

        assert_eq!(storage.prune("a", 0).unwrap(), 2);
        assert_eq!(storage.count("a").unwrap(), 0);
        assert_eq!(storage.count("b").unwrap(), 1);
    }

    #[test]
    fn settings_are_kept_replaced_and_forgotten() {
        let storage = Storage::in_memory().unwrap().unlocked();
        assert_eq!(storage.setting("tema").unwrap(), None);
        storage.set_setting("tema", Some("oscuro")).unwrap();
        storage.set_setting("tema", Some("claro")).unwrap();
        assert_eq!(storage.setting("tema").unwrap().as_deref(), Some("claro"));
        storage.set_setting("tema", None).unwrap();
        assert_eq!(storage.setting("tema").unwrap(), None);
        storage.set_setting("zoom", Some("125")).unwrap();
        storage.set_setting("cuenta:1:canal", Some("7")).unwrap();
        assert_eq!(
            storage.settings().unwrap(),
            [
                ("cuenta:1:canal".to_owned(), "7".to_owned()),
                ("zoom".to_owned(), "125".to_owned())
            ]
        );
    }

    #[test]
    fn drafts_and_snapshots_belong_to_their_account_and_go_with_it() {
        let storage = Storage::in_memory().unwrap().unlocked();
        storage
            .set_draft("a", ChannelId(5), Some("a medio escribir"))
            .unwrap();
        storage
            .set_draft("b", ChannelId(5), Some("otra cuenta"))
            .unwrap();
        storage.set_draft("a", ChannelId(6), Some("")).unwrap();
        assert_eq!(
            storage.drafts("a").unwrap(),
            vec![(ChannelId(5), "a medio escribir".to_owned())]
        );
        storage.save_snapshot("a", &vec![1, 2, 3]).unwrap();
        assert_eq!(
            storage.snapshot::<Vec<u32>>("a").unwrap(),
            Some(vec![1, 2, 3])
        );
        assert_eq!(
            storage.snapshot::<String>("a").unwrap(),
            None,
            "a snapshot that no longer reads is skipped"
        );
        storage.forget_account("a").unwrap();
        assert!(
            storage.drafts("a").unwrap().is_empty()
                && storage.snapshot::<Vec<u32>>("a").unwrap().is_none()
        );
        assert_eq!(storage.drafts("b").unwrap().len(), 1, "other accounts stay");
    }

    #[test]
    fn forgetting_an_account_rolls_back_if_one_table_cannot_be_cleared() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        storage.save_messages("a", &[message(5, 1)]).unwrap();
        storage
            .delete_message("a", ChannelId(5), MessageId(2))
            .unwrap();
        storage.set_draft("a", ChannelId(5), Some("draft")).unwrap();
        storage.save_snapshot("a", &[1, 2, 3]).unwrap();
        storage
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_forget BEFORE DELETE ON drafts
             BEGIN SELECT RAISE(ABORT, 'test forget failure'); END;",
            )
            .unwrap();
        assert!(storage.forget_account("a").is_err());
        assert_eq!(ids(&storage), vec![1]);
        assert_eq!(
            storage.drafts("a").unwrap(),
            vec![(ChannelId(5), "draft".into())]
        );
        assert_eq!(
            storage.snapshot::<Vec<u32>>("a").unwrap(),
            Some(vec![1, 2, 3])
        );
        storage.save_messages("a", &[message(5, 2)]).unwrap();
        assert_eq!(ids(&storage), vec![1], "the deletion was rolled back too");
    }

    #[test]
    fn a_first_version_file_is_migrated_and_a_newer_one_left_alone() {
        let path = temporary("migrate");
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        {
            // A store as the first version of RUNE wrote it.
            let old = Connection::open(&path).unwrap();
            old.execute_batch(
                "CREATE TABLE messages (account TEXT NOT NULL, channel INTEGER NOT NULL, id INTEGER NOT NULL, body TEXT NOT NULL, PRIMARY KEY (account, channel, id)) WITHOUT ROWID;
                 CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO settings VALUES ('tema', 'claro');
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        }
        let storage = Storage::open(&path).unwrap().unlocked();
        assert_eq!(
            storage.setting("tema").unwrap().as_deref(),
            Some("claro"),
            "what it had is kept"
        );
        storage
            .set_draft("a", ChannelId(1), Some("ya hay borradores"))
            .unwrap();
        // And the files of drafts, from the sixth version on.
        let kept = [DraftFile {
            path: "/home/vos/foto.png".into(),
            size_bytes: 12,
        }];
        storage.set_draft_files("a", ChannelId(1), &kept).unwrap();
        assert_eq!(
            storage.draft_files("a").unwrap(),
            vec![(ChannelId(1), kept.to_vec())]
        );
        drop(storage);
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        assert!(matches!(
            Storage::open(&path),
            Err(Error::Newer {
                found: 99,
                supported: SCHEMA
            })
        ));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_second_version_store_keeps_its_data_when_tombstones_are_added() {
        let old = Connection::open_in_memory().unwrap();
        old.execute_batch(
            "CREATE TABLE messages (account TEXT NOT NULL, channel INTEGER NOT NULL, id INTEGER NOT NULL, body TEXT NOT NULL, PRIMARY KEY (account, channel, id)) WITHOUT ROWID;
             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE drafts (account TEXT NOT NULL, channel INTEGER NOT NULL, text TEXT NOT NULL, PRIMARY KEY (account, channel)) WITHOUT ROWID;
             CREATE TABLE snapshots (account TEXT PRIMARY KEY, body TEXT NOT NULL);
             INSERT INTO settings VALUES ('tema', 'claro');
             INSERT INTO drafts VALUES ('a', 5, 'draft');
             INSERT INTO snapshots VALUES ('a', '[1,2,3]');
             PRAGMA user_version = 2;",
        )
        .unwrap();
        old.execute(
            "INSERT INTO messages VALUES ('a', 5, 1, ?1)",
            params![to_json(&message(5, 1)).unwrap()],
        )
        .unwrap();
        let mut storage = Storage::migrate(old).unwrap().unlocked();
        assert_eq!(ids(&storage), vec![1]);
        assert_eq!(storage.setting("tema").unwrap().as_deref(), Some("claro"));
        assert_eq!(
            storage.drafts("a").unwrap(),
            vec![(ChannelId(5), "draft".into())]
        );
        assert_eq!(
            storage.snapshot::<Vec<u32>>("a").unwrap(),
            Some(vec![1, 2, 3])
        );
        storage
            .delete_message("a", ChannelId(5), MessageId(1))
            .unwrap();
        storage.save_messages("a", &[message(5, 1)]).unwrap();
        assert!(ids(&storage).is_empty());
        let version: i32 = storage
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA);
    }

    #[test]
    fn migrating_v2_removes_previously_saved_ephemeral_snapshot_state() {
        let path = temporary("snapshot-privacy");
        let persisted = serde_json::json!({
            "me": { "id": 1, "name": "mock" },
            "presences": [[1, { "status": "online" }]],
            "voice": [{ "user": 1, "channel": 5 }],
            "last_messages": [[5, 50]],
            "future_field": { "kept": true }
        });
        {
            // A version 2 store kept it in plain text.
            let storage = Storage::open(&path).unwrap();
            storage.plain(&[("snapshots", "legacy", 0, 0, persisted.to_string())]);
            storage
                .connection
                .execute_batch("DROP TABLE tombstones; PRAGMA user_version = 2;")
                .unwrap();
            // An unreadable old snapshot must not block the other migrations.
            storage
                .connection
                .execute(
                    "INSERT INTO snapshots VALUES ('invalid', 'malformed JSON')",
                    [],
                )
                .unwrap();
        }
        {
            // A legacy namespace is never unlocked: it only moves.
            let storage = Storage::open(&path).unwrap();
            storage.migrate_account("legacy", "scoped").unwrap();
        }
        let storage = Storage::open(&path).unwrap().unlocked();
        let restored = storage
            .snapshot::<serde_json::Value>("scoped")
            .unwrap()
            .unwrap();
        assert_eq!(
            restored["presences"],
            serde_json::json!([]),
            "previously saved presence must be removed during migration"
        );
        assert_eq!(restored["voice"], serde_json::json!([]));
        assert_eq!(restored["me"], persisted["me"]);
        assert_eq!(restored["last_messages"], persisted["last_messages"]);
        assert_eq!(restored["future_field"], persisted["future_field"]);
        drop(storage);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn account_migration_preserves_newer_data_and_cannot_revive_tombstones() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        // A legacy namespace predates sealing.
        storage.plain(&[
            ("messages", "legacy", 5, 1, to_json(&message(5, 1)).unwrap()),
            ("messages", "legacy", 5, 2, to_json(&message(5, 2)).unwrap()),
            ("drafts", "legacy", 5, 0, "old draft".into()),
            ("snapshots", "legacy", 0, 0, "[1]".into()),
        ]);
        let mut newer = message(5, 1);
        newer.content = "new namespace".into();
        newer.edited = Some(rune_model::Timestamp(100));
        storage.save_messages("scoped", &[newer.clone()]).unwrap();
        storage
            .delete_message("scoped", ChannelId(5), MessageId(2))
            .unwrap();
        storage
            .set_draft("scoped", ChannelId(5), Some("new draft"))
            .unwrap();
        storage.migrate_account("legacy", "scoped").unwrap();
        assert_eq!(
            storage.recent("scoped", ChannelId(5), 10).unwrap(),
            vec![newer]
        );
        assert_eq!(
            storage.drafts("scoped").unwrap(),
            vec![(ChannelId(5), "new draft".into())]
        );
        assert_eq!(
            storage.snapshot::<Vec<u32>>("scoped").unwrap(),
            Some(vec![1])
        );
        assert!(storage
            .recent("legacy", ChannelId(5), 10)
            .unwrap()
            .is_empty());
        assert!(storage.drafts("legacy").unwrap().is_empty());
        assert!(storage.snapshot::<Vec<u32>>("legacy").unwrap().is_none());
        storage.migrate_account("legacy", "scoped").unwrap();
        assert_eq!(storage.recent("scoped", ChannelId(5), 10).unwrap().len(), 1);
    }

    #[test]
    fn legacy_tombstones_do_not_evict_more_recent_destination_deletions() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        storage
            .delete_message("legacy", ChannelId(5), MessageId(1))
            .unwrap();
        for id in 1000..1256 {
            storage
                .delete_message("scoped", ChannelId(5), MessageId(id))
                .unwrap();
        }
        storage.migrate_account("legacy", "scoped").unwrap();
        storage
            .save_messages("scoped", &[message(5, 1000)])
            .unwrap();
        assert!(storage
            .recent("scoped", ChannelId(5), 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn account_migration_is_atomic_if_the_destination_rejects_a_draft() {
        let storage = Storage::in_memory().unwrap().unlocked();
        storage.plain(&[
            ("messages", "legacy", 5, 1, to_json(&message(5, 1)).unwrap()),
            ("drafts", "legacy", 5, 0, "keep me".into()),
        ]);
        storage.connection.execute_batch(
            "CREATE TRIGGER refuse_migration BEFORE INSERT ON drafts WHEN NEW.account = 'scoped'
             BEGIN SELECT RAISE(ABORT, 'fixture'); END;"
        ).unwrap();
        assert!(storage.migrate_account("legacy", "scoped").is_err());
        assert_eq!(storage.count("legacy").unwrap(), 1);
        let drafts: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM drafts WHERE account = 'legacy'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(drafts, 1);
        assert!(storage
            .recent("scoped", ChannelId(5), 10)
            .unwrap()
            .is_empty());
    }

    /// Whether `marker` is anywhere in the store's files, its log included.
    fn on_disk(path: &Path, marker: &str) -> bool {
        [path.to_path_buf(), path.with_extension("db-wal")]
            .iter()
            .filter_map(|file| std::fs::read(file).ok())
            .any(|bytes| {
                bytes
                    .windows(marker.len())
                    .any(|window| window == marker.as_bytes())
            })
    }

    #[test]
    fn what_an_account_keeps_is_sealed_and_needs_its_own_key() {
        let path = temporary("sealed");
        const MARKER: &str = "SECRETO_EN_DISCO";
        let kept = Message {
            content: MARKER.into(),
            ..message(5, 1)
        };
        let mark = rune_model::ReadMark {
            through: Some(MessageId(1)),
            unread_from: None,
            revision: 3,
        };
        {
            let mut storage = Storage::open(&path).unwrap().unlocked();
            storage
                .apply_messages(
                    "a",
                    &[
                        MessageMutation::Upsert(Box::new(kept.clone())),
                        MessageMutation::Found(vec![Message {
                            content: MARKER.into(),
                            ..message(5, 900)
                        }]),
                    ],
                )
                .unwrap();
            storage.set_draft("a", ChannelId(5), Some(MARKER)).unwrap();
            storage.save_snapshot("a", &[MARKER]).unwrap();
            storage.set_read_mark("a", ChannelId(5), mark).unwrap();
        }
        assert!(!on_disk(&path, MARKER), "nothing of it in plain text");
        let mut storage = Storage::open(&path).unwrap();
        assert!(matches!(
            storage.recent("a", ChannelId(5), 10),
            Err(Error::Locked)
        ));
        assert!(matches!(storage.drafts("a"), Err(Error::Locked)));
        assert!(matches!(
            storage.set_draft("a", ChannelId(5), Some("x")),
            Err(Error::Locked)
        ));
        assert!(matches!(
            storage.save_messages("a", &[message(5, 2)]),
            Err(Error::Locked)
        ));
        assert!(matches!(
            storage.snapshot::<Vec<String>>("a"),
            Err(Error::Locked)
        ));
        assert!(matches!(storage.read_marks("a"), Err(Error::Locked)));
        // Another key opens nothing.
        storage.unlock("a", Key::generate().unwrap()).unwrap();
        assert!(storage.recent("a", ChannelId(5), 10).unwrap().is_empty());
        assert!(storage.drafts("a").unwrap().is_empty());
        assert_eq!(storage.snapshot::<Vec<String>>("a").unwrap(), None);
        assert!(matches!(
            storage.read_marks("a"),
            Err(Error::InvalidReadMark)
        ));
        // Its own does.
        storage.unlock("a", test_key()).unwrap();
        assert_eq!(storage.recent("a", ChannelId(5), 10).unwrap(), vec![kept]);
        assert_eq!(
            storage.drafts("a").unwrap(),
            vec![(ChannelId(5), MARKER.to_owned())]
        );
        assert_eq!(
            storage.snapshot::<Vec<String>>("a").unwrap(),
            Some(vec![MARKER.to_owned()])
        );
        assert_eq!(storage.read_marks("a").unwrap(), vec![(ChannelId(5), mark)]);
        // A row copied to another account's place does not open there.
        storage
            .connection
            .execute(
                "INSERT INTO messages SELECT 'b', channel, id, body FROM messages WHERE account = 'a'",
                [],
            )
            .unwrap();
        let storage = storage.unlocked();
        assert!(storage.recent("b", ChannelId(5), 10).unwrap().is_empty());
        drop(storage);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_new_key_drops_what_a_lost_one_sealed_and_still_seals_plain_rows() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        let mark = rune_model::ReadMark {
            through: Some(MessageId(1)),
            unread_from: None,
            revision: 1,
        };
        storage
            .apply_messages(
                "a",
                &[
                    MessageMutation::Upsert(Box::new(message(5, 1))),
                    MessageMutation::Found(vec![message(5, 900)]),
                ],
            )
            .unwrap();
        storage
            .set_draft("a", ChannelId(5), Some("perdido"))
            .unwrap();
        storage.save_snapshot("a", &["perdida"]).unwrap();
        storage.set_read_mark("a", ChannelId(5), mark).unwrap();
        storage
            .set_draft("b", ChannelId(5), Some("de otra"))
            .unwrap();
        storage.plain(&[("drafts", "a", 6, 0, "de v4".into())]);
        storage.lock();
        // The keyring lost the key that sealed them: one made now.
        storage.unlock_new("a", Key::generate().unwrap()).unwrap();
        let query = rune_model::SearchQuery {
            channel: ChannelId(5),
            text: "hola".into(),
            ..Default::default()
        };
        // No read meets a row it cannot open, so none fails.
        let chunk = storage
            .search_chunk("a", &query, None, 20, &|| false)
            .unwrap();
        assert!(chunk.hits.is_empty());
        assert_eq!(storage.read_marks("a").unwrap(), Vec::new());
        assert_eq!(storage.count("a").unwrap(), 0);
        assert_eq!(storage.snapshot::<Vec<String>>("a").unwrap(), None);
        assert_eq!(
            storage.drafts("a").unwrap(),
            vec![(ChannelId(6), "de v4".to_owned())],
            "plain text is sealed, not dropped"
        );
        storage.unlock("b", test_key()).unwrap();
        assert_eq!(
            storage.drafts("b").unwrap(),
            vec![(ChannelId(5), "de otra".to_owned())],
            "another account keeps its own"
        );
    }

    #[test]
    fn a_version_five_store_gets_the_files_of_drafts_and_keeps_what_it_sealed() {
        let path = temporary("version-five");
        {
            // As version 5 left it: sealed, without the files of drafts.
            let storage = Storage::open(&path).unwrap().unlocked();
            storage
                .set_draft("a", ChannelId(5), Some("sellado en v5"))
                .unwrap();
            storage
                .connection
                .execute_batch("DROP TABLE draft_files; PRAGMA user_version = 5;")
                .unwrap();
        }
        let storage = Storage::open(&path).unwrap().unlocked();
        let version: i32 = storage
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA);
        assert_eq!(
            storage.drafts("a").unwrap(),
            vec![(ChannelId(5), "sellado en v5".to_owned())]
        );
        let kept = [DraftFile {
            path: "/home/vos/foto.png".into(),
            size_bytes: 12,
        }];
        storage.set_draft_files("a", ChannelId(5), &kept).unwrap();
        assert_eq!(
            storage.draft_files("a").unwrap(),
            vec![(ChannelId(5), kept.to_vec())]
        );
        drop(storage);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_version_four_store_is_sealed_when_its_account_is_unlocked() {
        let path = temporary("version-four");
        const MARKER: &str = "TEXTO_PLANO_V4";
        let kept = Message {
            content: MARKER.into(),
            ..message(5, 1)
        };
        let mark = rune_model::ReadMark {
            through: Some(MessageId(1)),
            unread_from: Some(MessageId(1)),
            revision: 2,
        };
        {
            // As version 4 left it: every account's rows in plain text.
            let storage = Storage::open(&path).unwrap();
            storage
                .connection
                .execute_batch("DROP TABLE found; PRAGMA user_version = 4;")
                .unwrap();
            storage.plain(&[
                ("messages", "a", 5, 1, to_json(&kept).unwrap()),
                ("drafts", "a", 5, 0, MARKER.into()),
                ("snapshots", "a", 0, 0, format!("[\"{MARKER}\"]")),
                ("read_marks", "a", 5, 0, to_json(&mark).unwrap()),
                ("drafts", "b", 5, 0, "otra cuenta".into()),
            ]);
        }
        let mut storage = Storage::open(&path).unwrap();
        let version: i32 = storage
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA);
        assert_eq!(storage.count("a").unwrap(), 1, "the rows are kept");
        assert!(matches!(storage.drafts("a"), Err(Error::Locked)));
        storage.unlock("a", test_key()).unwrap();
        let plain = |account: &str| -> i64 {
            storage
                .connection
                .query_row(
                    "SELECT (SELECT COUNT(*) FROM messages WHERE account = ?1 AND typeof(body) = 'text')
                          + (SELECT COUNT(*) FROM drafts WHERE account = ?1 AND typeof(text) = 'text')
                          + (SELECT COUNT(*) FROM snapshots WHERE account = ?1 AND typeof(body) = 'text')
                          + (SELECT COUNT(*) FROM read_marks WHERE account = ?1 AND typeof(body) = 'text')",
                    params![account],
                    |row| row.get(0),
                )
                .unwrap()
        };
        assert_eq!(plain("a"), 0, "every row of the account is sealed");
        assert_eq!(plain("b"), 1, "another account waits for its own key");
        assert_eq!(storage.recent("a", ChannelId(5), 10).unwrap(), vec![kept]);
        assert_eq!(
            storage.drafts("a").unwrap(),
            vec![(ChannelId(5), MARKER.to_owned())]
        );
        assert_eq!(
            storage.snapshot::<Vec<String>>("a").unwrap(),
            Some(vec![MARKER.to_owned()])
        );
        assert_eq!(storage.read_marks("a").unwrap(), vec![(ChannelId(5), mark)]);
        drop(storage);
        assert!(!on_disk(&path, MARKER), "the plain text is overwritten");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn what_a_search_found_is_kept_apart_and_never_joins_a_page() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        storage
            .apply_messages("a", &[history(None, 90..=100, false)])
            .unwrap();
        let edited = Message {
            content: "hola, editado".into(),
            edited: Some(Timestamp(500)),
            ..message(5, 95)
        };
        storage
            .apply_messages(
                "a",
                &[MessageMutation::Found(vec![
                    message(5, 3),
                    message(5, 40),
                    edited.clone(),
                ])],
            )
            .unwrap();
        // The history's run, and its bounds, stay as they were; a message it
        // held is updated there.
        assert_eq!(ids(&storage), (90..=100).collect::<Vec<_>>());
        assert_eq!(storage.recent("a", ChannelId(5), 100).unwrap()[5], edited);
        let found = |storage: &Storage| -> Vec<u64> {
            let query = rune_model::SearchQuery {
                channel: ChannelId(5),
                text: "hola".into(),
                ..Default::default()
            };
            let mut found = Vec::new();
            let mut before = None;
            loop {
                let chunk = storage
                    .search_chunk("a", &query, before, 20, &|| false)
                    .unwrap();
                found.extend(chunk.hits.iter().map(|hit| hit.id.0));
                if !chunk.more {
                    return found;
                }
                before = chunk.before;
            }
        };
        // A search reads it with the history.
        assert!(found(&storage).contains(&3) && found(&storage).contains(&40));
        // A newest page cannot take it for a neighbor.
        storage
            .apply_messages("a", &[history(None, 91..=101, false)])
            .unwrap();
        assert!(!ids(&storage).contains(&3) && !ids(&storage).contains(&40));
        // A page whose range holds one it found, and leaves it out, proves it
        // gone, though no deletion was seen; outside that range it stays.
        storage
            .apply_messages("a", &[MessageMutation::Found(vec![message(5, 60)])])
            .unwrap();
        assert!(found(&storage).contains(&60));
        storage
            .apply_messages("a", &[history(Some(91), [50, 70], false)])
            .unwrap();
        assert!(!found(&storage).contains(&60));
        assert!(found(&storage).contains(&3) && found(&storage).contains(&40));
        // An edit reaches it; a deletion takes it and keeps it from returning.
        storage
            .apply_messages(
                "a",
                &[
                    MessageMutation::Patch(Box::new(MessagePatch {
                        content: Some("hola, corregido".into()),
                        ..patch(3)
                    })),
                    MessageMutation::Delete {
                        channel: ChannelId(5),
                        message: MessageId(40),
                    },
                    MessageMutation::Found(vec![message(5, 40)]),
                ],
            )
            .unwrap();
        assert!(found(&storage).contains(&3) && !found(&storage).contains(&40));
        // A page that brings it makes it history, and it leaves the found ones.
        storage
            .apply_messages("a", &[history(Some(91), 1..=10, true)])
            .unwrap();
        assert!(ids(&storage).contains(&3));
        let apart: i64 = storage
            .connection
            .query_row("SELECT COUNT(*) FROM found", [], |row| row.get(0))
            .unwrap();
        assert_eq!(apart, 0);
        // Pruning bounds it, and forgetting the account takes it.
        storage
            .apply_messages(
                "a",
                &[MessageMutation::Found(
                    (200..210).map(|id| message(6, id)).collect(),
                )],
            )
            .unwrap();
        storage.prune("a", 4).unwrap();
        let count = |storage: &Storage| -> i64 {
            storage
                .connection
                .query_row("SELECT COUNT(*) FROM found", [], |row| row.get(0))
                .unwrap()
        };
        assert_eq!(count(&storage), 4);
        storage.forget_account("a").unwrap();
        assert_eq!(count(&storage), 0);
    }

    #[test]
    fn a_file_store_survives_reopening_and_is_private() {
        let path = temporary("reopen");
        {
            let mut storage = Storage::open(&path).unwrap().unlocked();
            storage.save_messages("a", &[message(5, 1)]).unwrap();
        }
        let storage = Storage::open(&path).unwrap().unlocked();
        assert_eq!(storage.recent("a", ChannelId(5), 1).unwrap().len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&path), 0o600);
            assert_eq!(mode(path.parent().unwrap()), 0o700);
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
