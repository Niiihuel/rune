//! The simulation's search through whole histories and its member lookup:
//! literal text over every made-up message, pages of 25, and a crowd of
//! people of «Taller de Rust» known only once looked up by name.

use std::collections::HashSet;

use rune_i18n::t;
use rune_model::{
    ChannelId, Member, Message, Permissions, RemoteOrder, RemotePage, RemoteSearch,
    RemoteSearchError, Rgb, ServerId, User, UserId, MAX_REMOTE_OFFSET, REMOTE_PAGE,
};

use super::{message_id, World};
use crate::{Event, RequestId};

/// The channel whose index the simulation «prepares» once per run, so the
/// first search through it answers as a service still indexing does.
pub(super) const INDEXED_LATE: &str = "rendimiento";
/// How long it asks to wait then.
const INDEXING_MS: u64 = 1_500;
/// The most members one lookup returns, as services cap it.
const LOOKUP_LIMIT: usize = 100;

const FIRST: [&str; 20] = [
    "Agustina",
    "Benjamín",
    "Carla",
    "Diego",
    "Emilia",
    "Federico",
    "Guadalupe",
    "Héctor",
    "Irene",
    "Joaquín",
    "Lara",
    "Martín",
    "Noelia",
    "Óscar",
    "Paula",
    "Ramiro",
    "Sofía",
    "Tomás",
    "Valeria",
    "Walter",
];
const LAST: [&str; 20] = [
    "Acosta",
    "Benítez",
    "Cabrera",
    "Domínguez",
    "Escobar",
    "Fernández",
    "Giménez",
    "Herrera",
    "Ibarra",
    "Juárez",
    "Ledesma",
    "Medina",
    "Navarro",
    "Ortiz",
    "Peralta",
    "Quiroga",
    "Romero",
    "Sosa",
    "Torres",
    "Vega",
];
/// Where the crowd's ids start, far from everyone else's.
const CROWD: u64 = 900_000;

/// Someone of the crowd: in «Taller de Rust», never in the snapshot.
fn crowd(index: usize) -> User {
    let (first, last) = (FIRST[index % FIRST.len()], LAST[index / FIRST.len()]);
    // Handles are plain ASCII, as services keep them.
    let handle: String = format!("{first}.{last}")
        .to_lowercase()
        .chars()
        .map(|letter| match letter {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' => 'u',
            other => other,
        })
        .collect();
    User {
        identity: Default::default(),
        id: UserId(CROWD + index as u64),
        name: handle,
        display_name: Some(format!("{first} {last}")),
        accent: Rgb(0x60_70_80 + (index as u32 * 0x01_03_05) % 0x40_40_40),
        avatar: None,
        banner: Default::default(),
        bot: false,
        pronouns: None,
        about: None,
        default_avatar: None,
    }
}

impl World {
    /// The channels `search` goes through that the account reads.
    fn searched(&self, search: &RemoteSearch) -> Vec<ChannelId> {
        let readable = |channel: &rune_model::Channel| {
            channel.kind.has_messages()
                && channel
                    .permissions
                    .contains(Permissions::VIEW | Permissions::READ_HISTORY)
        };
        match search.server {
            Some(server) => self
                .channels
                .iter()
                .filter(|channel| channel.server == Some(server) && readable(channel))
                .filter(|channel| {
                    search.channels.is_empty() || search.channels.contains(&channel.id)
                })
                .map(|channel| channel.id)
                .collect(),
            None => self
                .channel(search.channel)
                .filter(|channel| channel.server.is_none() && readable(channel))
                .map(|channel| vec![channel.id])
                .unwrap_or_default(),
        }
    }

    /// A page of `search` from `offset`: every made-up and written message
    /// of its channels, newest first. The first search through
    /// [`INDEXED_LATE`] in a run waits for its index instead.
    pub(super) fn search_messages(
        &mut self,
        request: RequestId,
        search: &RemoteSearch,
        offset: u32,
    ) -> Vec<Event> {
        let result = (|| {
            search
                .validate()
                .map_err(|reason| RemoteSearchError::Failed(reason.into()))?;
            if offset > MAX_REMOTE_OFFSET || !(offset as usize).is_multiple_of(REMOTE_PAGE) {
                return Err(RemoteSearchError::Failed(t("Esa página no existe.").into()));
            }
            if search.server.is_some_and(|server| {
                self.channel(search.channel)
                    .is_none_or(|channel| channel.server != Some(server))
            }) {
                return Err(RemoteSearchError::Failed(
                    t("Esa conversación no es de ese servidor.").into(),
                ));
            }
            let channels = self.searched(search);
            if channels.is_empty() {
                return Err(RemoteSearchError::Failed(
                    t("No podés leer el historial de esa conversación.").into(),
                ));
            }
            if let Some(late) = channels.iter().copied().find(|channel| {
                self.channel(*channel)
                    .is_some_and(|channel| channel.name == INDEXED_LATE)
            }) {
                if self.indexed.insert(late) {
                    return Err(RemoteSearchError::Indexing {
                        retry_after_ms: INDEXING_MS,
                    });
                }
            }
            // Who kind of author wrote it is the account's, not the message's.
            let plain = RemoteSearch {
                author_kinds: Vec::new(),
                ..search.clone()
            };
            let matches = plain.loaded_matcher().expect("no author kinds");
            let kind = |message: &Message| {
                let bot = self
                    .users
                    .iter()
                    .any(|user| user.id == message.author && user.bot);
                search.author_kinds.is_empty()
                    || search.author_kinds.iter().any(|kind| match kind {
                        rune_model::AuthorKind::User => !bot,
                        rune_model::AuthorKind::Bot => bot,
                        rune_model::AuthorKind::Webhook => false,
                    })
            };
            let mut found: Vec<Message> = Vec::new();
            for channel in channels {
                let Some(sim) = self.sims.get(&channel) else {
                    continue;
                };
                let total = sim.generated + sim.extra.len() as u64;
                found.extend(
                    (0..total)
                        .filter_map(|index| self.message(channel, message_id(channel, index)))
                        .filter(|message| matches(message) && kind(message)),
                );
            }
            match search.order {
                RemoteOrder::Newest => {
                    found.sort_by_key(|message| std::cmp::Reverse((message.timestamp, message.id)))
                }
                // The simulation's own ranking: how often the text appears,
                // then the newest; a service ranks by its own measure.
                RemoteOrder::Relevance => {
                    let needle = search.text.trim().to_lowercase();
                    found.sort_by_cached_key(|message| {
                        let times = if needle.is_empty() {
                            0
                        } else {
                            message.content.to_lowercase().matches(&needle).count()
                        };
                        std::cmp::Reverse((times, message.timestamp, message.id))
                    });
                }
            }
            let total = found.len() as u64;
            let messages = found
                .into_iter()
                .skip(offset as usize)
                .take(REMOTE_PAGE)
                .collect();
            Ok(RemotePage {
                messages,
                total,
                offset,
                partial: false,
            })
        })();
        let mut events: Vec<Event> = match &result {
            // The authors first, as a service names them with the page.
            Ok(page) => {
                let authors: HashSet<UserId> =
                    page.messages.iter().map(|message| message.author).collect();
                self.users
                    .iter()
                    .filter(|user| authors.contains(&user.id))
                    .cloned()
                    .map(Event::UserChanged)
                    .collect()
            }
            Err(_) => Vec::new(),
        };
        events.push(Event::SearchResults { request, result });
        events
    }

    /// The members of `server` whose name starts with `query`: the known
    /// ones and, in the first server, the crowd. Their accounts first.
    pub(super) fn search_members(&self, server: ServerId, query: &str) -> Vec<Event> {
        let query = query.trim().to_lowercase();
        if query.is_empty() || !self.servers.iter().any(|known| known.id == server) {
            return Vec::new();
        }
        let named = |user: &User| {
            user.name.to_lowercase().starts_with(&query)
                || user
                    .display_name
                    .as_ref()
                    .is_some_and(|name| name.to_lowercase().starts_with(&query))
        };
        let mut users: Vec<User> = self
            .members
            .iter()
            .filter(|member| member.server == server)
            .filter_map(|member| self.users.iter().find(|user| user.id == member.user))
            .filter(|user| named(user))
            .cloned()
            .collect();
        if self.servers.first().is_some_and(|first| first.id == server) {
            users.extend(
                (0..FIRST.len() * LAST.len())
                    .map(crowd)
                    .filter(|user| named(user)),
            );
        }
        users.truncate(LOOKUP_LIMIT);
        let members = users
            .iter()
            .map(|user| {
                self.members
                    .iter()
                    .find(|member| member.server == server && member.user == user.id)
                    .cloned()
                    .unwrap_or(Member {
                        user: user.id,
                        server,
                        ..Default::default()
                    })
            })
            .collect();
        let mut events: Vec<Event> = users.into_iter().map(Event::UserChanged).collect();
        events.push(Event::MembersFound {
            server,
            query,
            members,
        });
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crowd_is_deterministic_and_unique() {
        let people: HashSet<_> = (0..400).map(|index| crowd(index).name).collect();
        assert_eq!(people.len(), 400);
        assert_eq!(crowd(0).display_name.as_deref(), Some("Agustina Acosta"));
        assert_eq!(crowd(21).name, "benjamin.benitez");
    }
}
