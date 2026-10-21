//! The account's explicit content filter applied to what it reads: which
//! pictures the service found explicit wait blurred until the account asks
//! to see each, by where they were posted and who posted them, as the
//! official client decides it (docs/DISCORD.md, contenido-de-mensajes).

use rune_model::{ChannelId, ContentFilter, MessagingPrivacy, RelationshipKind, UserId};

use crate::Store;

impl Store {
    /// Whether the explicit pictures `author` posts in `channel` wait
    /// blurred: the account's level for servers, for friends' private
    /// conversations or for everyone else's. The account's own never do,
    /// nor those of a conversation RUNE does not know.
    ///
    /// Until a provider that keeps the levels reports them, everything
    /// explicit waits blurred. A level never chosen is the service's
    /// default for an adult account:
    /// shown in servers and from friends, hidden from anyone else. Blocking
    /// is the service's to do; here it blurs, as blurring does.
    pub fn obscures_explicit(&self, channel: ChannelId, author: UserId) -> bool {
        let Some(conversation) = self.channel(channel) else {
            return false;
        };
        if self.me() == Some(author) {
            return false;
        }
        // Levels a provider keeps but has not read yet, as from the cache
        // before READY: blurred, rather than show what the account may blur.
        if self.messaging_privacy().is_none() && self.capabilities().messaging_privacy {
            return true;
        }
        let level = |pick: fn(&MessagingPrivacy) -> ContentFilter| {
            self.messaging_privacy()
                .map_or(ContentFilter::Default, pick)
        };
        let filter = if conversation.server.is_some() {
            level(|privacy| privacy.explicit_servers)
        } else if self
            .relationship(author)
            .is_some_and(|relationship| relationship.kind == RelationshipKind::Friend)
        {
            level(|privacy| privacy.explicit_friends)
        } else {
            match level(|privacy| privacy.explicit_others) {
                ContentFilter::Default => ContentFilter::Block,
                chosen => chosen,
            }
        };
        matches!(filter, ContentFilter::Blur | ContentFilter::Block)
    }
}
