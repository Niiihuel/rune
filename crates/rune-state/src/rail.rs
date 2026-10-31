//! What the rail and the notifications read beyond who is in voice: the
//! apps launched in voice channels, the order of each server's voice
//! channels, and whether a message tells the account through its roles.

use std::collections::HashMap;

use rune_model::{ChannelId, Message, ServerId, UserId, VoiceActivity};

use crate::{Applied, Store};

impl Store {
    pub(crate) fn activity_changed(&mut self, activity: VoiceActivity) -> Applied {
        let channel = activity.channel;
        if activity.people.is_empty() {
            self.voice_activities.remove(&activity.id);
        } else {
            self.voice_activities.insert(activity.id.clone(), activity);
        }
        Applied::Voice(channel)
    }

    /// Where each channel of `server` the account sees is in its list.
    pub fn channel_order(&self, server: ServerId) -> HashMap<ChannelId, usize> {
        self.channel_tree(server)
            .into_iter()
            .flat_map(|(_, channels)| channels)
            .enumerate()
            .map(|(at, channel)| (channel.id, at))
            .collect()
    }

    /// The apps launched in `server`'s voice channels, in the order of its
    /// channels, and who uses each.
    pub fn voice_activities(&self, server: ServerId) -> Vec<&VoiceActivity> {
        let mut found: Vec<&VoiceActivity> = self
            .voice_activities
            .values()
            .filter(|activity| {
                self.channel(activity.channel)
                    .is_some_and(|channel| channel.server == Some(server))
            })
            .collect();
        // Asked for every server of the rail: the list is read only when needed.
        if found.len() > 1 {
            let order = self.channel_order(server);
            found.sort_by_key(|activity| {
                (
                    order.get(&activity.channel).copied().unwrap_or(usize::MAX),
                    activity.id.clone(),
                )
            });
        }
        found
    }

    /// Whether `message` tells the account through one of its roles in the
    /// server of its channel.
    pub fn mentions_my_roles(&self, message: &Message) -> bool {
        let Some(me) = self.me else {
            return false;
        };
        if message.mention_roles.is_empty() {
            return false;
        }
        self.channel(message.channel)
            .and_then(|channel| channel.server)
            .and_then(|server| self.member(server, me))
            .is_some_and(|member| {
                member
                    .roles
                    .iter()
                    .any(|role| message.mention_roles.contains(role))
            })
    }

    /// Whether mentions of the account's roles stop telling it in `server`,
    /// as the account keeps it with the provider.
    pub fn suppresses_roles(&self, server: Option<ServerId>) -> bool {
        self.synced_notifications()
            && self
                .notifications(server)
                .is_some_and(|kept| kept.suppress_roles)
    }

    /// Whether `@everyone` and `@here` stop telling the account in `server`.
    pub fn suppresses_everyone(&self, server: Option<ServerId>) -> bool {
        self.synced_notifications()
            && self
                .notifications(server)
                .is_some_and(|kept| kept.suppress_everyone)
    }

    /// Whether `message`, not the account's, counts as a mention of it: by
    /// name, or through its roles or everyone in a server unless it
    /// silenced those. Nothing an ignored person writes counts.
    pub fn counts_as_mention(&self, message: &Message, me: UserId) -> bool {
        if message.author == me || self.is_ignored(message.author) {
            return false;
        }
        let server = self
            .channel(message.channel)
            .and_then(|channel| channel.server);
        message.mentions(me)
            || (self.mentions_my_roles(message) && !self.suppresses_roles(server))
            || (message.mention_everyone && server.is_some() && !self.suppresses_everyone(server))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use rune_model::{ChannelKind, VoiceState};
    use rune_provider::mock::{MockConfig, MockProvider};
    use rune_provider::{Event, Provider};

    use super::*;

    #[test]
    fn voice_goes_by_channel_then_by_arrival_and_apps_come_and_go() {
        let provider = MockProvider::new(MockConfig::default());
        let mut store = Store::new();
        store.provider = Some(provider.info());
        store.apply(Event::Ready(Box::new(provider.snapshot())), Instant::now());
        let server = store.servers()[0].id;
        let rooms: Vec<ChannelId> = store
            .channel_tree(server)
            .into_iter()
            .flat_map(|(_, channels)| channels)
            .filter(|channel| channel.kind == ChannelKind::Voice)
            .map(|channel| channel.id)
            .collect();
        let busy: Vec<UserId> = store.voice_in(rooms[0]).iter().map(|s| s.user).collect();
        let someone = store
            .members(server)
            .iter()
            .map(|member| member.user)
            .find(|user| Some(*user) != store.me() && !busy.contains(user))
            .unwrap();
        let first = store.voice_in(rooms[0])[0].clone();
        // Someone arrives last in the first room: listed with its room.
        store.apply(
            Event::VoiceChanged(VoiceState {
                user: someone,
                channel: rooms[0],
                ..first
            }),
            Instant::now(),
        );
        let order = store.channel_order(server);
        let channels: Vec<ChannelId> = store.voice_by_server()[&server]
            .people
            .iter()
            .map(|state| state.channel)
            .collect();
        let mut sorted = channels.clone();
        sorted.sort_by_key(|channel| order[channel]);
        assert_eq!(channels, sorted);
        let launch = |people: Vec<UserId>| {
            Event::VoiceActivity(VoiceActivity {
                id: "a".into(),
                channel: rooms[0],
                people,
            })
        };
        store.apply(launch(vec![someone]), Instant::now());
        assert_eq!(store.voice_activities(server)[0].people, [someone]);
        store.apply(launch(Vec::new()), Instant::now());
        assert!(store.voice_activities(server).is_empty());
    }
}
