//! The simulation's stage: two speakers and an audience, one of them with a
//! hand raised. The account joins as audience, raises its hand, and the
//! made-up moderation invites it to speak a moment later; accepting makes it
//! a speaker. Moderating others needs to mute members there, as a service
//! asks. Nothing of this carries sound.

use rune_i18n::t;
use rune_model::{ChannelId, ChannelKind, Permissions, StageSeat, Timestamp, UserId, VoiceState};

use super::{World, ME};
use crate::{Command, Event};

type Outcome = Result<Vec<Event>, String>;

impl World {
    /// Seat a few members of the stage's server on it: two speakers and three
    /// in the audience, the last with a hand raised.
    pub(super) fn add_stage_people(&mut self, stage: ChannelId) {
        let Some(server) = self.channel(stage).and_then(|found| found.server) else {
            return;
        };
        let busy: Vec<UserId> = self.voice.iter().map(|state| state.user).collect();
        let people: Vec<UserId> = self
            .members
            .iter()
            .filter(|member| member.server == server && member.user != ME)
            .map(|member| member.user)
            .filter(|user| {
                !busy.contains(user)
                    && !self
                        .users
                        .iter()
                        .any(|known| known.id == *user && known.bot)
            })
            .take(5)
            .collect();
        for (index, user) in people.into_iter().enumerate() {
            let audience = index >= 2;
            self.voice.push(VoiceState {
                user,
                channel: stage,
                muted: false,
                deafened: false,
                speaking: false,
                streaming: false,
                server_muted: audience,
                server_deafened: false,
                camera: false,
                suppressed: audience,
                speak_request: (index == 4).then(|| Timestamp(self.history_end.0 - 60_000)),
            });
        }
    }

    fn is_stage(&self, channel: ChannelId) -> bool {
        self.channel(channel)
            .is_some_and(|found| found.kind == ChannelKind::Stage)
    }

    /// The account joins a stage in its audience.
    pub(super) fn seat_on_join(&self, state: &mut VoiceState) {
        if self.is_stage(state.channel) {
            state.suppressed = true;
            state.server_muted = true;
        }
    }

    /// Change `user`'s place on `channel` with `change`, and report it.
    fn seat(
        &mut self,
        channel: ChannelId,
        user: UserId,
        change: impl FnOnce(&mut VoiceState),
    ) -> Outcome {
        let state = self
            .voice
            .iter_mut()
            .find(|state| state.user == user && state.channel == channel)
            .ok_or(t("no está en el escenario"))?;
        change(state);
        // Held until a speaker, as a service holds the microphone.
        state.server_muted = state.stage_seat() != StageSeat::Speaker;
        Ok(vec![Event::VoiceChanged(state.clone())])
    }

    /// Raise or lower a hand, or move someone between speakers and audience.
    pub(super) fn stage_command(&mut self, command: Command) -> Outcome {
        let channel = match &command {
            Command::RaiseHand { channel, .. } | Command::SetSpeaker { channel, .. } => *channel,
            _ => return Err(t("la simulación no sabe hacer eso").into()),
        };
        let found = self
            .channel(channel)
            .filter(|found| found.kind == ChannelKind::Stage)
            .ok_or(t("el escenario no existe"))?;
        let moderates = found.permissions.contains(Permissions::MUTE_MEMBERS);
        let may_ask = found.permissions.contains(Permissions::REQUEST_TO_SPEAK);
        let now = self.now;
        let mine = self
            .voice
            .iter()
            .find(|state| state.user == ME && state.channel == channel)
            .map(VoiceState::stage_seat)
            .ok_or(t("no estás en el escenario"))?;
        match command {
            Command::RaiseHand { raised, .. } => {
                if raised && !may_ask {
                    return Err(t("no tienes permiso para pedir la palabra").into());
                }
                if matches!(mine, StageSeat::Speaker | StageSeat::Invited) {
                    return Err(t("ya puedes hablar").into());
                }
                self.seat(channel, ME, |state| {
                    state.speak_request = raised.then_some(now);
                })
            }
            Command::SetSpeaker {
                user,
                speaker: true,
                ..
            } if user == ME => {
                if mine != StageSeat::Invited && !moderates {
                    return Err(t("espera que la moderación te invite a hablar").into());
                }
                self.seat(channel, ME, |state| {
                    state.suppressed = false;
                    state.speak_request = None;
                })
            }
            Command::SetSpeaker {
                user,
                speaker: false,
                ..
            } if user == ME => self.seat(channel, ME, |state| {
                state.suppressed = true;
                state.speak_request = None;
            }),
            Command::SetSpeaker { user, speaker, .. } => {
                if !moderates {
                    return Err(t("solo la moderación invita a hablar").into());
                }
                // An invitation waits for them to accept it.
                self.seat(channel, user, |state| {
                    state.suppressed = !speaker;
                    state.speak_request = speaker.then_some(now);
                })
            }
            _ => Err(t("la simulación no sabe hacer eso").into()),
        }
    }

    /// The made-up moderation invites the account to speak, if its hand is
    /// still raised on `channel`.
    pub(super) fn simulated_invitation(&mut self, channel: ChannelId, at: Timestamp) -> Vec<Event> {
        let raised = self.voice.iter().any(|state| {
            state.user == ME
                && state.channel == channel
                && state.stage_seat() == StageSeat::HandRaised
        });
        if !raised {
            return Vec::new();
        }
        self.seat(channel, ME, |state| {
            state.suppressed = false;
            state.speak_request = Some(at);
        })
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use crate::mock::{MockConfig, MockProvider, ME};
    use crate::{Command, Event};
    use rune_model::{ChannelKind, Permissions, StageSeat, Timestamp};

    #[test]
    fn the_account_joins_the_audience_raises_its_hand_and_speaks_when_invited() {
        let provider = MockProvider::new(MockConfig {
            history_end: Timestamp(1_790_000_000_000),
            ..MockConfig::default()
        });
        let snapshot = provider.snapshot();
        let stage = snapshot
            .channels
            .iter()
            .find(|c| c.kind == ChannelKind::Stage)
            .unwrap()
            .id;
        let seats: Vec<StageSeat> = snapshot
            .voice
            .iter()
            .filter(|state| state.channel == stage)
            .map(|state| state.stage_seat())
            .collect();
        assert_eq!(
            seats,
            vec![
                StageSeat::Speaker,
                StageSeat::Speaker,
                StageSeat::Audience,
                StageSeat::Audience,
                StageSeat::HandRaised
            ]
        );
        let mut world = provider.world.lock().unwrap();
        let joined = world.join_voice(stage, false, false);
        let Some(Event::VoiceChanged(mine)) = joined
            .iter()
            .find(|e| matches!(e, Event::VoiceChanged(s) if s.user == ME))
        else {
            panic!("{joined:?}");
        };
        assert_eq!(mine.stage_seat(), StageSeat::Audience);
        assert!(mine.server_muted);
        // Not invited yet, it cannot step up; nor move others.
        let speak = |world: &mut crate::mock::World, user, speaker| {
            world.stage_command(Command::SetSpeaker {
                channel: stage,
                user,
                speaker,
            })
        };
        assert!(speak(&mut world, ME, true).is_err());
        let other = snapshot
            .voice
            .iter()
            .find(|s| s.channel == stage)
            .unwrap()
            .user;
        assert!(speak(&mut world, other, false).is_err());
        let raised = world
            .stage_command(Command::RaiseHand {
                channel: stage,
                raised: true,
            })
            .unwrap();
        assert!(
            matches!(&raised[..], [Event::VoiceChanged(s)] if s.stage_seat() == StageSeat::HandRaised)
        );
        let invited = world.simulated_invitation(stage, Timestamp(1_790_000_100_000));
        assert!(matches!(&invited[..], [Event::VoiceChanged(s)]
            if s.stage_seat() == StageSeat::Invited && s.server_muted));
        let spoke = speak(&mut world, ME, true).unwrap();
        assert!(matches!(&spoke[..], [Event::VoiceChanged(s)]
            if s.stage_seat() == StageSeat::Speaker && !s.server_muted));
        // Stepping down is always the account's to do.
        assert!(matches!(&speak(&mut world, ME, false).unwrap()[..],
            [Event::VoiceChanged(s)] if s.stage_seat() == StageSeat::Audience));
        // Moderation moves others.
        if let Some(found) = world.channels.iter_mut().find(|c| c.id == stage) {
            found.permissions = found.permissions | Permissions::MUTE_MEMBERS;
        }
        assert!(matches!(&speak(&mut world, other, false).unwrap()[..],
            [Event::VoiceChanged(s)] if s.stage_seat() == StageSeat::Audience));
        assert!(matches!(&speak(&mut world, other, true).unwrap()[..],
            [Event::VoiceChanged(s)] if s.stage_seat() == StageSeat::Invited));
    }
}
