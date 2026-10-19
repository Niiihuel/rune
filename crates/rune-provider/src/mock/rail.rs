//! The simulation's rail and notifications (2026-10-18): one of the people
//! in the busiest voice room of the first server, who neither streams nor
//! shows a camera, uses an app there, so the rail's hint has its row; and
//! what a message tells beyond names, as the service decides it.
//! Deterministic, and nothing of it runs.

use rune_model::{permissions, ChannelId, ChannelKind, Permissions, RoleId, UserId, VoiceActivity};

use super::World;
use crate::Event;

impl World {
    /// What a session reports of the apps in voice once it is ready.
    pub(super) fn voice_activities(&self) -> Vec<Event> {
        let Some(server) = self.servers.first().map(|server| server.id) else {
            return Vec::new();
        };
        // As the service: apps launch in voice rooms, never on a stage.
        let in_server = |channel: ChannelId| {
            self.channel(channel).is_some_and(|found| {
                found.server == Some(server) && found.kind == ChannelKind::Voice
            })
        };
        let count = |channel: ChannelId| {
            self.voice
                .iter()
                .filter(|state| state.channel == channel)
                .count()
        };
        let Some(room) = self
            .voice
            .iter()
            .map(|state| state.channel)
            .filter(|channel| in_server(*channel))
            .max_by_key(|channel| (count(*channel), std::cmp::Reverse(channel.0)))
        else {
            return Vec::new();
        };
        let people = self
            .voice
            .iter()
            .filter(|state| state.channel == room && !state.streaming && !state.camera)
            .map(|state| state.user)
            .take(2)
            .collect::<Vec<_>>();
        if people.is_empty() {
            return Vec::new();
        }
        vec![Event::VoiceActivity(VoiceActivity {
            id: format!("{room}:1"),
            channel: room,
            people,
        })]
    }

    /// What `author` tells in `channel` by writing `content`, besides names,
    /// as the service decides: the roles written `<@&id>` it may mention
    /// (mentionable ones, or any when it may mention everyone), and
    /// whether `@everyone` or `@here` tells everyone.
    pub(super) fn told(
        &self,
        channel: ChannelId,
        author: UserId,
        content: &str,
    ) -> (Vec<RoleId>, bool) {
        let nothing = (Vec::new(), false);
        if !content.contains('@') {
            return nothing;
        }
        let Some(found) = self.channel(channel) else {
            return nothing;
        };
        // A thread's permissions are its parent's.
        let found = found
            .parent
            .filter(|_| found.is_thread())
            .and_then(|parent| self.channel(parent))
            .unwrap_or(found);
        let Some(server) = found.server.and_then(|server| self.server(server).ok()) else {
            return nothing;
        };
        let may = self.member(server.id, author).is_some_and(|member| {
            permissions::in_channel(server, member, found).contains(Permissions::MENTION_EVERYONE)
        });
        let roles = server
            .roles
            .iter()
            .filter(|role| role.id != server.everyone_id() && (role.mentionable || may))
            .filter(|role| content.contains(&format!("<@&{}>", role.id)))
            .map(|role| role.id)
            .collect();
        let everyone = may
            && content.split_whitespace().any(|word| {
                let word = word.trim_end_matches(|character: char| !character.is_alphanumeric());
                word == "@everyone" || word == "@here"
            });
        (roles, everyone)
    }
}

#[cfg(test)]
mod tests {
    use rune_model::{ServerId, Timestamp};

    use super::*;
    use crate::mock::{message_id, MockConfig, ME};

    #[test]
    fn a_message_tells_the_roles_and_everyone_its_author_may() {
        let mut world = World::build(&MockConfig::default());
        let named = |world: &World, server: u64, name: &str| {
            world
                .channels
                .iter()
                .find(|found| found.server == Some(ServerId(server)) && found.name == name)
                .unwrap()
                .id
        };
        // The account owns the second server: it may tell any role and everyone.
        let owned = named(&world, 2, "limn-core");
        let sent = world
            .post(owned, ME, "<@&22> <@&21> @here".into(), None, Timestamp(1))
            .unwrap();
        assert_eq!(sent.mention_roles, [RoleId(21), RoleId(22)]);
        assert!(sent.mention_everyone);
        // In the first, only the role anyone may mention; everyone, nobody.
        let general = named(&world, 1, "general");
        let sent = world
            .post(
                general,
                ME,
                "<@&12> <@&11> @everyone".into(),
                None,
                Timestamp(1),
            )
            .unwrap();
        assert_eq!(sent.mention_roles, [RoleId(11)]);
        assert!(!sent.mention_everyone);
        // The history has one, of a role the account may take where it owns.
        let parts = named(&world, 2, "componentes");
        let total = world.sims[&parts].generated;
        let seeded = world.message(parts, message_id(parts, total - 2)).unwrap();
        assert_eq!(seeded.mention_roles, [RoleId(21)]);
    }
}
