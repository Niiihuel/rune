//! Short, bounded reads. No SQLite statement survives a call into this module.

use rune_model::{
    CachedContext, ChannelId, LocalReadError, Message, MessageId, SearchHit, SearchQuery,
};
use rusqlite::{params, Connection};

use super::sealed::Place;
use super::{Key, Storage};

const SCAN_ROWS: usize = 32;
// Bound a single legacy/corrupt row before serde allocates its nested fields.
const MAX_ROW_BYTES: usize = 1024 * 1024;
const CONTEXT_RAW_BYTES: usize = 1024 * 1024;

/// A scan checkpoint is private to the storage worker, not a presentation cursor.
pub struct SearchChunk {
    pub hits: Vec<SearchHit>,
    pub before: Option<MessageId>,
    pub more: bool,
}

impl Storage {
    pub fn search_chunk(
        &self,
        account: &str,
        query: &SearchQuery,
        before: Option<MessageId>,
        max_hits: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<SearchChunk, LocalReadError> {
        query.validate()?;
        if max_hits == 0 || max_hits > 20 {
            return Err(LocalReadError::InvalidQuery);
        }
        let key = self.key(account).map_err(|_| LocalReadError::Unavailable)?;
        let matcher = query.matcher();
        let mut hits = Vec::with_capacity(max_hits);
        let mut scanned = before;
        // What earlier searches found is searched too.
        let more = visit(
            &self.connection,
            key,
            account,
            true,
            query.channel,
            before,
            false,
            SCAN_ROWS,
            cancelled,
            &mut |message, _| {
                scanned = Some(message.id);
                if matcher(&message) {
                    hits.push(SearchHit::from_message(&message));
                }
                hits.len() < max_hits
            },
        )?;
        Ok(SearchChunk {
            hits,
            before: scanned,
            more,
        })
    }

    pub fn cached_context(
        &self,
        account: &str,
        channel: ChannelId,
        anchor: MessageId,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<CachedContext, LocalReadError> {
        if channel.0 == 0 || anchor.0 == 0 {
            return Err(LocalReadError::InvalidQuery);
        }
        if cancelled() {
            return Err(LocalReadError::Cancelled);
        }
        let key = self.key(account).map_err(|_| LocalReadError::Unavailable)?;
        let mut statement = self.connection.prepare_cached(
            "SELECT id, body FROM messages m WHERE account = ?1 AND channel = ?2 AND id = ?3
             AND NOT EXISTS (SELECT 1 FROM tombstones t WHERE t.account=m.account AND t.channel=m.channel AND t.id=m.id)"
        ).map_err(|_| LocalReadError::Failed)?;
        let mut rows = statement
            .query(params![account, channel.0 as i64, anchor.0 as i64])
            .map_err(|_| LocalReadError::Failed)?;
        let row = rows
            .next()
            .map_err(|_| LocalReadError::Failed)?
            .ok_or(LocalReadError::NotFoundLocal)?;
        let (message, mut raw_bytes) = decode(row, key, account, channel)?;
        drop(rows);
        drop(statement);
        let mut messages = Vec::with_capacity(51);
        messages.push(message);
        let mut clipped_older = false;
        let mut clipped_newer = false;
        let mut older = 0;
        // Only the history's neighbors: what a search found is no run of it.
        let has_older_cached = visit(
            &self.connection,
            key,
            account,
            false,
            channel,
            Some(anchor),
            false,
            25,
            cancelled,
            &mut |message, bytes| {
                if raw_bytes.saturating_add(bytes) > CONTEXT_RAW_BYTES {
                    clipped_older = true;
                    return false;
                }
                raw_bytes += bytes;
                messages.push(message);
                older += 1;
                older < 25
            },
        )? || clipped_older;
        let mut newer = 0;
        let has_newer_cached = visit(
            &self.connection,
            key,
            account,
            false,
            channel,
            Some(anchor),
            true,
            25,
            cancelled,
            &mut |message, bytes| {
                if raw_bytes.saturating_add(bytes) > CONTEXT_RAW_BYTES {
                    clipped_newer = true;
                    return false;
                }
                raw_bytes += bytes;
                messages.push(message);
                newer += 1;
                newer < 25
            },
        )? || clipped_newer;
        messages.sort_unstable_by_key(|message| message.id);
        Ok(CachedContext {
            anchor,
            messages,
            has_older_cached,
            has_newer_cached,
        })
    }
}

fn decode(
    row: &rusqlite::Row<'_>,
    key: &Key,
    account: &str,
    channel: ChannelId,
) -> Result<(Message, usize), LocalReadError> {
    let id: i64 = row.get(0).map_err(|_| LocalReadError::Failed)?;
    let value = row.get_ref(1).map_err(|_| LocalReadError::Failed)?;
    let bytes = value.as_bytes().map_err(|_| LocalReadError::Failed)?.len();
    if bytes > MAX_ROW_BYTES {
        return Err(LocalReadError::TooLarge);
    }
    let body = key
        .open(&Place::message(account, channel.0 as i64, id), value)
        .ok_or(LocalReadError::Failed)?;
    let message: Message = serde_json::from_str(&body).map_err(|_| LocalReadError::Failed)?;
    if message.id != MessageId(id as u64)
        || message.channel != channel
        || message.delivery.is_local()
    {
        return Err(LocalReadError::Failed);
    }
    Ok((message, bytes))
}

/// Two indexed signed ranges preserve the model's unsigned id ordering.
fn visit(
    connection: &Connection,
    key: &Key,
    account: &str,
    found: bool,
    channel: ChannelId,
    bound: Option<MessageId>,
    ascending: bool,
    limit: usize,
    cancelled: &dyn Fn() -> bool,
    visitor: &mut dyn FnMut(Message, usize) -> bool,
) -> Result<bool, LocalReadError> {
    if cancelled() {
        return Err(LocalReadError::Cancelled);
    }
    let high = bound.is_some_and(|bound| bound.0 > i64::MAX as u64);
    let ranges: Vec<(bool, Option<i64>)> = match (ascending, bound) {
        (false, None) => vec![(true, None), (false, None)],
        (false, Some(bound)) if high => vec![(true, Some(bound.0 as i64)), (false, None)],
        (false, Some(bound)) => vec![(false, Some(bound.0 as i64))],
        (true, None) => vec![(false, None), (true, None)],
        (true, Some(bound)) if !high => vec![(false, Some(bound.0 as i64)), (true, None)],
        (true, Some(bound)) => vec![(true, Some(bound.0 as i64))],
    };
    let mut scanned = 0;
    let mut stopped = false;
    for (negative, bound) in ranges {
        let sign = if negative { "<" } else { ">=" };
        let compare = if ascending { ">" } else { "<" };
        let order = if ascending { "ASC" } else { "DESC" };
        let checkpoint = if bound.is_some() {
            format!("AND id {compare} ?3")
        } else {
            String::new()
        };
        let rows = if found {
            "(SELECT account, channel, id, body FROM messages
              UNION ALL SELECT account, channel, id, body FROM found)"
        } else {
            "messages"
        };
        // The interpolated fragments are fixed SQL, never user input.
        let sql = format!(
            "SELECT id, body FROM {rows} m WHERE account = ?1 AND channel = ?2 AND id {sign} 0
             {checkpoint}
             AND NOT EXISTS (SELECT 1 FROM tombstones t WHERE t.account=m.account AND t.channel=m.channel AND t.id=m.id)
             ORDER BY id {order} LIMIT ?4"
        );
        let mut statement = connection
            .prepare_cached(&sql)
            .map_err(|_| LocalReadError::Failed)?;
        let mut rows = statement
            .query(params![
                account,
                channel.0 as i64,
                bound,
                (limit.saturating_sub(scanned) + 1) as i64
            ])
            .map_err(|_| LocalReadError::Failed)?;
        while let Some(row) = rows.next().map_err(|_| LocalReadError::Failed)? {
            if cancelled() {
                return Err(LocalReadError::Cancelled);
            }
            if stopped || scanned == limit {
                return Ok(true);
            }
            let (message, bytes) = decode(row, key, account, channel)?;
            scanned += 1;
            stopped = !visitor(message, bytes);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MessageMutation;
    use rune_model::{Attachment, AttachmentId, Delivery, Message, MessageKind, Timestamp, UserId};

    fn message(id: u64) -> Message {
        Message {
            id: MessageId(id),
            channel: ChannelId(5),
            author: UserId(2),
            content: format!("ÚNICO literal from:me %_ {id}"),
            timestamp: Timestamp(id as i64),
            edited: None,
            kind: MessageKind::Default,
            reply_to: None,
            mentions: vec![],
            reactions: vec![],
            attachments: vec![],
            stickers: vec![],
            text_references: vec![],
            delivery: Delivery::Sent,
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

    fn query() -> SearchQuery {
        SearchQuery {
            channel: ChannelId(5),
            text: " único ".into(),
            ..Default::default()
        }
    }

    fn all(storage: &Storage, query: &SearchQuery) -> Vec<u64> {
        let mut before = None;
        let mut hits = vec![];
        loop {
            let chunk = storage
                .search_chunk("a", query, before, 20, &|| false)
                .unwrap();
            hits.extend(chunk.hits.iter().map(|hit| hit.id.0));
            if !chunk.more {
                return hits;
            }
            assert_ne!(
                chunk.before, before,
                "even a block without matches advances"
            );
            before = chunk.before;
        }
    }

    #[test]
    fn local_search_keyset_is_unsigned_bounded_and_scoped() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        let ids: Vec<_> = (1..=75)
            .chain([i64::MAX as u64, 1 << 63, u64::MAX])
            .collect();
        let messages: Vec<_> = ids.iter().copied().map(message).collect();
        storage.save_messages("a", &messages).unwrap();
        storage.save_messages("other", &[message(999)]).unwrap();
        storage
            .save_messages(
                "a",
                &[Message {
                    channel: ChannelId(6),
                    ..message(998)
                }],
            )
            .unwrap();
        let chunk = storage
            .search_chunk("a", &query(), None, 20, &|| false)
            .unwrap();
        assert_eq!(chunk.hits.len(), 20);
        assert!(chunk.more);
        assert_eq!(chunk.hits[0].id, MessageId(u64::MAX));
        assert_eq!(
            all(&storage, &query()),
            ids.into_iter().rev().collect::<Vec<_>>()
        );
        assert!(storage
            .search_chunk("missing", &query(), None, 20, &|| false)
            .unwrap()
            .hits
            .is_empty());
    }

    #[test]
    fn local_search_empty_blocks_advance_and_match_literal_unicode_and_all_filters() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        let mut messages: Vec<_> = (1..=100).map(message).collect();
        for message in &mut messages[10..] {
            message.content = "unrelated".into();
        }
        let selected = &mut messages[5];
        selected.pinned = true;
        selected.attachments.push(Attachment {
            id: AttachmentId(1),
            filename: "a.txt".into(),
            size_bytes: 1,
            content_type: None,
            dimensions: None,
            preview: None,
            download: None,
            spoiler: false,
            voice: None,
            explicit: false,
            placeholder: None,
        });
        storage.save_messages("a", &messages).unwrap();
        let first = storage
            .search_chunk("a", &query(), None, 20, &|| false)
            .unwrap();
        assert!(first.hits.is_empty());
        assert_eq!(first.before, Some(MessageId(69)));
        assert!(first.more);
        let filtered = SearchQuery {
            text: "FROM:ME %_".into(),
            author: Some(UserId(2)),
            files: true,
            pinned: true,
            from: Some(Timestamp(6)),
            until: Some(Timestamp(7)),
            ..query()
        };
        assert_eq!(all(&storage, &filtered), vec![6]);
        assert!(all(
            &storage,
            &SearchQuery {
                from: None,
                until: Some(Timestamp(6)),
                ..filtered
            }
        )
        .is_empty());
    }

    #[test]
    fn local_search_and_context_respect_tombstones_unknown_scope_and_cancellation() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        storage
            .save_messages("a", &[message(1), message(2)])
            .unwrap();
        storage
            .apply_messages(
                "a",
                &[MessageMutation::Delete {
                    channel: ChannelId(5),
                    message: MessageId(2),
                }],
            )
            .unwrap();
        assert_eq!(all(&storage, &query()), vec![1]);
        assert_eq!(
            storage.cached_context("a", ChannelId(5), MessageId(2), &|| false),
            Err(LocalReadError::NotFoundLocal)
        );
        assert_eq!(
            storage.cached_context("other", ChannelId(5), MessageId(1), &|| false),
            Err(LocalReadError::NotFoundLocal)
        );
        assert_eq!(
            storage.cached_context("a", ChannelId(6), MessageId(1), &|| false),
            Err(LocalReadError::NotFoundLocal)
        );
        assert!(matches!(
            storage.search_chunk("a", &query(), None, 20, &|| true),
            Err(LocalReadError::Cancelled)
        ));
        assert_eq!(
            storage.cached_context("a", ChannelId(5), MessageId(1), &|| true),
            Err(LocalReadError::Cancelled)
        );
    }

    #[test]
    fn cached_context_requires_anchor_and_only_reports_local_neighbors() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        let messages: Vec<_> = (1..=90).map(|id| message(id * 2)).collect();
        storage.save_messages("a", &messages).unwrap();
        let context = storage
            .cached_context("a", ChannelId(5), MessageId(90), &|| false)
            .unwrap();
        assert_eq!(context.anchor, MessageId(90));
        assert_eq!(context.messages.len(), 51);
        assert_eq!(context.messages.first().unwrap().id, MessageId(40));
        assert_eq!(context.messages.last().unwrap().id, MessageId(140));
        assert!(context.has_older_cached && context.has_newer_cached);
        let first = storage
            .cached_context("a", ChannelId(5), MessageId(2), &|| false)
            .unwrap();
        assert!(!first.has_older_cached);
        assert!(first.has_newer_cached);
        assert_eq!(
            storage.cached_context("a", ChannelId(5), MessageId(91), &|| false),
            Err(LocalReadError::NotFoundLocal)
        );
    }

    #[test]
    fn cached_context_neighbors_cross_the_signed_sqlite_boundary() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        let messages: Vec<_> = [1, i64::MAX as u64, 1 << 63, u64::MAX]
            .into_iter()
            .map(message)
            .collect();
        storage.save_messages("a", &messages).unwrap();
        let context = storage
            .cached_context("a", ChannelId(5), MessageId(1 << 63), &|| false)
            .unwrap();
        assert_eq!(context.messages, messages);
        assert!(!context.has_older_cached && !context.has_newer_cached);
    }

    #[test]
    fn local_read_corrupt_or_oversized_rows_are_errors_not_false_empty_results() {
        let mut storage = Storage::in_memory().unwrap().unlocked();
        storage.save_messages("a", &[message(1)]).unwrap();
        storage
            .connection
            .execute("UPDATE messages SET body = 'broken'", [])
            .unwrap();
        assert!(matches!(
            storage.search_chunk("a", &query(), None, 20, &|| false),
            Err(LocalReadError::Failed)
        ));
        assert_eq!(
            storage.cached_context("a", ChannelId(5), MessageId(1), &|| false),
            Err(LocalReadError::Failed)
        );
        let large = Message {
            content: "x".repeat(2 * 1024 * 1024),
            ..message(2)
        };
        storage
            .connection
            .execute("DELETE FROM messages", [])
            .unwrap();
        storage.save_messages("a", &[large]).unwrap();
        assert_eq!(
            storage.cached_context("a", ChannelId(5), MessageId(2), &|| false),
            Err(LocalReadError::TooLarge)
        );
    }
}
