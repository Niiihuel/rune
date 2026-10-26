//! Relationships, group membership and friends' presence as the Gateway
//! reports them, and the checks before a write about people or conversations
//! outside servers (`http::social` sends it, by the routes Serein uses).
//! Making groups, changing their people and handing them over go by the
//! official web client's routes (2026-10-17, docs/DISCORD.md).
//!
//! Relationship rows are untrusted and optional to the account: a malformed
//! row is dropped without failing the session, and a list that cannot be
//! read whole stays unknown instead of passing for a complete one.

use rune_i18n::es;
use rune_model::{Channel, ChannelId, ChannelKind, Relationship, RelationshipKind, UserId};
use rune_provider::{Command, Event, Snapshot};
use serde_json::Value;

use crate::{bootstrap, members, wire, LoginError};

/// A group has the account and up to this many others.
const GROUP_OTHERS: usize = 9;
/// Direct conversations closed during a session, kept to list them again
/// when someone writes in one; past this the oldest is forgotten.
const CLOSED_MAX: usize = 64;
/// Presences read at once from READY, and from a supplemental list.
const PRESENCES_MAX: usize = 5_000;

/// Busy accounts hold thousands of requests and blocks; past this the list
/// is not shown rather than shown cut.
const MAX_RELATIONSHIPS: usize = 10_000;

/// Whether `command`, about people or a conversation outside servers, can
/// go out: what it names must be known to this session first.
pub(crate) fn allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    let me = snapshot.me.as_ref().map(|me| me.id);
    let kind_with = |user: UserId| {
        snapshot
            .relationships
            .as_ref()?
            .iter()
            .find(|known| known.user == user)
            .map(|known| known.kind)
    };
    let conversation = |channel: ChannelId, kind: ChannelKind| {
        snapshot
            .channels
            .iter()
            .any(|known| known.id == channel && known.kind == kind)
    };
    let refuse = |reason: &'static str| Err(LoginError::new(reason));
    match command {
        Command::SendFriendRequest { .. } => Ok(()),
        Command::AcceptFriendRequest { user }
            if kind_with(*user) != Some(RelationshipKind::IncomingRequest) =>
        {
            refuse(es("No hay una solicitud de esa persona."))
        }
        Command::RemoveRelationship { user } if kind_with(*user).is_none() => {
            refuse(es("No hay ninguna relación con esa persona."))
        }
        Command::SetFriendNickname { user, .. }
            if kind_with(*user) != Some(RelationshipKind::Friend) =>
        {
            refuse(es("Solo tus amigos pueden tener un apodo."))
        }
        Command::AcceptFriendRequest { user }
        | Command::RemoveRelationship { user }
        | Command::SetFriendNickname { user, .. }
        | Command::BlockUser { user }
        | Command::OpenDirect { user, .. } => {
            if user.0 == 0 || Some(*user) == me {
                refuse(es("Esa persona no es válida para esta acción."))
            } else {
                Ok(())
            }
        }
        Command::RenameGroup { channel, .. }
        | Command::SetGroupIcon { channel, .. }
        | Command::LeaveGroup { channel } => {
            if conversation(*channel, ChannelKind::Group) {
                Ok(())
            } else {
                refuse(es("Ese grupo ya no está en tu lista."))
            }
        }
        Command::CreateGroup { users, .. } => {
            let mut seen = std::collections::HashSet::new();
            if !(2..=GROUP_OTHERS).contains(&users.len()) {
                refuse(es("Un grupo nuevo lleva de 2 a 9 amigos."))
            } else if !users.iter().all(|user| {
                seen.insert(*user) && kind_with(*user) == Some(RelationshipKind::Friend)
            }) {
                refuse(es("Solo se crea un grupo con amigos, cada uno una vez."))
            } else {
                Ok(())
            }
        }
        Command::AddGroupMember { channel, user }
        | Command::RemoveGroupMember { channel, user }
        | Command::SetGroupOwner { channel, user } => {
            let Some(group) = snapshot
                .channels
                .iter()
                .find(|known| known.id == *channel && known.kind == ChannelKind::Group)
            else {
                return refuse(es("Ese grupo ya no está en tu lista."));
            };
            let inside = group.recipients.contains(user);
            match command {
                _ if user.0 == 0 || Some(*user) == me => {
                    refuse(es("Esa persona no es válida para esta acción."))
                }
                Command::AddGroupMember { .. } if inside => {
                    refuse(es("Esa persona ya está en el grupo."))
                }
                Command::AddGroupMember { .. }
                    if kind_with(*user) != Some(RelationshipKind::Friend) =>
                {
                    refuse(es("Solo puedes añadir a tus amigos."))
                }
                Command::AddGroupMember { .. } if group.recipients.len() >= GROUP_OTHERS => {
                    refuse(es("El grupo ya tiene 10 personas."))
                }
                Command::AddGroupMember { .. } => Ok(()),
                _ if group.owner.is_none() || group.owner != me => {
                    refuse(es("Solo quien es dueño del grupo puede hacerlo."))
                }
                _ if !inside => refuse(es("Esa persona ya no está en el grupo.")),
                _ => Ok(()),
            }
        }
        Command::CloseDirect { channel } => {
            if conversation(*channel, ChannelKind::Direct) {
                Ok(())
            } else {
                refuse(es("Esa conversación ya no está en tu lista."))
            }
        }
        _ => refuse(es(
            "Esta acción todavía no está disponible en esta conexión.",
        )),
    }
}

/// Keep what a write's answer confirmed: a conversation listed, changed or
/// gone, and its people. A direct conversation gone is kept in `closed`.
pub(crate) fn confirmed(snapshot: &mut Snapshot, closed: &mut Vec<Channel>, event: &Event) {
    match event {
        Event::UserChanged(user) => {
            if let Some(known) = snapshot.users.iter_mut().find(|known| known.id == user.id) {
                *known = user.clone();
            } else {
                snapshot.users.push(user.clone());
            }
        }
        Event::ChannelChanged(channel) if channel.server.is_none() => {
            closed.retain(|known| known.id != channel.id);
            match snapshot
                .channels
                .iter_mut()
                .find(|known| known.id == channel.id)
            {
                Some(known) => *known = channel.clone(),
                None => snapshot.channels.push(channel.clone()),
            }
        }
        Event::ChannelRemoved { channel } => forget(snapshot, closed, *channel),
        _ => {}
    }
}

/// Before `channel` leaves the list: a direct conversation is remembered,
/// as closing one deletes nothing.
pub(crate) fn forget(snapshot: &mut Snapshot, closed: &mut Vec<Channel>, channel: ChannelId) {
    let Some(index) = snapshot
        .channels
        .iter()
        .position(|known| known.id == channel)
    else {
        return;
    };
    let gone = snapshot.channels.remove(index);
    if gone.kind == ChannelKind::Direct {
        closed.retain(|known| known.id != channel);
        if closed.len() >= CLOSED_MAX {
            closed.remove(0);
        }
        closed.push(gone);
    }
}

/// A message in a direct conversation closed here lists it again first, as
/// the official client shows it back; `None` for any other.
pub(crate) fn reopen(
    snapshot: &mut Snapshot,
    closed: &mut Vec<Channel>,
    data: &Value,
) -> Option<Event> {
    if !data["guild_id"].is_null() {
        return None;
    }
    let channel = ChannelId(wire::id(&data["channel_id"]).ok()?);
    let index = closed.iter().position(|known| known.id == channel)?;
    let channel = closed.remove(index);
    if snapshot.channels.iter().any(|known| known.id == channel.id) {
        return None;
    }
    snapshot.channels.push(channel.clone());
    Some(Event::ChannelChanged(channel))
}

/// Someone whose presence the friends page shows: a friend, or the other
/// person of a direct conversation.
fn followed(snapshot: &Snapshot, user: UserId) -> bool {
    snapshot.me.as_ref().is_none_or(|me| me.id != user)
        && (snapshot.relationships.as_ref().is_some_and(|list| {
            list.iter()
                .any(|known| known.user == user && known.kind == RelationshipKind::Friend)
        }) || snapshot
            .channels
            .iter()
            .any(|known| known.kind == ChannelKind::Direct && known.recipients == [user]))
}

/// A presence object's person and presence; `None` when malformed.
fn presence_of(value: &Value) -> Option<(UserId, rune_model::Presence)> {
    let user = wire::id(&value["user"]["id"])
        .or_else(|_| wire::id(&value["user_id"]))
        .ok()?;
    Some((UserId(user), members::presence(value)?))
}

/// READY's `presences`, kept for the people the account follows. Without
/// them a friend shows offline, as in the official client.
pub(crate) fn ready_presences(snapshot: &mut Snapshot, value: &Value) {
    let Some(rows) = value.as_array() else {
        return;
    };
    for (user, presence) in rows.iter().take(PRESENCES_MAX).filter_map(presence_of) {
        if followed(snapshot, user) && !snapshot.presences.iter().any(|(known, _)| *known == user) {
            snapshot.presences.push((user, presence));
        }
    }
}

/// The friends' presences of a READY_SUPPLEMENTAL, as changes.
pub(crate) fn supplemental_presences(snapshot: &Snapshot, data: &Value) -> Vec<Event> {
    let rows = data["merged_presences"]["friends"]
        .as_array()
        .or_else(|| data["presences"].as_array());
    rows.into_iter()
        .flatten()
        .take(PRESENCES_MAX)
        .filter_map(presence_of)
        .filter(|(user, _)| followed(snapshot, *user))
        .map(|(user, presence)| Event::PresenceChanged { user, presence })
        .collect()
}

/// A PRESENCE_UPDATE outside any server, for someone the account follows.
pub(crate) fn presence(snapshot: &Snapshot, data: &Value) -> Option<Event> {
    if !data["guild_id"].is_null() {
        return None;
    }
    let (user, presence) = presence_of(data)?;
    followed(snapshot, user).then_some(Event::PresenceChanged { user, presence })
}

/// Read READY's `relationships`. Absent or not a list: unknown.
pub(crate) fn ready(snapshot: &mut Snapshot, value: &Value) {
    let Some(rows) = value
        .as_array()
        .filter(|rows| rows.len() <= MAX_RELATIONSHIPS)
    else {
        return;
    };
    let mut list: Vec<Relationship> = Vec::new();
    for row in rows {
        if let Some(relationship) = decode(snapshot, row, None) {
            if !list.iter().any(|known| known.user == relationship.user) {
                list.push(relationship);
            }
        }
    }
    snapshot.relationships = Some(list);
}

/// Translate a relationship or group-membership dispatch; `None` for any
/// other kind. Malformed payloads report nothing.
pub(crate) fn dispatch(snapshot: &mut Snapshot, kind: &str, data: &Value) -> Option<Vec<Event>> {
    Some(match kind {
        "RELATIONSHIP_ADD" | "RELATIONSHIP_UPDATE" => changed(snapshot, data).unwrap_or_default(),
        "RELATIONSHIP_REMOVE" => {
            let Ok(user) = wire::id(&data["id"]).map(UserId) else {
                return Some(Vec::new());
            };
            if let Some(list) = snapshot.relationships.as_mut() {
                list.retain(|known| known.user != user);
            }
            vec![Event::RelationshipRemoved { user }]
        }
        "CHANNEL_RECIPIENT_ADD" | "CHANNEL_RECIPIENT_REMOVE" => {
            recipient(snapshot, data, kind == "CHANNEL_RECIPIENT_ADD").unwrap_or_default()
        }
        _ => return None,
    })
}

fn changed(snapshot: &mut Snapshot, data: &Value) -> Option<Vec<Event>> {
    snapshot.relationships.as_ref()?;
    let user = UserId(wire::id(&data["id"]).ok()?);
    let list = snapshot.relationships.as_ref()?;
    let previous = list.iter().find(|known| known.user == user).cloned();
    if previous.is_none() && list.len() >= MAX_RELATIONSHIPS {
        return None;
    }
    let users = snapshot.users.len();
    let mut relationship = decode(snapshot, data, previous.as_ref().map(|known| known.kind))?;
    if let Some(previous) = previous {
        // An update may carry only some fields: keep what it leaves out.
        if data.get("since").is_none() {
            relationship.since = previous.since;
        }
        if data.get("nickname").is_none() {
            relationship.nickname = previous.nickname;
        }
        if data.get("is_spam_request").is_none() {
            relationship.spam = previous.spam && relationship.kind == previous.kind;
        }
    }
    let list = snapshot.relationships.as_mut()?;
    list.retain(|known| known.user != user);
    list.push(relationship.clone());
    let mut events = Vec::new();
    if data["user"].is_object() || snapshot.users.len() != users {
        if let Some(known) = snapshot.users.iter().find(|known| known.id == user) {
            events.push(Event::UserChanged(known.clone()));
        }
    }
    events.push(Event::RelationshipChanged(relationship));
    Some(events)
}

/// One relationship row, or `None` when it is malformed or about the account
/// itself. A request Discord files as spam (`is_spam_request`, as the
/// official client reads it) is kept, marked, for its own folder. `kind`
/// stands in for a missing `type`, as a partial update leaves it.
fn decode(
    snapshot: &mut Snapshot,
    row: &Value,
    kind: Option<RelationshipKind>,
) -> Option<Relationship> {
    let user = wire::id(&row["id"])
        .or_else(|_| wire::id(&row["user_id"]))
        .ok()?;
    for other in [&row["user_id"], &row["user"]["id"]] {
        if !other.is_null() && wire::id(other).ok()? != user {
            return None;
        }
    }
    let kind = match row.get("type").map(Value::as_u64) {
        Some(Some(1)) => RelationshipKind::Friend,
        Some(Some(2)) => RelationshipKind::Blocked,
        Some(Some(3)) => RelationshipKind::IncomingRequest,
        Some(Some(4)) => RelationshipKind::OutgoingRequest,
        None => kind?,
        _ => return None,
    };
    if snapshot.me.as_ref().is_some_and(|me| me.id.0 == user) {
        return None;
    }
    if row["user"].is_object() {
        bootstrap::add_user(snapshot, &row["user"]).ok()?;
    }
    let nickname = row["nickname"]
        .as_str()
        .filter(|text| {
            !text.trim().is_empty()
                && text.chars().count() <= Relationship::NICKNAME_MAX
                && !text.chars().any(char::is_control)
        })
        .map(str::to_owned);
    Some(Relationship {
        user: UserId(user),
        kind,
        since: wire::timestamp(&row["since"]).ok(),
        nickname,
        spam: kind == RelationshipKind::IncomingRequest
            && row["is_spam_request"].as_bool() == Some(true),
    })
}

fn recipient(snapshot: &mut Snapshot, data: &Value, added: bool) -> Option<Vec<Event>> {
    let channel = ChannelId(wire::id(&data["channel_id"]).ok()?);
    let user = UserId(wire::id(&data["user"]["id"]).ok()?);
    let me = snapshot.me.as_ref().map(|me| me.id);
    let index = snapshot
        .channels
        .iter()
        .position(|known| known.id == channel && known.kind == ChannelKind::Group)?;
    let mut events = Vec::new();
    if added {
        bootstrap::add_user(snapshot, &data["user"]).ok()?;
        let group = &mut snapshot.channels[index];
        if !group.recipients.contains(&user) && Some(user) != me && group.recipients.len() < 64 {
            group.recipients.push(user);
        }
        if let Some(known) = snapshot.users.iter().find(|known| known.id == user) {
            events.push(Event::UserChanged(known.clone()));
        }
    } else if Some(user) == me {
        snapshot.channels.remove(index);
        return Some(vec![Event::ChannelRemoved { channel }]);
    } else {
        snapshot.channels[index]
            .recipients
            .retain(|known| *known != user);
    }
    events.push(Event::ChannelChanged(snapshot.channels[index].clone()));
    Some(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session() -> Snapshot {
        let me = wire::user(&json!({"id":"9","username":"me"})).unwrap();
        let mut snapshot = bootstrap::ready(
            &json!({
                "user":{"id":"9"},
                "users":[{"id":"8","username":"friend"}],
                "private_channels":[{"id":"30","type":3,"owner_id":"9","recipient_ids":["8"]}],
                "relationships":[
                    {"id":"8","type":1,"nickname":"Amiga","since":"2024-05-01T10:00:00+00:00"},
                    {"id":"7","type":3,"user":{"id":"7","username":"asks"}},
                    {"id":"6","type":3,"is_spam_request":true},
                    {"id":"5","type":4},
                    {"id":"4","type":2},
                    {"id":"3","type":99},
                    {"id":"bad","type":1},
                    {"id":"9","type":1},
                    {"id":"8","type":2},
                    {"id":"2","user_id":"1","type":1}
                ]
            }),
            &me,
        )
        .unwrap();
        snapshot.me = Some(me);
        snapshot
    }

    /// A friend READY lists with a picture keeps it; one without keeps
    /// Discord's default, never only an initial (2026-10-19).
    #[test]
    fn ready_friends_carry_their_picture_or_the_default_one() {
        let me = wire::user(&json!({"id":"9","username":"me"})).unwrap();
        let hash = "0123456789abcdef0123456789abcdef";
        let snapshot = bootstrap::ready(
            &json!({
                "user":{"id":"9"},
                "relationships":[
                    {"id":"8","type":1,"user":{"id":"8","username":"deity","avatar":hash}},
                    {"id":"4194304","type":1,"user":{"id":"4194304","username":"adanger","avatar":null,"discriminator":"0"}},
                    {"id":"7","type":1,"user":{"id":"7","username":"viejo","discriminator":"1337"}}
                ]
            }),
            &me,
        )
        .unwrap();
        let user = |name: &str| {
            snapshot
                .users
                .iter()
                .find(|user| user.name == name)
                .unwrap()
        };
        assert_eq!(
            user("deity").picture().unwrap().url,
            format!("https://cdn.discordapp.com/avatars/8/{hash}.png?size=128")
        );
        // (4194304 >> 22) % 6 = 1; an old tag 1337 % 5 = 2.
        for (name, index) in [("adanger", 1), ("viejo", 2)] {
            assert!(user(name).avatar.is_none());
            assert_eq!(
                user(name).picture().unwrap().url,
                format!("https://cdn.discordapp.com/embed/avatars/{index}.png")
            );
        }
    }

    #[test]
    fn ready_keeps_valid_rows_once_and_drops_the_rest_without_failing() {
        let snapshot = session();
        let list = snapshot.relationships.as_ref().unwrap();
        let kinds: Vec<_> = list
            .iter()
            .map(|known| (known.user.0, known.kind))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (8, RelationshipKind::Friend),
                (7, RelationshipKind::IncomingRequest),
                (6, RelationshipKind::IncomingRequest),
                (5, RelationshipKind::OutgoingRequest),
                (4, RelationshipKind::Blocked),
            ]
        );
        // Spam waits in its own folder; only an incoming request can be spam.
        let spam: Vec<_> = list.iter().map(|known| known.spam).collect();
        assert_eq!(spam, vec![false, false, true, false, false]);
        assert_eq!(list[0].nickname.as_deref(), Some("Amiga"));
        assert!(list[0].since.is_some());
        assert!(snapshot.users.iter().any(|user| user.name == "asks"));
        let group = &snapshot.channels[0];
        assert_eq!(group.owner, Some(UserId(9)));
        // Without the field the list is unknown, not empty.
        let me = wire::user(&json!({"id":"9","username":"me"})).unwrap();
        let bare = bootstrap::ready(&json!({"user":{"id":"9"}}), &me).unwrap();
        assert!(bare.relationships.is_none());
        let oversized = bootstrap::ready(
            &json!({"user":{"id":"9"},"relationships":vec![json!({"id":"8","type":1}); MAX_RELATIONSHIPS + 1]}),
            &me,
        )
        .unwrap();
        assert!(oversized.relationships.is_none());
    }

    #[test]
    fn dispatches_add_update_and_remove_one_row_per_person() {
        let mut snapshot = session();
        let events = dispatch(
            &mut snapshot,
            "RELATIONSHIP_ADD",
            &json!({"id":"5","type":1}),
        )
        .unwrap();
        assert!(matches!(&events[..], [Event::RelationshipChanged(r)]
            if r.user == UserId(5) && r.kind == RelationshipKind::Friend));
        // A partial update keeps the kind and the date it leaves out.
        let events = dispatch(
            &mut snapshot,
            "RELATIONSHIP_UPDATE",
            &json!({"id":"8","nickname":"Otra"}),
        )
        .unwrap();
        assert!(matches!(&events[..], [Event::RelationshipChanged(r)]
            if r.kind == RelationshipKind::Friend && r.since.is_some()
                && r.nickname.as_deref() == Some("Otra")));
        let count = snapshot.relationships.as_ref().unwrap().len();
        assert_eq!(count, 5);
        // An update that leaves the flag out keeps it; one that clears it, clears it.
        let events = dispatch(
            &mut snapshot,
            "RELATIONSHIP_UPDATE",
            &json!({"id":"6","nickname":null}),
        )
        .unwrap();
        assert!(matches!(&events[..], [Event::RelationshipChanged(r)] if r.spam));
        let events = dispatch(
            &mut snapshot,
            "RELATIONSHIP_UPDATE",
            &json!({"id":"6","is_spam_request":false}),
        )
        .unwrap();
        assert!(matches!(&events[..], [Event::RelationshipChanged(r)] if !r.spam));
        let events = dispatch(
            &mut snapshot,
            "RELATIONSHIP_REMOVE",
            &json!({"id":"5","type":1}),
        )
        .unwrap();
        assert_eq!(events, vec![Event::RelationshipRemoved { user: UserId(5) }]);
        assert_eq!(snapshot.relationships.as_ref().unwrap().len(), 4);
        for bad in [
            json!({"id":"x","type":1}),
            json!({"id":"12"}),
            json!({"id":"12","type":"1"}),
        ] {
            assert_eq!(
                dispatch(&mut snapshot, "RELATIONSHIP_ADD", &bad),
                Some(Vec::new())
            );
        }
        assert!(dispatch(&mut snapshot, "MESSAGE_CREATE", &json!({})).is_none());
    }

    #[test]
    fn group_membership_follows_recipient_dispatches() {
        let mut snapshot = session();
        let events = dispatch(
            &mut snapshot,
            "CHANNEL_RECIPIENT_ADD",
            &json!({"channel_id":"30","user":{"id":"7","username":"asks"}}),
        )
        .unwrap();
        assert!(
            matches!(events.last(), Some(Event::ChannelChanged(c)) if c.recipients == [UserId(8), UserId(7)])
        );
        let events = dispatch(
            &mut snapshot,
            "CHANNEL_RECIPIENT_REMOVE",
            &json!({"channel_id":"30","user":{"id":"8","username":"friend"}}),
        )
        .unwrap();
        assert!(matches!(&events[..], [Event::ChannelChanged(c)] if c.recipients == [UserId(7)]));
        let events = dispatch(
            &mut snapshot,
            "CHANNEL_RECIPIENT_REMOVE",
            &json!({"channel_id":"30","user":{"id":"9","username":"me"}}),
        )
        .unwrap();
        assert_eq!(
            events,
            vec![Event::ChannelRemoved {
                channel: ChannelId(30)
            }]
        );
    }

    #[test]
    fn writes_go_out_only_about_what_this_session_knows() {
        let snapshot = session();
        let ok = |command: Command| allowed(&snapshot, &command).is_ok();
        assert!(ok(Command::AcceptFriendRequest { user: UserId(7) }));
        assert!(!ok(Command::AcceptFriendRequest { user: UserId(8) }));
        assert!(ok(Command::SetFriendNickname {
            user: UserId(8),
            nickname: None
        }));
        assert!(!ok(Command::SetFriendNickname {
            user: UserId(7),
            nickname: None
        }));
        assert!(!ok(Command::RemoveRelationship { user: UserId(3) }));
        assert!(!ok(Command::BlockUser { user: UserId(9) }));
        assert!(ok(Command::RenameGroup {
            channel: ChannelId(30),
            name: "Asado".into()
        }));
        assert!(!ok(Command::CloseDirect {
            channel: ChannelId(30)
        }));
        // Not routes Serein uses: refused here too, whatever the interface asks.
        assert!(!ok(Command::CreateGroup {
            request: 1,
            users: vec![UserId(8), UserId(7)]
        }));
        assert!(!ok(Command::AddGroupMember {
            channel: ChannelId(30),
            user: UserId(7)
        }));
    }

    #[test]
    fn a_closed_direct_conversation_is_listed_again_by_its_next_message() {
        let mut snapshot = session();
        let (direct, _) = bootstrap::private_channel(
            &json!({"id":"40","type":1,"recipients":[{"id":"8","username":"friend"}]}),
        )
        .unwrap();
        let mut closed = Vec::new();
        confirmed(
            &mut snapshot,
            &mut closed,
            &Event::ChannelChanged(direct.clone()),
        );
        assert!(snapshot.channels.iter().any(|c| c.id == ChannelId(40)));
        confirmed(
            &mut snapshot,
            &mut closed,
            &Event::ChannelRemoved {
                channel: ChannelId(40),
            },
        );
        assert!(!snapshot.channels.iter().any(|c| c.id == ChannelId(40)));
        assert_eq!(closed.len(), 1);
        // A message elsewhere, or in a server, lists nothing.
        assert!(reopen(&mut snapshot, &mut closed, &json!({"channel_id":"41"})).is_none());
        assert!(reopen(
            &mut snapshot,
            &mut closed,
            &json!({"channel_id":"40","guild_id":"2"})
        )
        .is_none());
        let event = reopen(&mut snapshot, &mut closed, &json!({"channel_id":"40"})).unwrap();
        assert_eq!(event, Event::ChannelChanged(direct));
        assert!(closed.is_empty());
        // Leaving a group keeps nothing to reopen.
        forget(&mut snapshot, &mut closed, ChannelId(30));
        assert!(closed.is_empty());
    }

    #[test]
    fn friends_presence_comes_from_ready_and_from_updates_outside_servers() {
        let mut snapshot = session();
        ready_presences(
            &mut snapshot,
            &json!([
                {"user":{"id":"8"},"status":"online","activities":[]},
                // Only asked to be friends: not followed.
                {"user":{"id":"7"},"status":"idle"},
                {"user_id":"x","status":"online"}
            ]),
        );
        let users: Vec<_> = snapshot.presences.iter().map(|(user, _)| *user).collect();
        assert_eq!(users, vec![UserId(8)]);
        assert!(presence(&snapshot, &json!({"user":{"id":"8"},"status":"dnd"})).is_some());
        assert!(presence(
            &snapshot,
            &json!({"user":{"id":"8"},"status":"dnd","guild_id":"2"})
        )
        .is_none());
        assert!(presence(&snapshot, &json!({"user":{"id":"5"},"status":"dnd"})).is_none());
        let events = supplemental_presences(
            &snapshot,
            &json!({"merged_presences":{"friends":[{"user_id":"8","status":"idle"}]}}),
        );
        assert!(matches!(&events[..], [Event::PresenceChanged { user, .. }] if *user == UserId(8)));
    }
}
