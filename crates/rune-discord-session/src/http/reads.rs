//! Acknowledgements and searches by the routes Serein uses (its
//! `discord-api/src/lib.rs` `mark_read`, `mark_guild_read` and `search`, and
//! `discord-protocol/src/search.rs`, read 2026-10-17). An acknowledgement is a
//! write: it goes through [`Http::request_answer`], so a challenge the service
//! sets on it is offered like any other's. Fixtures only.

use rune_i18n::{es, t};
use std::collections::HashSet;
use std::sync::Mutex;

use reqwest::Method;
use rune_model::{
    ChannelId, Message, MessageId, RemoteOrder, RemotePage, RemoteSearch, RemoteSearchError,
    Timestamp, REMOTE_PAGE,
};
use rune_provider::{Command, Event, RequestId};
use serde_json::{json, Value};
use zeroize::Zeroizing;

use super::{Http, RestError};
use crate::{reads, wire, SessionSecret};

/// The most a page of results reads, as Serein bounds it.
const PAGE: usize = 512 * 1024;
/// Messages around each hit the service sends with it, at most.
const GROUP: usize = 5;
/// The service's epoch for ids, in milliseconds.
const ID_EPOCH_MS: i64 = 1_420_070_400_000;
/// How long the service's index is waited for, at most, before asking again.
const MAX_INDEXING_MS: u64 = 60_000;

impl Http {
    /// The REST call of a read command; its answer is completed by
    /// [`reads::Reads::confirmed`] in the provider loop.
    pub(crate) async fn reads(
        &self,
        secret: &SessionSecret,
        command: &Command,
        token: &Mutex<Option<Zeroizing<String>>>,
    ) -> Result<Vec<Event>, RestError> {
        match command {
            Command::MarkRead {
                channel, message, ..
            } => {
                self.ack(secret, *channel, *message, None, token).await?;
                Ok(Vec::new())
            }
            Command::MarkUnread {
                channel,
                message,
                mentions,
                ..
            } => {
                // The service takes a reminder as the cursor before it.
                let cursor = reads::before(*message).unwrap_or(MessageId(0));
                self.ack(secret, *channel, cursor, Some(*mentions), token)
                    .await?;
                Ok(Vec::new())
            }
            Command::MarkServerRead { server, .. } => {
                self.request_answer(
                    secret,
                    Method::POST,
                    &format!("/guilds/{server}/ack"),
                    None,
                    4096,
                    true,
                )
                .await?;
                Ok(Vec::new())
            }
            Command::SearchMessages {
                request,
                search,
                offset,
            } => self.search(secret, *request, search, *offset).await,
            _ => Err(RestError::rejected(es(
                "Esta acción todavía no está disponible.",
            ))),
        }
    }

    /// `manual` holds the channel unread, with the mentions it leaves; a
    /// plain one sends the last token, as the service's normal client does.
    async fn ack(
        &self,
        secret: &SessionSecret,
        channel: ChannelId,
        message: MessageId,
        manual: Option<u32>,
        token: &Mutex<Option<Zeroizing<String>>>,
    ) -> Result<(), RestError> {
        let body = match manual {
            Some(mentions) => json!({"manual": true, "mention_count": mentions}),
            None => {
                let held = token
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                json!({"token": held.as_deref().map(String::as_str), "manual": false})
            }
        };
        let answer = self
            .request_answer(
                secret,
                Method::POST,
                &format!("/channels/{channel}/messages/{message}/ack"),
                Some(body),
                4096,
                true,
            )
            .await?;
        if manual.is_none() {
            let next = answer["token"]
                .as_str()
                .filter(|next| next.len() <= 2048 && !next.chars().any(char::is_control))
                .map(|next| Zeroizing::new(next.to_owned()));
            *token
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = next;
        }
        Ok(())
    }

    async fn search(
        &self,
        secret: &SessionSecret,
        request: RequestId,
        search: &RemoteSearch,
        offset: u32,
    ) -> Result<Vec<Event>, RestError> {
        let path = search_path(search, offset);
        let reply = self.request(secret, Method::GET, &path, None, PAGE).await?;
        let result = page(&reply, search, offset);
        let mut events = Vec::new();
        let result = match result {
            Ok((page, authors)) => {
                events.extend(authors.into_iter().map(Event::UserChanged));
                Ok(page)
            }
            Err(error @ RemoteSearchError::Indexing { .. }) => Err(error),
            Err(_) => {
                return Err(RestError::rejected(es(
                    "Discord envió resultados incompatibles.",
                )))
            }
        };
        events.push(Event::SearchResults { request, result });
        Ok(events)
    }
}

/// Percent-encode every byte: the person's text can add no parameter.
fn encode(value: &str) -> String {
    value.bytes().map(|byte| format!("%{byte:02X}")).collect()
}

/// The service's id for a time: what was written then starts there.
fn id_at(time: Timestamp) -> Option<u64> {
    let since = u64::try_from(time.0.checked_sub(ID_EPOCH_MS)?).ok()?;
    (since > 0 && since <= u64::MAX >> 22).then_some(since << 22)
}

/// The route and filters of `search` from `offset`, newest first or, as
/// the web client asks for «most relevant», by relevance.
pub(crate) fn search_path(search: &RemoteSearch, offset: u32) -> String {
    let mut path = match search.server {
        Some(server) => format!("/guilds/{server}/messages/search?"),
        None => format!("/channels/{}/messages/search?", search.channel),
    };
    let sort = match search.order {
        RemoteOrder::Newest => "timestamp",
        RemoteOrder::Relevance => "relevance",
    };
    path.push_str(&format!(
        "limit={REMOTE_PAGE}&sort_by={sort}&sort_order=desc"
    ));
    let text = search.text.trim();
    if !text.is_empty() {
        path.push_str(&format!("&content={}", encode(text)));
    }
    for author in &search.authors {
        path.push_str(&format!("&author_id={author}"));
    }
    for user in &search.mentions {
        path.push_str(&format!("&mentions={user}"));
    }
    for has in &search.has {
        path.push_str(&format!("&has={}", has.key()));
    }
    for channel in &search.channels {
        path.push_str(&format!("&channel_id={channel}"));
    }
    for kind in &search.author_kinds {
        path.push_str(&format!("&author_type={}", kind.key()));
    }
    if let Some(pinned) = search.pinned {
        path.push_str(&format!("&pinned={pinned}"));
    }
    // The web client's `linkFrom:`; checked as a host name, escaped anyway.
    for host in &search.link_hosts {
        path.push_str(&format!("&link_hostname={}", encode(host)));
    }
    // `min_id` leaves out the id it names: one less keeps that millisecond.
    if let Some(after) = search.after.and_then(id_at) {
        path.push_str(&format!("&min_id={}", after - 1));
    }
    if let Some(before) = search.before.and_then(id_at) {
        path.push_str(&format!("&max_id={before}"));
    }
    if offset != 0 {
        path.push_str(&format!("&offset={offset}"));
    }
    path
}

/// A page of the service's answer, or that its index is not ready: a 202,
/// or a code of the 110000s, with the time to wait.
fn page(
    reply: &Value,
    search: &RemoteSearch,
    offset: u32,
) -> Result<(RemotePage, Vec<rune_model::User>), RemoteSearchError> {
    let indexing = reply["code"]
        .as_u64()
        .is_some_and(|code| (110_000..120_000).contains(&code))
        || reply.get("messages").is_none() && reply.get("total_results").is_none();
    if indexing {
        let wait = reply["retry_after"]
            .as_f64()
            .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
            .map_or(2_000, |seconds| (seconds * 1000.0) as u64);
        return Err(RemoteSearchError::Indexing {
            retry_after_ms: wait.clamp(1_000, MAX_INDEXING_MS),
        });
    }
    let bad = || RemoteSearchError::Failed(t("Discord envió resultados incompatibles.").into());
    let total = reply["total_results"].as_u64().ok_or_else(bad)?;
    let groups = reply["messages"]
        .as_array()
        .filter(|groups| groups.len() <= REMOTE_PAGE)
        .ok_or_else(bad)?;
    let mut messages: Vec<Message> = Vec::new();
    let mut authors = Vec::new();
    let mut seen = HashSet::new();
    for group in groups {
        let group = group
            .as_array()
            .filter(|group| !group.is_empty() && group.len() <= GROUP)
            .ok_or_else(bad)?;
        // The hit among the messages around it; a group of one is the hit.
        let mut hits = group
            .iter()
            .filter(|item| item["hit"] == true || group.len() == 1 && item["hit"].is_null());
        let hit = hits.next().ok_or_else(bad)?;
        if hits.next().is_some() {
            return Err(bad());
        }
        let (message, users) = wire::message(hit).map_err(|_| bad())?;
        if !seen.insert(message.id) {
            return Err(bad());
        }
        authors.extend(users);
        messages.push(message);
    }
    // Ranked by relevance, the service's order is the answer.
    if search.order == RemoteOrder::Newest {
        messages.sort_by_key(|message| std::cmp::Reverse((message.timestamp, message.id)));
    }
    let page = RemotePage {
        messages,
        total,
        offset,
        partial: reply["doing_deep_historical_index"] == true,
    };
    if !page.fits(search, offset) {
        return Err(bad());
    }
    Ok((page, authors))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{AuthorKind, Has, ServerId, UserId};

    fn hit(id: u64, channel: u64, at: &str) -> Value {
        json!({"id": id.to_string(), "channel_id": channel.to_string(),
            "author": {"id": "43", "username": "peer"}, "content": format!("hola {id}"),
            "timestamp": at, "attachments": [], "mentions": []})
    }

    #[test]
    fn filters_become_parameters_and_text_cannot_add_one() {
        let search = RemoteSearch {
            channel: ChannelId(30),
            server: Some(ServerId(10)),
            text: "hola&author_id=1 ñ".into(),
            authors: vec![UserId(43)],
            mentions: vec![UserId(44)],
            has: vec![Has::Link, Has::Sticker],
            channels: vec![ChannelId(31)],
            author_kinds: vec![AuthorKind::Bot],
            pinned: Some(true),
            after: Some(Timestamp(ID_EPOCH_MS + 1_000)),
            before: Some(Timestamp(ID_EPOCH_MS + 2_000)),
            link_hosts: vec!["docs.rs".into()],
            order: RemoteOrder::Newest,
        };
        let path = search_path(&search, 25);
        assert!(path
            .starts_with("/guilds/10/messages/search?limit=25&sort_by=timestamp&sort_order=desc"));
        assert!(path.contains("&content=%68%6F%6C%61%26%61%75"));
        assert!(!path.contains("&author_id=1&"));
        for part in [
            "&author_id=43",
            "&mentions=44",
            "&has=link",
            "&has=sticker",
            "&channel_id=31",
            "&author_type=bot",
            "&pinned=true",
            "&link_hostname=%64%6F%63%73%2E%72%73",
            "&offset=25",
        ] {
            assert!(path.contains(part), "{part} in {path}");
        }
        assert!(path.contains(&format!("&min_id={}", (1_000u64 << 22) - 1)));
        assert!(path.contains(&format!("&max_id={}", 2_000u64 << 22)));
        let direct = RemoteSearch {
            channel: ChannelId(20),
            pinned: Some(false),
            ..Default::default()
        };
        assert_eq!(
            search_path(&direct, 0),
            "/channels/20/messages/search?limit=25&sort_by=timestamp&sort_order=desc&pinned=false"
        );
        let ranked = RemoteSearch {
            order: RemoteOrder::Relevance,
            has: vec![Has::Poll],
            ..direct
        };
        assert_eq!(
            search_path(&ranked, 0),
            "/channels/20/messages/search?limit=25&sort_by=relevance&sort_order=desc&has=poll&pinned=false"
        );
    }

    #[test]
    fn a_page_keeps_each_hit_and_an_index_in_progress_waits() {
        let search = RemoteSearch {
            channel: ChannelId(30),
            server: Some(ServerId(10)),
            text: "hola".into(),
            ..Default::default()
        };
        let mut around = hit(5, 30, "2026-10-17T10:00:00Z");
        around["hit"] = json!(false);
        let mut found = hit(6, 31, "2026-10-17T10:01:00Z");
        found["hit"] = json!(true);
        let (page, authors) = page(
            &json!({"total_results": 40, "messages": [[around, found], [hit(4, 30, "2026-10-17T09:00:00Z")]],
                "doing_deep_historical_index": true}),
            &search,
            0,
        )
        .unwrap();
        assert_eq!(
            page.messages.iter().map(|m| m.id.0).collect::<Vec<_>>(),
            vec![6, 4]
        );
        assert_eq!(page.total, 40);
        assert!(page.partial);
        assert_eq!(authors.len(), 2);
        assert_eq!(
            super::page(&json!({"code": 110000, "retry_after": 3.5}), &search, 0).unwrap_err(),
            RemoteSearchError::Indexing {
                retry_after_ms: 3_500
            }
        );
        assert_eq!(
            super::page(&json!({"message": "Index not yet available"}), &search, 0).unwrap_err(),
            RemoteSearchError::Indexing {
                retry_after_ms: 2_000
            }
        );
        // Two hits in a group, a repeated hit, or a channel outside the scope.
        for bad in [
            json!({"total_results": 1, "messages": [[{"hit": true, "id": "1"}, {"hit": true, "id": "2"}]]}),
            json!({"total_results": 2, "messages": [[hit(4, 30, "2026-10-17T09:00:00Z")], [hit(4, 30, "2026-10-17T09:00:00Z")]]}),
        ] {
            assert!(matches!(
                super::page(&bad, &search, 0),
                Err(RemoteSearchError::Failed(_))
            ));
        }
        let ranked = RemoteSearch {
            order: RemoteOrder::Relevance,
            ..search.clone()
        };
        let (page, _) = super::page(
            &json!({"total_results": 2, "messages": [[hit(4, 30, "2026-10-17T09:00:00Z")],
                [hit(6, 30, "2026-10-17T10:00:00Z")]]}),
            &ranked,
            0,
        )
        .unwrap();
        assert_eq!(
            page.messages.iter().map(|m| m.id.0).collect::<Vec<_>>(),
            vec![4, 6],
            "ranked by relevance: the service's order"
        );
        let single = RemoteSearch {
            server: None,
            ..search
        };
        assert!(super::page(
            &json!({"total_results": 1, "messages": [[hit(4, 31, "2026-10-17T09:00:00Z")]]}),
            &single,
            0
        )
        .is_err());
    }
}
