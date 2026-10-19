//! The files chosen for a draft, kept beside its text so they come back
//! after a restart (envios-y-subidas): where each one is and how large it
//! was, never its bytes, sealed with the account's key as the text is.
//! Whoever restores them checks the file again.

use rune_model::ChannelId;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{open_json, seal_json, Error, Place, Storage};

/// The most files one draft keeps, as a message carries.
const MAX_FILES: usize = 10;
/// A row past this is not read back.
const MAX_BODY_BYTES: usize = 64 * 1024;

/// A file of a draft, as it was chosen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftFile {
    /// Its absolute path.
    pub path: String,
    /// Its size when it was chosen: a file of another size changed since.
    #[serde(default)]
    pub size_bytes: u64,
}

impl Storage {
    /// Keep `files` as what `channel`'s draft of `account` carries; none
    /// forgets them.
    pub fn set_draft_files(
        &self,
        account: &str,
        channel: ChannelId,
        files: &[DraftFile],
    ) -> Result<(), Error> {
        let key = self.key(account)?;
        if files.is_empty() {
            self.connection.execute(
                "DELETE FROM draft_files WHERE account = ?1 AND channel = ?2",
                params![account, channel.0 as i64],
            )?;
        } else {
            self.connection.execute(
                "INSERT INTO draft_files (account, channel, body) VALUES (?1, ?2, ?3)
                 ON CONFLICT(account, channel) DO UPDATE SET body = excluded.body",
                params![
                    account,
                    channel.0 as i64,
                    seal_json(
                        key,
                        &Place::draft_files(account, channel.0 as i64),
                        &&files[..files.len().min(MAX_FILES)]
                    )?
                ],
            )?;
        }
        Ok(())
    }

    /// Every draft's files kept for `account`; a row that no longer opens
    /// or reads is skipped, as a lost choice is better than none restored.
    pub fn draft_files(&self, account: &str) -> Result<Vec<(ChannelId, Vec<DraftFile>)>, Error> {
        let key = self.key(account)?;
        let mut query = self
            .connection
            .prepare_cached("SELECT channel, body FROM draft_files WHERE account = ?1")?;
        let mut rows = query.query(params![account])?;
        let mut kept = Vec::new();
        while let Some(row) = rows.next()? {
            let channel: i64 = row.get(0)?;
            let body = row.get_ref(1)?;
            let files = (body.as_bytes().map_or(0, <[u8]>::len) <= MAX_BODY_BYTES)
                .then(|| {
                    open_json::<Vec<DraftFile>>(key, &Place::draft_files(account, channel), body)
                })
                .flatten()
                .filter(|files| !files.is_empty() && files.len() <= MAX_FILES);
            if let Some(files) = files {
                kept.push((ChannelId(channel as u64), files));
            }
        }
        Ok(kept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: u64) -> DraftFile {
        DraftFile {
            path: format!("/home/vos/{name}"),
            size_bytes: size,
        }
    }

    /// How `account`'s rows are kept: SQLite's type of each body.
    fn kinds(storage: &Storage, account: &str) -> Vec<String> {
        storage
            .connection
            .prepare("SELECT typeof(body) FROM draft_files WHERE account = ?1")
            .unwrap()
            .query_map(params![account], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn draft_files_are_sealed_belong_to_their_account_and_go_with_it() {
        let storage = Storage::in_memory().unwrap().unlocked();
        let both = [file("contrato.pdf", 12), file("notas.txt", 3)];
        storage.set_draft_files("a", ChannelId(5), &both).unwrap();
        storage
            .set_draft_files("b", ChannelId(5), &[file("otra.png", 1)])
            .unwrap();
        storage.set_draft_files("a", ChannelId(6), &[]).unwrap();
        assert_eq!(
            storage.draft_files("a").unwrap(),
            vec![(ChannelId(5), both.to_vec())]
        );
        // Sealed: neither the path nor the name is in the file.
        assert_eq!(kinds(&storage, "a"), ["blob"]);
        let body: Vec<u8> = storage
            .connection
            .query_row(
                "SELECT body FROM draft_files WHERE account = 'a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!body.windows(8).any(|window| window == b"contrato"));
        // Sent or removed: none left.
        storage.set_draft_files("a", ChannelId(5), &[]).unwrap();
        assert!(storage.draft_files("a").unwrap().is_empty());
        storage.set_draft_files("a", ChannelId(5), &both).unwrap();
        storage.forget_account("a").unwrap();
        assert!(storage.draft_files("a").unwrap().is_empty());
        assert_eq!(
            storage.draft_files("b").unwrap().len(),
            1,
            "other accounts stay"
        );
        // Without the account's key, nothing is written or read.
        let locked = Storage::in_memory().unwrap();
        assert!(matches!(
            locked.set_draft_files("a", ChannelId(5), &both),
            Err(Error::Locked)
        ));
        assert!(matches!(locked.draft_files("a"), Err(Error::Locked)));
    }

    #[test]
    fn plain_rows_are_sealed_on_unlock_and_one_that_does_not_read_is_skipped() {
        let mut storage = Storage::in_memory().unwrap();
        for (account, channel, body) in [
            ("a", 1, "no es json"),
            ("a", 2, "[]"),
            ("a", 3, r#"[{"path":"/home/vos/vieja.png"}]"#),
            (
                "legacy",
                4,
                r#"[{"path":"/home/vos/movida.png","size_bytes":2}]"#,
            ),
        ] {
            storage
                .connection
                .execute(
                    "INSERT INTO draft_files VALUES (?1, ?2, ?3)",
                    params![account, channel, body],
                )
                .unwrap();
        }
        storage.unlock("a", crate::test_key()).unwrap();
        assert_eq!(kinds(&storage, "a"), ["blob"; 3]);
        assert_eq!(
            storage.draft_files("a").unwrap(),
            vec![(ChannelId(3), vec![file("vieja.png", 0)])]
        );
        // Copied to another account, a sealed row does not open there.
        storage
            .connection
            .execute(
                "INSERT INTO draft_files SELECT 'scoped', channel, body FROM draft_files
                 WHERE account = 'a' AND channel = 3",
                [],
            )
            .unwrap();
        storage.unlock("scoped", crate::test_key()).unwrap();
        assert!(storage.draft_files("scoped").unwrap().is_empty());
        // A legacy namespace's move seals it for the destination.
        storage.forget_account("scoped").unwrap();
        storage.migrate_account("legacy", "scoped").unwrap();
        assert_eq!(kinds(&storage, "scoped"), ["blob"]);
        assert_eq!(
            storage.draft_files("scoped").unwrap(),
            vec![(ChannelId(4), vec![file("movida.png", 2)])]
        );
    }
}
