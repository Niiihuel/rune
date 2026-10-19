//! Since when each voice channel has had someone in it (voz-protocolo), as
//! the provider tells it: the channel list shows how long it has been on.

use rune_model::{ChannelId, ServerId, Timestamp};

use crate::{Applied, Store};

impl Store {
    pub(crate) fn voice_started_changed(
        &mut self,
        server: ServerId,
        times: Vec<(ChannelId, Option<Timestamp>)>,
        whole: bool,
    ) -> Applied {
        let channels = &self.channels;
        // Only `server`'s own channels: another's, or one not known, is
        // never taken from its word.
        let of_server = |channel: &ChannelId| {
            channels
                .get(channel)
                .is_some_and(|found| found.server == Some(server))
        };
        if whole {
            self.voice_started.retain(|channel, _| {
                channels
                    .get(channel)
                    .is_some_and(|found| found.server != Some(server))
            });
        }
        for (channel, since) in times.into_iter().filter(|(channel, _)| of_server(channel)) {
            match since {
                Some(since) => self.voice_started.insert(channel, since),
                None => self.voice_started.remove(&channel),
            };
        }
        // Drawn from the store on the next frame; nothing else follows.
        Applied::Nothing
    }

    /// Someone left `channel`: once nobody is in it, its start time goes,
    /// so whoever comes next does not inherit it.
    pub(crate) fn voice_room_left(&mut self, channel: ChannelId) {
        if !self.voice.iter().any(|state| state.channel == channel) {
            self.voice_started.remove(&channel);
        }
    }

    /// Since when `channel` has had someone in it, while someone still is.
    pub fn voice_started(&self, channel: ChannelId) -> Option<Timestamp> {
        self.voice_started
            .get(&channel)
            .copied()
            .filter(|_| self.voice.iter().any(|state| state.channel == channel))
    }

    /// Since when each channel someone is in has had someone, when told.
    pub fn voice_started_all(&self) -> impl Iterator<Item = Timestamp> + '_ {
        self.voice_started
            .iter()
            .filter(|(channel, _)| self.voice.iter().any(|state| state.channel == **channel))
            .map(|(_, since)| *since)
    }
}
