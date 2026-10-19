//! A server's member list the way Serein keeps it: a Gateway subscription
//! (op 37, undocumented; owner-authorized 2026-10-16) to the rows near the
//! view, and the operations of `GUILD_MEMBER_LIST_UPDATE` applied to them.
//! Unofficial and not verified against Discord; only local fixtures parse.
//! An update that does not fit is dropped: it never ends the session.

use std::collections::{HashSet, VecDeque};

use rune_model::{
    ChannelId, ChannelKind, ListGroup, ListRow, MemberList, OverwriteTarget, Permissions, Presence,
    RoleId, ServerId, Status, TextPicture, UserId, LIST_GROUPS_MAX,
};
use rune_provider::{Command, Event, Snapshot};
use serde_json::{json, Value};

use crate::{bootstrap, profiles::text, wire};

/// Rows per range asked for, as the service pages them.
const RANGE: u32 = 100;
/// Operations in one update, at most; more drops the update.
const MAX_OPS: usize = 1024;
/// Lists left lately, so a late update of one is never taken for the new one.
const RETIRED: usize = 8;
/// The highest row index taken.
const MAX_INDEX: u32 = 10_000_000;
/// People subscribed for their cards, at most; the official client warns
/// past 50.
const PEOPLE: usize = 8;

/// What the Gateway keeps subscribed: rows of one list of one server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Watch {
    pub server: ServerId,
    pub channel: ChannelId,
    /// The list the service keeps for whoever can see the channel.
    pub list: String,
    pub ranges: Vec<[u32; 2]>,
    /// People whose presence is kept current besides the rows: those whose
    /// card shows, as the official client subscribes its popouts' members
    /// (`members`, 2026-10-17).
    pub people: Vec<UserId>,
}

impl Watch {
    fn same_list(&self, other: &Self) -> bool {
        self.server == other.server && self.list == other.list
    }

    fn covers(&self, index: u32) -> bool {
        self.ranges.iter().any(|[a, b]| (*a..=*b).contains(&index))
    }
}

/// The ranges around rows `first..=last`: the page they are in, or the two
/// adjacent pages ending at `last`'s.
pub(crate) fn ranges(first: u32, last: u32) -> Vec<[u32; 2]> {
    let (first, last) = (
        first.min(last).min(MAX_INDEX),
        first.max(last).min(MAX_INDEX),
    );
    let page = |k: u32| [k * RANGE, k * RANGE + RANGE - 1];
    let (a, b) = (first / RANGE, last / RANGE);
    if a == b {
        vec![page(b)]
    } else {
        vec![page(b - 1), page(b)]
    }
}

/// The Gateway frames (op 37 payloads) that take the subscription from
/// `sent` to `want`: leave a list before opening another one.
pub(crate) fn frames(sent: Option<&Watch>, want: Option<&Watch>) -> Vec<Value> {
    if sent == want {
        return Vec::new();
    }
    let frame = |server: ServerId, typing: bool, channels: Value, people: &[UserId]| {
        let people: Vec<String> = people.iter().map(|user| user.to_string()).collect();
        json!({"subscriptions": {server.to_string(): {
            "typing": typing, "threads": false, "activities": true, "member_updates": false,
            "members": people, "channels": channels, "thread_member_lists": []
        }}})
    };
    let mut frames = Vec::new();
    if let Some(sent) = sent.filter(|sent| want.is_none_or(|want| !sent.same_list(want))) {
        frames.push(frame(sent.server, false, json!({}), &[]));
    }
    if let Some(want) = want {
        if sent.is_none_or(|sent| !sent.same_list(want)) {
            frames.push(frame(want.server, true, json!({}), &[]));
        }
        frames.push(frame(
            want.server,
            true,
            json!({ want.channel.to_string(): want.ranges }),
            &want.people,
        ));
    }
    frames
}

/// The list the service keeps for who can see `channel`: `everyone` when
/// all can, else a hash of the overwrites that decide it.
fn list_id(snapshot: &Snapshot, channel: ChannelId) -> Option<String> {
    let channel = snapshot.channels.iter().find(|known| known.id == channel)?;
    let server = channel.server?;
    let everyone = snapshot
        .servers
        .iter()
        .find(|known| known.id == server)?
        .roles
        .iter()
        .find(|role| role.id.0 == server.0)?;
    let view = Permissions::VIEW;
    if everyone.permissions.contains(view)
        && !channel.overwrites.iter().any(|o| o.deny.contains(view))
    {
        return Some("everyone".into());
    }
    let mut entries: Vec<String> = channel
        .overwrites
        .iter()
        .filter_map(|overwrite| {
            let id = match overwrite.target {
                OverwriteTarget::Role(role) => role.0,
                OverwriteTarget::Member(user) => user.0,
            };
            if overwrite.allow.contains(view) {
                Some(format!("allow:{id}"))
            } else if overwrite.deny.contains(view) {
                Some(format!("deny:{id}"))
            } else {
                None
            }
        })
        .collect();
    entries.sort();
    Some(murmur3(entries.join(",").as_bytes()).to_string())
}

/// MurmurHash3, x86 32-bit, seed 0: how the service names a list.
fn murmur3(bytes: &[u8]) -> u32 {
    let mix = |k: u32| {
        k.wrapping_mul(0xcc9e_2d51)
            .rotate_left(15)
            .wrapping_mul(0x1b87_3593)
    };
    let mut hash = 0u32;
    let blocks = bytes.chunks_exact(4);
    let tail = blocks.remainder();
    for block in blocks {
        hash ^= mix(u32::from_le_bytes([block[0], block[1], block[2], block[3]]));
        hash = hash
            .rotate_left(13)
            .wrapping_mul(5)
            .wrapping_add(0xe654_6b64);
    }
    if !tail.is_empty() {
        let k = tail
            .iter()
            .rev()
            .fold(0u32, |k, byte| (k << 8) | u32::from(*byte));
        hash ^= mix(k);
    }
    hash ^= bytes.len() as u32;
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85eb_ca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2_ae35);
    hash ^ (hash >> 16)
}

/// Someone's status and what they do, from a presence object. Offline
/// says nothing else; an unknown status is not a presence.
pub(crate) fn presence(value: &Value) -> Option<Presence> {
    let status = match value["status"].as_str()? {
        "online" => Status::Online,
        "idle" => Status::Idle,
        "dnd" => Status::DoNotDisturb,
        "offline" | "invisible" => Status::Offline,
        _ => return None,
    };
    let mut presence = Presence {
        status,
        ..Default::default()
    };
    let activities = match &value["activities"] {
        Value::Null => &[][..],
        activities => activities.as_array().filter(|a| a.len() <= 16)?,
    };
    if !status.is_around() {
        return Some(presence);
    }
    if let Some((custom, emoji)) = activities
        .iter()
        .filter(|activity| activity["type"].as_u64() == Some(4))
        .find_map(custom_status)
    {
        presence.custom = custom;
        presence.custom_emoji = emoji;
    }
    // Presence, identity and expressions (P5).
    presence.activities = crate::identity::activities(activities);
    presence.platforms = crate::identity::platforms(&value["client_status"]);
    Some(presence)
}

/// A custom status as its line reads: the Unicode emoji it carries before
/// its text, as the official client shows it (2026-10-17: the emoji was
/// dropped); a server's emoji (one with an `id`) apart, as its picture.
/// `None` when it says nothing.
fn custom_status(activity: &Value) -> Option<(Option<String>, Option<TextPicture>)> {
    let said = text(&activity["state"], 128);
    let emoji = &activity["emoji"];
    let picture = wire::id(&emoji["id"]).ok().and_then(|id| {
        let name = emoji["name"]
            .as_str()
            .filter(|name| (2..=32).contains(&name.len()))
            .filter(|name| name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))?;
        let animated = emoji["animated"].as_bool() == Some(true);
        Some(TextPicture {
            token: format!("<{}:{name}:{id}>", if animated { "a" } else { "" }),
            name: name.to_owned(),
            image: crate::emoji::inline(id, animated),
        })
    });
    let unicode = emoji["name"]
        .as_str()
        .filter(|_| emoji["id"].is_null())
        .filter(|name| {
            !name.is_empty()
                && name.chars().count() <= 16
                && !name
                    .chars()
                    .any(|c| c.is_control() || c.is_ascii_alphanumeric())
        });
    let line = match (unicode, said) {
        (Some(emoji), Some(said)) => Some(format!("{emoji} {said}")),
        (Some(emoji), None) => Some(emoji.to_owned()),
        (None, said) => said,
    };
    (line.is_some() || picture.is_some()).then_some((line, picture))
}

fn index(value: &Value) -> Option<u32> {
    value
        .as_u64()
        .filter(|index| *index <= u64::from(MAX_INDEX))
        .map(|index| index as u32)
}

fn range(value: &Value) -> Option<(u32, u32)> {
    match value.as_array()?.as_slice() {
        [a, b] => Some((index(a)?, index(b)?)).filter(|(a, b)| a <= b),
        _ => None,
    }
}

fn group(value: &Value) -> Option<ListGroup> {
    match value.as_str()? {
        "online" => Some(ListGroup::Online),
        "offline" => Some(ListGroup::Offline),
        role => wire::id(&Value::from(role))
            .ok()
            .map(|id| ListGroup::Role(RoleId(id))),
    }
}

fn groups(value: &Value) -> Option<Vec<(ListGroup, u32)>> {
    let list = value
        .as_array()
        .filter(|list| list.len() <= LIST_GROUPS_MAX)?;
    let mut seen = HashSet::new();
    list.iter()
        .map(|entry| {
            let id = group(&entry["id"]).filter(|id| seen.insert(*id))?;
            Some((id, index(&entry["count"]).unwrap_or(0)))
        })
        .collect()
}

/// The row an item makes; its person's account, membership and presence
/// go to `people`. A malformed item is a hole: later rows keep their place.
fn item(
    item: &Value,
    server: ServerId,
    me: Option<UserId>,
    people: &mut Vec<Event>,
) -> Option<ListRow> {
    if let Some(heading) = item.get("group") {
        return Some(ListRow::Heading {
            group: group(&heading["id"])?,
            count: index(&heading["count"]).unwrap_or(0),
        });
    }
    let member = item.get("member")?;
    let user = wire::user(&member["user"]).ok()?;
    let listed = bootstrap::list_member(member, server).ok()?;
    if listed.user != user.id {
        return None;
    }
    let id = user.id;
    // The presence beside the member wins over one inside it.
    let presence = item
        .get("presence")
        .or_else(|| member.get("presence"))
        .and_then(presence);
    people.push(Event::UserChanged(user));
    // The account's own membership and status stay as the session has them.
    if me != Some(id) {
        people.push(Event::MemberChanged(listed));
        if let Some(presence) = presence {
            people.push(Event::PresenceChanged { user: id, presence });
        }
    }
    Some(ListRow::Member(id))
}

/// The account and membership a `VOICE_STATE_UPDATE` of a server carries
/// in its `member`, before the call's own handling: the official client
/// stores them first, so whoever is in voice has a name and a face even in
/// a server whose list never sent them (2026-10-17). The account's own
/// stays as the session has it; a broken member says nothing.
pub(crate) fn in_voice(me: Option<UserId>, data: &Value) -> Vec<Event> {
    let found = || {
        let server = ServerId(wire::id(&data["guild_id"]).ok()?);
        let member = data.get("member")?;
        let user = wire::user(&member["user"]).ok()?;
        let listed = bootstrap::list_member(member, server).ok()?;
        let said = wire::id(&data["user_id"]).ok()?;
        (listed.user == user.id && said == user.id.0 && me != Some(user.id))
            .then(|| vec![Event::UserChanged(user), Event::MemberChanged(listed)])
    };
    found().unwrap_or_default()
}

/// The member list this session watches, as far as it was sent.
#[derive(Default)]
pub(crate) struct Lists {
    watch: Option<Watch>,
    /// The people of a server whose cards show (2026-10-17).
    people: Option<(ServerId, Vec<UserId>)>,
    /// The list updates are taken from: the computed one, or the one the
    /// first sync named instead.
    list: Option<String>,
    synced: bool,
    retired: VecDeque<String>,
    groups: Vec<(ListGroup, u32)>,
    rows: Vec<(u32, ListRow)>,
}

impl Lists {
    /// What to subscribe to after `command`, `None` for nothing. A channel
    /// that cannot be read, or is not of a server, watches nothing.
    pub(crate) fn watch(&mut self, snapshot: &Snapshot, command: &Command) -> Option<Watch> {
        let want = match *command {
            Command::WatchMemberList {
                server,
                channel,
                first,
                last,
            } => snapshot
                .channels
                .iter()
                .find(|known| {
                    known.id == channel
                        && known.server == Some(server)
                        && known.kind.has_messages()
                        && known.kind != ChannelKind::Thread
                        && known.permissions.contains(Permissions::VIEW)
                })
                .and_then(|_| list_id(snapshot, channel))
                .map(|list| Watch {
                    server,
                    channel,
                    list,
                    ranges: ranges(first, last),
                    people: self
                        .people
                        .as_ref()
                        .filter(|(at, _)| *at == server)
                        .map(|(_, people)| people.clone())
                        .unwrap_or_default(),
                }),
            _ => None,
        };
        let same = matches!((&self.watch, &want), (Some(old), Some(new)) if old.same_list(new));
        if !same {
            if let Some(old) = self.list.take() {
                if self.retired.len() >= RETIRED {
                    self.retired.pop_front();
                }
                self.retired.push_back(old);
            }
            self.restart_with(want.as_ref());
        }
        self.watch = want.clone();
        if let Some(watch) = &self.watch {
            self.rows.retain(|(at, _)| watch.covers(*at));
        }
        want
    }

    /// Keep the presence of `people` of `server` current with the list
    /// watched there, at most [`PEOPLE`]: what to subscribe to now.
    pub(crate) fn people(&mut self, server: ServerId, people: &[UserId]) -> Option<Watch> {
        let people: Vec<UserId> = people.iter().copied().take(PEOPLE).collect();
        self.people = (!people.is_empty()).then(|| (server, people.clone()));
        if let Some(watch) = self.watch.as_mut() {
            watch.people = if watch.server == server {
                people
            } else {
                Vec::new()
            };
        }
        self.watch.clone()
    }

    /// A new Gateway session subscribes again from nothing.
    pub(crate) fn restart(&mut self) {
        let watch = self.watch.clone();
        self.restart_with(watch.as_ref());
    }

    fn restart_with(&mut self, watch: Option<&Watch>) {
        self.list = watch.map(|watch| watch.list.clone());
        self.synced = false;
        self.groups.clear();
        self.rows.clear();
    }

    /// The people an update names and the list after it, or nothing when
    /// it is not of the watched list or does not fit.
    pub(crate) fn update(&mut self, me: Option<UserId>, data: &Value) -> Vec<Event> {
        self.apply(me, data).unwrap_or_default()
    }

    fn apply(&mut self, me: Option<UserId>, data: &Value) -> Option<Vec<Event>> {
        let watch = self.watch.clone()?;
        if wire::id(&data["guild_id"]).ok()? != watch.server.0 {
            return None;
        }
        let id = data["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 32)?;
        let ops = data["ops"].as_array().filter(|ops| ops.len() <= MAX_OPS)?;
        if self.list.as_deref() != Some(id) {
            // The computed name may be wrong: before the first sync, a
            // populated sync of this server, of no list left lately, is taken.
            let populated = ops.iter().any(|op| {
                op["op"] == "SYNC" && op["items"].as_array().is_some_and(|i| !i.is_empty())
            });
            if self.synced || !populated || self.retired.iter().any(|old| old == id) {
                return None;
            }
        }
        let groups = match data.get("groups") {
            None => self.groups.clone(),
            Some(value) => groups(value)?,
        };
        let mut rows = self.rows.clone();
        let mut people = Vec::new();
        let mut synced = self.synced;
        for op in ops {
            match op["op"].as_str()? {
                "SYNC" => {
                    let (a, b) = range(&op["range"])?;
                    let items = match &op["items"] {
                        Value::Null => &[][..],
                        items => items.as_array()?.as_slice(),
                    };
                    if items.len() > RANGE as usize || items.len() as u64 > u64::from(b - a) + 1 {
                        return None;
                    }
                    rows.retain(|(at, _)| !(a..=b).contains(at));
                    for (k, value) in items.iter().enumerate() {
                        if let Some(row) = item(value, watch.server, me, &mut people) {
                            rows.push((a + k as u32, row));
                        }
                    }
                    synced = true;
                }
                "INVALIDATE" => {
                    let (a, b) = range(&op["range"])?;
                    rows.retain(|(at, _)| !(a..=b).contains(at));
                }
                "INSERT" => {
                    let at = index(&op["index"])?;
                    for (known, _) in rows.iter_mut().filter(|(known, _)| *known >= at) {
                        *known = known.saturating_add(1);
                    }
                    if let Some(row) = item(&op["item"], watch.server, me, &mut people) {
                        rows.push((at, row));
                    }
                }
                "UPDATE" => {
                    let at = index(&op["index"])?;
                    rows.retain(|(known, _)| *known != at);
                    if let Some(row) = item(&op["item"], watch.server, me, &mut people) {
                        rows.push((at, row));
                    }
                }
                "DELETE" => {
                    let at = index(&op["index"])?;
                    rows.retain(|(known, _)| *known != at);
                    for (known, _) in rows.iter_mut().filter(|(known, _)| *known > at) {
                        *known -= 1;
                    }
                }
                _ => {}
            }
        }
        rows.sort_by_key(|(at, _)| *at);
        let mut list = MemberList {
            server: watch.server,
            channel: watch.channel,
            groups,
            rows: Vec::new(),
        };
        let total = list.len();
        let mut seen = HashSet::new();
        rows.retain(|(at, row)| {
            *at < total
                && watch.covers(*at)
                && !matches!(row, ListRow::Member(user) if !seen.insert(*user))
        });
        // Headings count as the groups say, the most recent count there is.
        for (_, row) in &mut rows {
            if let ListRow::Heading { group, count } = row {
                if let Some((_, known)) = list.groups.iter().find(|(id, _)| id == group) {
                    *count = *known;
                }
            }
        }
        self.list = Some(id.to_owned());
        self.synced = synced;
        self.groups = list.groups.clone();
        self.rows = rows.clone();
        list.rows = rows;
        people.push(Event::MemberListChanged(list));
        Some(people)
    }

    /// A presence change of someone in the rows sent; others are not shown.
    pub(crate) fn presence(&self, me: Option<UserId>, data: &Value) -> Option<Event> {
        let watch = self.watch.as_ref()?;
        if wire::id(&data["guild_id"]).ok()? != watch.server.0 {
            return None;
        }
        let user = UserId(wire::id(&data["user"]["id"]).ok()?);
        if me == Some(user)
            || !self
                .rows
                .iter()
                .any(|(_, row)| *row == ListRow::Member(user))
                && !watch.people.contains(&user)
        {
            return None;
        }
        Some(Event::PresenceChanged {
            user,
            presence: presence(data)?,
        })
    }
}

#[cfg(test)]
#[path = "members_tests.rs"]
mod tests;
