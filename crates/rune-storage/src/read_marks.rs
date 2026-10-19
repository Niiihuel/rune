//! Account-local choices are independent of provider snapshots and message pruning.

use rune_model::{ChannelId, ReadMark};
use rusqlite::{params, OptionalExtension};

use super::sealed::{Place, OVERHEAD};
use super::{Error, Storage, MAX_READ_MARKS as MAX_MARKS};

/// A sealed mark: its JSON, at most 4 KiB, and what sealing adds.
const MAX_BODY_BYTES: usize = 4096 + OVERHEAD;

impl Storage {
    /// Bounded startup restore. Corruption never masquerades as an empty account.
    pub fn read_marks(&self, account: &str) -> Result<Vec<(ChannelId, ReadMark)>, Error> {
        let key = self.key(account)?;
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM read_marks WHERE account=?1",
            params![account],
            |row| row.get(0),
        )?;
        if count > MAX_MARKS as i64 {
            return Err(Error::ReadMarksTooLarge);
        }
        let mut statement = self.connection.prepare_cached(
            "SELECT channel, body FROM read_marks WHERE account=?1 ORDER BY channel LIMIT ?2",
        )?;
        let mut rows = statement.query(params![account, MAX_MARKS as i64 + 1])?;
        // Power-of-two growth never exceeds MAX_MARKS, even if another SQLite
        // connection adds rows between the count and this bounded SELECT.
        let mut marks = Vec::new();
        while let Some(row) = rows.next()? {
            if marks.len() == MAX_MARKS {
                return Err(Error::ReadMarksTooLarge);
            }
            let channel: i64 = row.get(0)?;
            let value = row.get_ref(1)?;
            if value.as_bytes().map_err(|_| Error::InvalidReadMark)?.len() > MAX_BODY_BYTES {
                return Err(Error::ReadMarksTooLarge);
            }
            let body = key
                .open(&Place::read_mark(account, channel), value)
                .ok_or(Error::InvalidReadMark)?;
            let mark = serde_json::from_str(&body).map_err(|_| Error::InvalidReadMark)?;
            marks.push((ChannelId(channel as u64), mark));
        }
        marks.sort_unstable_by_key(|(channel, _)| *channel);
        Ok(marks)
    }

    /// A delayed local revision cannot replace a newer choice. Provider versions
    /// are deliberately absent from this local table.
    pub fn set_read_mark(
        &self,
        account: &str,
        channel: ChannelId,
        mark: ReadMark,
    ) -> Result<(), Error> {
        let key = self.key(account)?;
        let place = Place::read_mark(account, channel.0 as i64);
        let transaction = self.connection.unchecked_transaction()?;
        let previous: Option<ReadMark> = transaction
            .query_row(
                "SELECT body FROM read_marks WHERE account=?1 AND channel=?2",
                params![account, channel.0 as i64],
                |row| {
                    Ok(row
                        .get_ref(0)
                        .ok()
                        .filter(|value| {
                            value
                                .as_bytes()
                                .is_ok_and(|body| body.len() <= MAX_BODY_BYTES)
                        })
                        .and_then(|value| super::open_json(key, &place, value)))
                },
            )
            .optional()?
            .flatten();
        if previous.is_none_or(|previous| previous.revision <= mark.revision) {
            transaction.execute(
                "INSERT INTO read_marks(account,channel,body) VALUES(?1,?2,?3)
                 ON CONFLICT(account,channel) DO UPDATE SET body=excluded.body",
                params![
                    account,
                    channel.0 as i64,
                    super::seal_json(key, &place, &mark)?
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::MessageId;
    use rusqlite::params;

    fn mark(revision: u64) -> ReadMark {
        ReadMark {
            through: Some(MessageId(100)),
            unread_from: Some(MessageId(80)),
            revision,
        }
    }

    fn message(id: u64) -> rune_model::Message {
        rune_model::Message {
            id: MessageId(id),
            channel: ChannelId(5),
            author: rune_model::UserId(2),
            content: "kept".into(),
            timestamp: rune_model::Timestamp(1),
            edited: None,
            kind: rune_model::MessageKind::Default,
            reply_to: None,
            mentions: vec![],
            reactions: vec![],
            attachments: vec![],
            stickers: vec![],
            text_references: vec![],
            delivery: rune_model::Delivery::Sent,
            pinned: false,
            embeds: Vec::new(),
            forwarded: None,
            poll: None,
            components: Default::default(),
            call: None,
            mention_roles: Vec::new(),
            mention_everyone: false,
        }
    }

    #[test]
    fn read_marks_survive_restart_and_are_scoped_to_account_and_channel() {
        let directory =
            std::env::temp_dir().join(format!("rune-read-marks-{}", std::process::id()));
        let path = directory.join("rune.db");
        {
            let storage = Storage::open(&path).unwrap().unlocked();
            storage.set_read_mark("a", ChannelId(5), mark(3)).unwrap();
            storage.set_read_mark("b", ChannelId(5), mark(7)).unwrap();
            storage
                .set_read_mark("a", ChannelId(u64::MAX), mark(9))
                .unwrap();
        }
        let storage = Storage::open(&path).unwrap().unlocked();
        assert_eq!(
            storage.read_marks("a").unwrap(),
            vec![(ChannelId(5), mark(3)), (ChannelId(u64::MAX), mark(9))]
        );
        assert_eq!(
            storage.read_marks("b").unwrap(),
            vec![(ChannelId(5), mark(7))]
        );
        assert!(storage.read_marks("missing").unwrap().is_empty());
        drop(storage);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn newer_local_revisions_win_without_rewinding_the_through_cursor() {
        let storage = Storage::in_memory().unwrap().unlocked();
        let newest = ReadMark {
            through: Some(MessageId(u64::MAX)),
            unread_from: Some(MessageId(1)),
            revision: u64::MAX,
        };
        storage.set_read_mark("a", ChannelId(5), newest).unwrap();
        storage
            .set_read_mark("a", ChannelId(5), mark(u64::MAX - 1))
            .unwrap();
        storage.set_read_mark("a", ChannelId(5), newest).unwrap();
        assert_eq!(
            storage.read_marks("a").unwrap(),
            vec![(ChannelId(5), newest)]
        );
        let clear = ReadMark {
            through: Some(MessageId(50)),
            unread_from: None,
            revision: 2,
        };
        storage.set_read_mark("b", ChannelId(5), mark(1)).unwrap();
        storage.set_read_mark("b", ChannelId(5), clear).unwrap();
        assert_eq!(
            storage.read_marks("b").unwrap(),
            vec![(ChannelId(5), clear)],
            "revision orders choices; cursors are not combined with max"
        );
    }

    #[test]
    fn saturated_local_revisions_keep_the_last_choice_after_restart() {
        let directory =
            std::env::temp_dir().join(format!("rune-read-marks-saturated-{}", std::process::id()));
        let path = directory.join("rune.db");
        let latest = ReadMark {
            through: Some(MessageId(100)),
            unread_from: Some(MessageId(90)),
            revision: u64::MAX,
        };
        {
            let storage = Storage::open(&path).unwrap().unlocked();
            storage
                .set_read_mark("a", ChannelId(5), mark(u64::MAX))
                .unwrap();
            storage.set_read_mark("a", ChannelId(5), latest).unwrap();
        }
        {
            let storage = Storage::open(&path).unwrap().unlocked();
            assert_eq!(
                storage.read_marks("a").unwrap(),
                vec![(ChannelId(5), latest)]
            );
            storage
                .set_read_mark("a", ChannelId(5), mark(u64::MAX - 1))
                .unwrap();
        }
        let storage = Storage::open(&path).unwrap().unlocked();
        assert_eq!(
            storage.read_marks("a").unwrap(),
            vec![(ChannelId(5), latest)]
        );
        drop(storage);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn version_three_migrates_atomically_without_rewriting_prior_account_data() {
        let mut old = Storage::in_memory().unwrap().unlocked();
        old.save_messages("a", &[message(1), message(2)]).unwrap();
        old.delete_message("a", ChannelId(5), MessageId(2)).unwrap();
        old.set_draft("a", ChannelId(5), Some("draft")).unwrap();
        old.set_setting("zoom", Some("1.2")).unwrap();
        old.save_snapshot("a", &serde_json::json!({"keep":"snapshot"}))
            .unwrap();
        old.connection
            .execute_batch("DROP TABLE IF EXISTS read_marks; PRAGMA user_version=3;")
            .unwrap();
        let storage = Storage::migrate(old.connection).unwrap().unlocked();
        assert_eq!(
            storage
                .connection
                .pragma_query_value::<i32, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            crate::SCHEMA
        );
        assert!(storage.read_marks("a").unwrap().is_empty());
        assert_eq!(storage.recent("a", ChannelId(5), 10).unwrap().len(), 1);
        assert_eq!(
            storage.drafts("a").unwrap(),
            vec![(ChannelId(5), "draft".into())]
        );
        assert_eq!(storage.setting("zoom").unwrap(), Some("1.2".into()));
        assert_eq!(
            storage.snapshot::<serde_json::Value>("a").unwrap().unwrap()["keep"],
            "snapshot"
        );
        assert_eq!(
            storage
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM tombstones WHERE account='a'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn namespace_migration_and_forget_include_read_marks() {
        let storage = Storage::in_memory().unwrap().unlocked();
        // A legacy namespace predates sealing.
        let plain = |channel, revision| {
            (
                "read_marks",
                "legacy",
                channel,
                0,
                super::super::to_json(&mark(revision)).unwrap(),
            )
        };
        storage.plain(&[plain(5, 99), plain(6, 3)]);
        storage
            .set_read_mark("stable", ChannelId(5), mark(2))
            .unwrap();
        storage
            .set_read_mark("other", ChannelId(5), mark(7))
            .unwrap();
        storage.migrate_account("legacy", "stable").unwrap();
        assert!(storage.read_marks("legacy").unwrap().is_empty());
        assert_eq!(
            storage.read_marks("stable").unwrap(),
            vec![(ChannelId(5), mark(2)), (ChannelId(6), mark(3))]
        );
        storage.forget_account("stable").unwrap();
        assert!(storage.read_marks("stable").unwrap().is_empty());
        assert_eq!(
            storage.read_marks("other").unwrap(),
            vec![(ChannelId(5), mark(7))]
        );
    }

    #[test]
    fn pruning_or_deleting_messages_does_not_erase_manual_reminders() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        storage.save_messages("a", &[message(80)]).unwrap();
        storage.set_read_mark("a", ChannelId(5), mark(1)).unwrap();
        storage
            .delete_message("a", ChannelId(5), MessageId(80))
            .unwrap();
        storage.prune("a", 0).unwrap();
        assert_eq!(
            storage.read_marks("a").unwrap(),
            vec![(ChannelId(5), mark(1))]
        );
    }

    #[test]
    fn default_rows_restore_but_corruption_is_explicit_and_repairable() {
        let mut storage = Storage::in_memory().unwrap();
        storage
            .connection
            .execute(
                "INSERT INTO read_marks(account,channel,body) VALUES('a',5,'{}')",
                [],
            )
            .unwrap();
        // Sealed as an older version's row is.
        storage.unlock("a", super::super::test_key()).unwrap();
        assert_eq!(
            storage.read_marks("a").unwrap(),
            vec![(ChannelId(5), ReadMark::default())]
        );
        storage
            .connection
            .execute("UPDATE read_marks SET body='broken'", [])
            .unwrap();
        assert!(matches!(
            storage.read_marks("a"),
            Err(Error::InvalidReadMark)
        ));
        storage.set_read_mark("a", ChannelId(5), mark(1)).unwrap();
        assert_eq!(
            storage.read_marks("a").unwrap(),
            vec![(ChannelId(5), mark(1))]
        );
        storage
            .connection
            .execute(
                "UPDATE read_marks SET body=?1",
                params![vec![1_u8; MAX_BODY_BYTES + 1]],
            )
            .unwrap();
        assert!(matches!(
            storage.read_marks("a"),
            Err(Error::ReadMarksTooLarge)
        ));
    }

    #[test]
    fn startup_read_mark_count_limit_is_explicit() {
        let mut storage = Storage::in_memory().unwrap();
        storage.connection.execute("WITH RECURSIVE n(id) AS (SELECT 1 UNION ALL SELECT id+1 FROM n WHERE id<?1) INSERT INTO read_marks(account,channel,body) SELECT 'a',id,'{}' FROM n", params![MAX_MARKS as i64]).unwrap();
        storage.unlock("a", super::super::test_key()).unwrap();
        assert_eq!(storage.read_marks("a").unwrap().len(), MAX_MARKS);
        storage
            .connection
            .execute(
                "INSERT INTO read_marks(account,channel,body) VALUES('a',?1,'{}')",
                params![MAX_MARKS as i64 + 1],
            )
            .unwrap();
        assert!(matches!(
            storage.read_marks("a"),
            Err(Error::ReadMarksTooLarge)
        ));
    }
}
