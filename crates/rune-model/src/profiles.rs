//! What keeps a member from taking part in a server, and the parts of a
//! profile loaded on demand. Both are scoped: restrictions to one server,
//! a loaded profile to the server it was asked in (or none).

use serde::{Deserialize, Serialize};

use crate::identity::{MAX_BADGES, MAX_CONNECTIONS};
use crate::{
    Badge, ChannelId, Connection, ProfileBanner, Rgb, RoleId, ServerId, Timestamp, UserId,
};

/// Mutual servers and friends kept per loaded profile, at most.
pub const MAX_MUTUALS: usize = 100;

/// What the provider reports keeps a member from taking part in a server.
/// The default restricts nothing; rows kept before it existed read so.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemberRestrictions {
    /// Timed out: may read but not take part until then.
    #[serde(default)]
    pub timeout_until: Option<Timestamp>,
    /// The server's membership screening or verification is not complete.
    #[serde(default)]
    pub pending: bool,
    /// Held back by automatic moderation until something of theirs changes.
    #[serde(default)]
    pub quarantined: bool,
    /// Held back by automatic moderation for the server tag they show,
    /// until they change it or stop showing it.
    #[serde(default)]
    pub tag_quarantined: bool,
    // Rules and onboarding (2026-10-18).
    /// Finished the server's onboarding, as last reported. It restricts
    /// nothing: unanswered, only the roles and channels it gives are missing.
    #[serde(default)]
    pub onboarded: bool,
    /// Let in as a guest, to one voice channel: never asked the onboarding.
    #[serde(default)]
    pub guest: bool,
}

/// The restriction in force at one moment, the one shown first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Restriction {
    TimedOut {
        until: Timestamp,
    },
    Pending,
    Quarantined,
    /// Held back for the server tag they show.
    TagQuarantined,
}

impl MemberRestrictions {
    /// What restricts the member at `now`. A timeout past its end is none.
    pub fn at(&self, now: Timestamp) -> Option<Restriction> {
        match self.timeout_until {
            Some(until) if until > now => Some(Restriction::TimedOut { until }),
            _ if self.pending => Some(Restriction::Pending),
            _ if self.quarantined => Some(Restriction::Quarantined),
            _ if self.tag_quarantined => Some(Restriction::TagQuarantined),
            _ => None,
        }
    }
}

/// The parts of someone's profile a provider loads on demand. `None` is
/// "not reported", never "none": an empty list is a confirmed absence.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UserProfile {
    pub user: UserId,
    /// The server it was asked in; server-scoped data belongs only there.
    pub server: Option<ServerId>,
    /// When the account was created.
    pub created: Option<Timestamp>,
    pub mutual_servers: Option<Vec<ServerId>>,
    pub mutual_friends: Option<Vec<UserId>>,
    /// Their account-wide text and artwork, when the provider read them.
    pub about: Option<String>,
    pub pronouns: Option<String>,
    pub banner: ProfileBanner,
    pub accent: Option<Rgb>,
    /// What they set for `server` only, when reported there.
    pub nick: Option<String>,
    pub server_about: Option<String>,
    pub server_pronouns: Option<String>,
    pub server_banner: ProfileBanner,
    /// When they joined `server`.
    pub joined: Option<Timestamp>,
    // Presence, identity and expressions (P5).
    /// Their badges, the account's and `server`'s, in order.
    pub badges: Option<Vec<Badge>>,
    /// The accounts elsewhere they show.
    pub connections: Option<Vec<Connection>>,
    /// The effect over their card, the server's first.
    pub effect: Option<crate::ProfileEffect>,
    /// The pictures their texts name («Sobre mí», the server's too).
    pub pictures: Vec<crate::TextPicture>,
}

impl UserProfile {
    /// Whether it answers a request for `user` in `server` within bounds.
    pub fn answers(&self, user: UserId, server: Option<ServerId>) -> bool {
        let bounded = |list: Option<usize>| list.is_none_or(|len| len <= MAX_MUTUALS);
        let within = |list: Option<usize>, max: usize| list.is_none_or(|len| len <= max);
        self.user == user
            && self.server == server
            && bounded(self.mutual_servers.as_ref().map(Vec::len))
            && bounded(self.mutual_friends.as_ref().map(Vec::len))
            && within(self.badges.as_ref().map(Vec::len), MAX_BADGES)
            && within(self.connections.as_ref().map(Vec::len), MAX_CONNECTIONS)
            && within(
                self.effect.as_ref().map(|effect| effect.layers.len()),
                crate::identity::MAX_EFFECT_LAYERS,
            )
            && self.pictures.len() <= crate::MAX_TEXT_PICTURES
    }
}

/// A heading of a server's member list, as its provider groups people.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ListGroup {
    /// A role shown apart: its members who are around.
    Role(RoleId),
    Online,
    Offline,
}

/// One row of a member list: a heading and how many follow it, or a person.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListRow {
    Heading { group: ListGroup, count: u32 },
    Member(UserId),
}

/// Rows of a member list a provider keeps, at most: what is in view and
/// around it. Groups beyond this are dropped, never cut in the middle.
pub const LIST_ROWS_MAX: usize = 1000;
/// Headings of a member list, at most: every role, online and offline.
pub const LIST_GROUPS_MAX: usize = 514;

/// A server's member list as its provider orders it, seen from `channel`:
/// its groups with their counts, the total of rows, and only the rows it
/// sent (the ones near what is in view). A row not sent is not known.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemberList {
    pub server: ServerId,
    pub channel: ChannelId,
    /// Each group's heading and how many people it has, in order.
    pub groups: Vec<(ListGroup, u32)>,
    /// The rows sent, by their place in the whole list, in order.
    pub rows: Vec<(u32, ListRow)>,
}

impl MemberList {
    /// How many rows the whole list has: a heading per group that has
    /// people, and its people.
    pub fn len(&self) -> u32 {
        self.groups
            .iter()
            .filter(|(_, count)| *count > 0)
            .fold(0u32, |total, (_, count)| {
                total.saturating_add(count.saturating_add(1))
            })
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The row at `index`: as sent, else the heading the counts put there,
    /// else `None` for a person not sent yet.
    pub fn row(&self, index: u32) -> Option<ListRow> {
        if let Ok(found) = self.rows.binary_search_by_key(&index, |(at, _)| *at) {
            return Some(self.rows[found].1);
        }
        let mut start = 0u32;
        for (group, count) in self.groups.iter().filter(|(_, count)| *count > 0) {
            if index == start {
                return Some(ListRow::Heading {
                    group: *group,
                    count: *count,
                });
            }
            start = start.saturating_add(count.saturating_add(1));
            if index < start {
                return None;
            }
        }
        None
    }

    /// Whether it is within bounds, in order and without a person twice.
    pub fn valid(&self) -> bool {
        let len = self.len();
        let mut seen = std::collections::HashSet::new();
        self.groups.len() <= LIST_GROUPS_MAX
            && self.rows.len() <= LIST_ROWS_MAX
            && self.rows.windows(2).all(|pair| pair[0].0 < pair[1].0)
            && self.rows.iter().all(|(at, row)| {
                *at < len && !matches!(row, ListRow::Member(user) if !seen.insert(*user))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timeout_restricts_only_until_it_ends_and_comes_before_the_rest() {
        let restrictions = MemberRestrictions {
            timeout_until: Some(Timestamp(100)),
            pending: true,
            quarantined: false,
            tag_quarantined: true,
            onboarded: false,
            guest: false,
        };
        assert_eq!(
            restrictions.at(Timestamp(99)),
            Some(Restriction::TimedOut {
                until: Timestamp(100)
            })
        );
        assert_eq!(restrictions.at(Timestamp(100)), Some(Restriction::Pending));
        let tagged = MemberRestrictions {
            pending: false,
            ..restrictions
        };
        assert_eq!(tagged.at(Timestamp(100)), Some(Restriction::TagQuarantined));
        assert_eq!(MemberRestrictions::default().at(Timestamp(0)), None);
    }

    #[test]
    fn a_profile_answers_only_its_own_scope_within_bounds() {
        let profile = UserProfile {
            user: UserId(7),
            server: Some(ServerId(1)),
            mutual_servers: Some(vec![ServerId(1)]),
            ..Default::default()
        };
        assert!(profile.answers(UserId(7), Some(ServerId(1))));
        assert!(!profile.answers(UserId(7), None), "other scope");
        assert!(
            !profile.answers(UserId(8), Some(ServerId(1))),
            "other person"
        );
        let flood = UserProfile {
            mutual_friends: Some(vec![UserId(1); MAX_MUTUALS + 1]),
            ..profile.clone()
        };
        assert!(!flood.answers(UserId(7), Some(ServerId(1))));
        let badge = Badge {
            id: "x".into(),
            description: "X".into(),
            icon: None,
            link: None,
        };
        let badges = UserProfile {
            badges: Some(vec![badge; MAX_BADGES + 1]),
            ..profile
        };
        assert!(
            !badges.answers(UserId(7), Some(ServerId(1))),
            "too many badges"
        );
    }

    #[test]
    fn a_member_list_places_headings_by_counts_and_knows_only_what_was_sent() {
        let list = MemberList {
            groups: vec![
                (ListGroup::Role(RoleId(5)), 2),
                (ListGroup::Online, 0),
                (ListGroup::Offline, 3),
            ],
            rows: vec![
                (1, ListRow::Member(UserId(1))),
                (4, ListRow::Member(UserId(2))),
            ],
            ..Default::default()
        };
        assert_eq!(list.len(), 7, "two headings, five people");
        assert_eq!(
            list.row(0),
            Some(ListRow::Heading {
                group: ListGroup::Role(RoleId(5)),
                count: 2
            })
        );
        assert_eq!(list.row(1), Some(ListRow::Member(UserId(1))));
        assert_eq!(list.row(2), None, "not sent");
        assert_eq!(
            list.row(3),
            Some(ListRow::Heading {
                group: ListGroup::Offline,
                count: 3
            }),
            "an empty group has no heading"
        );
        assert_eq!(list.row(7), None);
        assert!(list.valid());
        let twice = MemberList {
            rows: vec![
                (1, ListRow::Member(UserId(1))),
                (2, ListRow::Member(UserId(1))),
            ],
            ..list.clone()
        };
        assert!(!twice.valid());
        let outside = MemberList {
            rows: vec![(7, ListRow::Member(UserId(9)))],
            ..list
        };
        assert!(!outside.valid());
    }
}
