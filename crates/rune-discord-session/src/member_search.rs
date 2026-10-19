//! Members looked up by name over the Gateway (op 8, `GUILD_MEMBERS_CHUNK`),
//! as Serein does for the composer (its `discord-gateway/src/member_search.rs`
//! and `client-core/src/member_search.rs`, read 2026-10-17): one query at a
//! time, the newest; a nonce matches its answer; at most 100 members; never
//! a whole server's directory. Typing waits a moment before a query goes,
//! queries are paced well below the Gateway's budget, and answers are kept
//! for a while, so retyping or narrowing asks nothing new. Fixtures only.

use std::collections::VecDeque;
use std::time::Duration;

use rune_model::{Member, ServerId, User};
use rune_provider::{Event, Snapshot};
use serde::Serialize;
use serde_json::Value;
use tokio::{sync::watch, time::Instant};

use crate::{bootstrap, wire};

/// The most members one query returns, as the service caps it.
pub(crate) const LIMIT: usize = 100;
/// How long typing must pause before a query goes.
pub(crate) const SETTLE: Duration = Duration::from_millis(350);
/// The least time between two queries, as Serein keeps it.
pub(crate) const GAP: Duration = Duration::from_secs(1);
/// Queries in a minute, at most: with heartbeats and member list frames,
/// far below the 120 Gateway messages a minute allows.
const PER_MINUTE: usize = 15;
const MINUTE: Duration = Duration::from_secs(60);
/// How long an answer is waited for before the query is let go.
const PATIENCE: Duration = Duration::from_secs(15);
/// Answers kept, and for how long.
const KEPT: usize = 32;
const FRESH: Duration = Duration::from_secs(300);
/// The longest name looked up, in characters.
const MAX_QUERY: usize = 32;

/// One query for the Gateway to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lookup {
    pub server: ServerId,
    pub query: String,
    pub nonce: u64,
}

/// The op 8 message of a lookup.
#[derive(Serialize)]
pub(crate) struct Request {
    guild_id: String,
    query: String,
    limit: usize,
    nonce: String,
}

/// When the Gateway sends the newest wish: after typing settles, a gap
/// after the last one, and within the minute's budget.
#[derive(Default)]
pub(crate) struct Pacer {
    told: Option<u64>,
    changed: Option<Instant>,
    sent: VecDeque<Instant>,
}

impl Pacer {
    /// The wish changed now.
    pub(crate) fn changed(&mut self, now: Instant) {
        self.changed = Some(now);
    }

    /// Whether `want` is still to be sent.
    pub(crate) fn wants(&self, want: Option<&Lookup>) -> bool {
        want.is_some_and(|want| Some(want.nonce) != self.told)
    }

    /// When the wish may go; fixtures shorten `settle` and `gap`.
    pub(crate) fn due(&mut self, now: Instant, settle: Duration, gap: Duration) -> Instant {
        while self
            .sent
            .front()
            .is_some_and(|at| now.duration_since(*at) >= MINUTE)
        {
            self.sent.pop_front();
        }
        let mut due = self.changed.map_or(now, |at| at + settle);
        if let Some(last) = self.sent.back() {
            due = due.max(*last + gap);
        }
        if self.sent.len() >= PER_MINUTE {
            due = due.max(self.sent[0] + MINUTE);
        }
        due
    }

    /// The message for `want`, sent now.
    pub(crate) fn send(&mut self, want: &Lookup, now: Instant) -> Request {
        self.told = Some(want.nonce);
        self.sent.push_back(now);
        Request {
            guild_id: want.server.to_string(),
            query: want.query.clone(),
            limit: LIMIT,
            nonce: want.nonce.to_string(),
        }
    }

    /// A new connection: the wish not answered is sent again.
    pub(crate) fn restart(&mut self) {
        self.told = None;
    }
}

struct Found {
    server: ServerId,
    query: String,
    people: Vec<(User, Member)>,
    at: Instant,
}

/// The session's side: the query asked for, and the answers kept.
pub(crate) struct Lookups {
    wanted: watch::Sender<Option<Lookup>>,
    asked: Option<(Lookup, Instant)>,
    nonce: u64,
    found: VecDeque<Found>,
}

impl Lookups {
    pub(crate) fn new(nonce_prefix: u32) -> (Self, watch::Receiver<Option<Lookup>>) {
        let (wanted, receiver) = watch::channel(None);
        (
            Self {
                wanted,
                asked: None,
                // Distinct from another session's, which may still answer late.
                nonce: u64::from(nonce_prefix) << 32,
                found: VecDeque::new(),
            },
            receiver,
        )
    }

    /// Look up `query` in `server`: at once from an answer kept, when one
    /// covers it, else over the Gateway, replacing a query not sent yet.
    pub(crate) fn ask(&mut self, snapshot: &Snapshot, server: ServerId, query: &str) -> Vec<Event> {
        let query = query.trim().to_lowercase();
        if query.is_empty()
            || query.chars().count() > MAX_QUERY
            || query.chars().any(char::is_control)
            || !snapshot.servers.iter().any(|known| known.id == server)
        {
            return Vec::new();
        }
        let now = Instant::now();
        self.found
            .retain(|found| now.duration_since(found.at) < FRESH);
        // The same query, or a narrower one of a query answered whole.
        let covering = self.found.iter().rev().find(|found| {
            found.server == server
                && (found.query == query
                    || query.starts_with(&found.query) && found.people.len() < LIMIT)
        });
        if let Some(found) = covering {
            let people: Vec<_> = found
                .people
                .iter()
                .filter(|(user, member)| named(user, member, &query))
                .cloned()
                .collect();
            return events(server, query, people);
        }
        // The same query still waiting asks nothing new; one unanswered for
        // too long may go again.
        if self.asked.as_ref().is_some_and(|(asked, at)| {
            asked.server == server && asked.query == query && now.duration_since(*at) < PATIENCE
        }) {
            return Vec::new();
        }
        self.nonce = self.nonce.wrapping_add(1);
        let lookup = Lookup {
            server,
            query,
            nonce: self.nonce,
        };
        self.asked = Some((lookup.clone(), now));
        self.wanted.send_replace(Some(lookup));
        Vec::new()
    }

    /// `GUILD_MEMBERS_CHUNK`: the answer to the query asked, once. Any
    /// other chunk, a late or malformed one, says nothing.
    pub(crate) fn chunk(&mut self, data: &Value) -> Vec<Event> {
        let now = Instant::now();
        if self
            .asked
            .as_ref()
            .is_some_and(|(_, at)| now.duration_since(*at) >= PATIENCE)
        {
            self.asked = None;
        }
        let Some((asked, _)) = self.asked.as_ref() else {
            return Vec::new();
        };
        if data["nonce"].as_str() != Some(asked.nonce.to_string().as_str())
            || wire::id(&data["guild_id"]).ok() != Some(asked.server.0)
        {
            return Vec::new();
        }
        let (asked, _) = self.asked.take().expect("checked above");
        let people = (|| {
            if data["chunk_index"].as_u64() != Some(0) || data["chunk_count"].as_u64() != Some(1) {
                return None;
            }
            let rows = data["members"]
                .as_array()
                .filter(|rows| rows.len() <= LIMIT)?;
            rows.iter()
                .map(|row| {
                    let user = wire::user(&row["user"]).ok()?;
                    let member = bootstrap::list_member(row, asked.server).ok()?;
                    (member.user == user.id).then_some((user, member))
                })
                .collect::<Option<Vec<_>>>()
        })();
        let Some(people) = people else {
            return Vec::new();
        };
        if self.found.len() >= KEPT {
            self.found.pop_front();
        }
        self.found.push_back(Found {
            server: asked.server,
            query: asked.query.clone(),
            people: people.clone(),
            at: now,
        });
        events(asked.server, asked.query, people)
    }
}

/// Whether `user` goes by a name starting with `query`, as the service matches.
fn named(user: &User, member: &Member, query: &str) -> bool {
    [
        Some(user.name.as_str()),
        user.display_name.as_deref(),
        member.nick.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|name| name.to_lowercase().starts_with(query))
}

fn events(server: ServerId, query: String, people: Vec<(User, Member)>) -> Vec<Event> {
    let (users, members): (Vec<_>, Vec<_>) = people.into_iter().unzip();
    let mut events: Vec<Event> = users.into_iter().map(Event::UserChanged).collect();
    events.push(Event::MembersFound {
        server,
        query,
        members,
    });
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reads::tests::snapshot;
    use serde_json::json;

    fn chunk(nonce: &str, members: Value) -> Value {
        json!({"guild_id":"10","nonce":nonce,"chunk_index":0,"chunk_count":1,"members":members})
    }

    #[tokio::test]
    async fn a_query_waits_for_its_nonce_and_its_answer_is_kept() {
        let (mut lookups, wanted) = Lookups::new(0);
        assert!(lookups.ask(&snapshot(), ServerId(10), "Pa").is_empty());
        let asked = wanted.borrow().clone().unwrap();
        assert_eq!((asked.query.as_str(), asked.nonce), ("pa", 1));
        // The same query again asks nothing new; another server or a
        // control character asks nothing at all.
        assert!(lookups.ask(&snapshot(), ServerId(10), "pa").is_empty());
        assert_eq!(wanted.borrow().as_ref().unwrap().nonce, 1);
        assert!(lookups.ask(&snapshot(), ServerId(11), "pa").is_empty());
        assert!(lookups.ask(&snapshot(), ServerId(10), "p\na").is_empty());
        let people = json!([{"user":{"id":"7","username":"pablo"},"roles":[]},
            {"user":{"id":"8","username":"paula"},"nick":"Pau","roles":[]}]);
        assert!(
            lookups.chunk(&chunk("2", people.clone())).is_empty(),
            "wrong nonce"
        );
        let found = lookups.chunk(&chunk("1", people.clone()));
        assert!(
            matches!(&found[..], [Event::UserChanged(_), Event::UserChanged(_),
            Event::MembersFound { query, members, .. }] if query == "pa" && members.len() == 2)
        );
        assert!(
            lookups.chunk(&chunk("1", people)).is_empty(),
            "answered once"
        );
        // A narrower query of a whole answer comes from it, at once.
        let narrower = lookups.ask(&snapshot(), ServerId(10), "pab");
        assert!(
            matches!(&narrower[..], [Event::UserChanged(user), Event::MembersFound { members, .. }]
            if user.name == "pablo" && members.len() == 1)
        );
        assert_eq!(wanted.borrow().as_ref().unwrap().nonce, 1);
    }

    #[tokio::test]
    async fn malformed_or_oversized_chunks_say_nothing() {
        let (mut lookups, _wanted) = Lookups::new(0);
        lookups.ask(&snapshot(), ServerId(10), "x");
        let many: Vec<Value> = (0..=LIMIT)
            .map(|id| json!({"user":{"id":(id + 1).to_string(),"username":"x"},"roles":[]}))
            .collect();
        assert!(lookups.chunk(&chunk("1", json!(many))).is_empty());
        lookups.ask(&snapshot(), ServerId(10), "y");
        let mut split = chunk("2", json!([]));
        split["chunk_count"] = json!(2);
        assert!(lookups.chunk(&split).is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn the_pacer_settles_spaces_and_bounds_queries() {
        let mut pacer = Pacer::default();
        let start = Instant::now();
        let want = |nonce| Lookup {
            server: ServerId(10),
            query: "a".into(),
            nonce,
        };
        assert!(pacer.wants(Some(&want(1))));
        pacer.changed(start);
        assert_eq!(pacer.due(start, SETTLE, GAP), start + SETTLE);
        let sent = start + SETTLE;
        let frame = pacer.send(&want(1), sent);
        assert_eq!(frame.limit, LIMIT);
        assert!(!pacer.wants(Some(&want(1))));
        pacer.changed(sent);
        assert_eq!(pacer.due(sent, SETTLE, GAP), sent + GAP);
        // The minute's budget holds the sixteenth.
        let mut now = sent;
        for nonce in 2..=PER_MINUTE as u64 {
            now += GAP;
            pacer.send(&want(nonce), now);
        }
        pacer.changed(now);
        assert_eq!(pacer.due(now, SETTLE, GAP), sent + MINUTE);
        pacer.restart();
        assert!(pacer.wants(Some(&want(PER_MINUTE as u64))));
    }
}
