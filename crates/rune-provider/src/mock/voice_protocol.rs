//! The simulation's voice protocol (voz-protocolo): the account's call alone
//! in its channel waits for the others, as the service's does, and the voice
//! rooms someone is in say since when, as the service answers op 43.
//! Deterministic: the start times are offsets from the clock by channel.

use rune_model::{CallPhase, ChannelId, ChannelKind, ServerId, Timestamp};

use super::{World, ME};
use crate::Event;

impl World {
    /// The account's call in `channel` as it is now: with others, or waiting
    /// for them.
    pub(super) fn call_phase(&self, channel: ChannelId) -> CallPhase {
        let others = self
            .voice
            .iter()
            .any(|state| state.channel == channel && state.user != ME);
        if others {
            CallPhase::Connected
        } else {
            CallPhase::Waiting
        }
    }

    /// The account's call reached `phase`: what [`Self::follow_call`]
    /// compares with.
    pub(super) fn call_shown(&mut self, channel: ChannelId, phase: &CallPhase) {
        self.voice_protocol.shown = Some((channel, phase.clone()));
    }

    /// Someone came to the account's call or left it: it waits for the
    /// others, or stops waiting. Nothing when it did not change.
    pub(super) fn follow_call(&mut self) -> Vec<Event> {
        let Some(channel) = self.my_call() else {
            self.voice_protocol.shown = None;
            return Vec::new();
        };
        let phase = self.call_phase(channel);
        if self.voice_protocol.shown.as_ref() == Some(&(channel, phase.clone())) {
            return Vec::new();
        }
        self.call_shown(channel, &phase);
        vec![Event::CallPhase {
            channel,
            phase,
            quality: None,
        }]
    }

    /// The servers' voice rooms someone is in now.
    fn occupied(&self) -> Vec<(ServerId, ChannelId)> {
        self.channels
            .iter()
            .filter(|channel| {
                channel.kind == ChannelKind::Voice
                    && self.voice.iter().any(|state| state.channel == channel.id)
            })
            .filter_map(|channel| Some((channel.server?, channel.id)))
            .collect()
    }

    /// Since when each server's voice rooms someone is in have had someone,
    /// as a session tells once it is ready.
    pub(super) fn voice_start_times(&mut self, now: Timestamp) -> Vec<Event> {
        let occupied = self.occupied();
        let events = self
            .servers
            .iter()
            .map(|server| Event::VoiceStartTimes {
                server: server.id,
                times: occupied
                    .iter()
                    .filter(|(at, _)| *at == server.id)
                    // Between 7 and 186 minutes ago, by the channel.
                    .map(|(_, channel)| {
                        let minutes = 7 + (channel.0 % 180) as i64;
                        (*channel, Some(Timestamp(now.0 - minutes * 60_000)))
                    })
                    .collect(),
                whole: true,
            })
            .collect();
        self.voice_protocol.rooms = Some(occupied);
        events
    }

    /// Someone came to a voice room nobody was in, or left one empty, by
    /// any way (the account, a call's people, moderation): it starts at
    /// `now`, or stops. Nothing until a session told them.
    pub(super) fn follow_rooms(&mut self, now: Timestamp) -> Vec<Event> {
        let Some(told) = self.voice_protocol.rooms.take() else {
            return Vec::new();
        };
        let occupied = self.occupied();
        let started = occupied
            .iter()
            .filter(|room| !told.contains(room))
            .map(|room| (*room, Some(now)));
        let stopped = told
            .iter()
            .filter(|room| !occupied.contains(room))
            .map(|room| (*room, None));
        let events = started
            .chain(stopped)
            .map(|((server, channel), since)| Event::VoiceStartTimes {
                server,
                times: vec![(channel, since)],
                whole: false,
            })
            .collect();
        self.voice_protocol.rooms = Some(occupied);
        events
    }
}

/// What the simulation last told of the account's call.
#[derive(Default)]
pub(super) struct VoiceProtocolSim {
    shown: Option<(ChannelId, CallPhase)>,
    /// The occupied voice rooms last told, once a session told them.
    rooms: Option<Vec<(ServerId, ChannelId)>>,
}

#[cfg(test)]
mod tests {
    use rune_model::{CallPhase, ChannelKind, Timestamp, UserId, VoiceState};

    use super::super::{MockConfig, World};
    use crate::Event;

    #[test]
    fn a_call_alone_waits_and_rooms_say_since_when_they_are_on() {
        let mut world = World::build(&MockConfig::default());
        world.voice.clear();
        let room = world
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Voice && channel.server.is_some())
            .map(|channel| channel.id)
            .expect("a voice room");
        let now = Timestamp(1_790_000_100_000);
        // Nothing before a session told them.
        assert!(world.follow_rooms(now).is_empty());
        assert!(world
            .voice_start_times(now)
            .iter()
            .all(|event| matches!(event, Event::VoiceStartTimes { times, whole: true, .. } if times.is_empty())));
        let joined = world.join_voice(room, false, false);
        assert!(joined.iter().any(|event| matches!(
            event,
            Event::CallPhase {
                phase: CallPhase::Waiting,
                ..
            }
        )));
        assert!(
            matches!(
                &world.follow_rooms(now)[..],
                [Event::VoiceStartTimes { times, whole: false, .. }] if times == &[(room, Some(now))]
            ),
            "the first one in starts it"
        );
        // Someone comes: the call stops waiting, once; the room was on.
        world.voice.push(VoiceState {
            user: UserId(77),
            channel: room,
            muted: false,
            deafened: false,
            speaking: false,
            streaming: false,
            server_muted: false,
            server_deafened: false,
            camera: false,
            suppressed: false,
            speak_request: None,
        });
        assert!(matches!(
            &world.follow_call()[..],
            [Event::CallPhase {
                phase: CallPhase::Connected,
                ..
            }]
        ));
        assert!(world.follow_call().is_empty());
        assert!(world.follow_rooms(now).is_empty());
        let told = world.voice_start_times(now);
        let times = told
            .iter()
            .find_map(|event| match event {
                Event::VoiceStartTimes {
                    times, whole: true, ..
                } if times.iter().any(|(channel, _)| *channel == room) => Some(times.clone()),
                _ => None,
            })
            .expect("the room's server tells it");
        let since = times
            .iter()
            .find(|(channel, _)| *channel == room)
            .unwrap()
            .1;
        assert!(since.is_some_and(|since| since.0 < now.0));
        // The account leaves, then the other is moved out by moderation: it stops.
        world.voice.retain(|state| state.user != super::super::ME);
        assert!(world.follow_rooms(now).is_empty());
        world.voice.clear();
        assert!(matches!(
            &world.follow_rooms(now)[..],
            [Event::VoiceStartTimes { times, whole: false, .. }] if times == &[(room, None)]
        ));
    }
}
