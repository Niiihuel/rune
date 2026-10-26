//! Stages: where the account sits on the stage of its call, what it may do
//! there (raise its hand, speak when invited, step down) and what moderation
//! may do to others (invite them to speak, send them to the audience). The
//! seat comes from the account's voice state as the provider reports it;
//! nothing here moves anyone before the provider says so.

use rune_i18n::{t, tr};
use rune_model::{ChannelId, ChannelKind, Permissions, StageSeat, UserId};

use crate::{Abilities, Permit, Store};

/// What the account may do on the stage of its call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageAbilities {
    /// Raise its hand to speak, or lower it.
    pub raise_hand: Permit,
    /// Speak: accept an invitation, or step up as moderation.
    pub speak: Permit,
    /// Go back to the audience.
    pub step_down: Permit,
    /// Invite others to speak, or send them to the audience.
    pub moderate: Permit,
}

impl Store {
    /// Where `user` sits on the stage `channel`, if they are on it.
    pub fn stage_seat(&self, channel: ChannelId, user: UserId) -> Option<StageSeat> {
        self.channel(channel)
            .filter(|found| found.kind == ChannelKind::Stage)?;
        self.voice_in(channel)
            .into_iter()
            .find(|state| state.user == user)
            .map(|state| state.stage_seat())
    }

    /// Why the account is not heard on the stage `channel`, if it is on it
    /// and not speaking.
    pub fn stage_hold(&self, channel: ChannelId) -> Option<&'static str> {
        match self.stage_seat(channel, self.me()?)? {
            StageSeat::Audience => Some(t(
                "Estás en el público: levanta la mano para pedir la palabra",
            )),
            StageSeat::HandRaised => Some(t(
                "Levantaste la mano: espera que la moderación te invite a hablar",
            )),
            StageSeat::Invited => Some(t("Te invitaron a hablar: acepta para que te escuchen")),
            StageSeat::Speaker => None,
        }
    }

    /// A stage holds the account's voice until it is a speaker there.
    pub(crate) fn hold_stage(&self, channel: ChannelId, abilities: &mut Abilities) {
        if let Some(reason) = self.stage_hold(channel) {
            abilities.speak = Permit::Denied(reason.into());
        }
    }

    /// What the account may do on the stage `channel`; `None` when it is not
    /// on that stage.
    pub fn stage_abilities(&self, channel: ChannelId) -> Option<StageAbilities> {
        let found = self.channel(channel)?;
        let seat = self.stage_seat(channel, self.me()?)?;
        let capabilities = self.capabilities();
        let moderates = found.permissions.contains(Permissions::MUTE_MEMBERS);
        let unavailable = self.offline().or_else(|| self.held(found.server));
        let gate = |may: bool, why: &str| {
            if !capabilities.stages {
                Permit::Denied(t("Este proveedor no admite canales de escenario").into())
            } else if !may {
                Permit::Denied(why.to_owned())
            } else if let Some(denied) = &unavailable {
                denied.clone()
            } else {
                Permit::Allowed
            }
        };
        let speaking = matches!(seat, StageSeat::Speaker);
        Some(StageAbilities {
            raise_hand: if matches!(seat, StageSeat::Speaker | StageSeat::Invited) {
                Permit::Denied(t("Ya puedes hablar").into())
            } else {
                gate(
                    found.permissions.contains(Permissions::REQUEST_TO_SPEAK),
                    &tr!(
                        "No tienes permiso para pedir la palabra en #{channel}",
                        channel = found.name
                    ),
                )
            },
            speak: if speaking {
                Permit::Denied(t("Ya estás hablando").into())
            } else {
                gate(
                    seat == StageSeat::Invited || moderates,
                    t("Levanta la mano y espera que la moderación te invite"),
                )
            },
            // Stepping down is always the account's to do.
            step_down: if matches!(seat, StageSeat::Audience | StageSeat::HandRaised) {
                Permit::Denied(t("Ya estás en el público").into())
            } else {
                gate(true, "")
            },
            moderate: gate(moderates, t("Solo la moderación invita a hablar")),
        })
    }
}
