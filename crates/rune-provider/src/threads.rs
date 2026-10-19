//! Threads and forum posts: what is asked to start, change, follow or page
//! through them. A provider reports each thread as a channel
//! ([`Event::ChannelChanged`](crate::Event)) once the service confirms it,
//! so nothing here is shown before that.

use rune_model::TagId;

/// Which threads of a channel or forum a page lists. A channel's are read in
/// this order: the open ones the session was not told of (a service may
/// report only those the account follows), then the archived ones.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ThreadScope {
    /// Open ones, the most recently active first.
    #[default]
    Active,
    /// Archived public ones.
    Archived,
    /// Every archived private one: only moderation may read them all.
    Private,
    /// The archived private ones the account is in.
    Joined,
}

/// Changes to a thread; `None` leaves a field as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadEdit {
    pub name: Option<String>,
    /// Closed: it leaves the active list and takes no reactions.
    pub archived: Option<bool>,
    /// Only moderation may reply or reopen it.
    pub locked: Option<bool>,
    /// Every forum tag the post carries.
    pub tags: Option<Vec<TagId>>,
}

impl ThreadEdit {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}
