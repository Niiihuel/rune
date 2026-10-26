//! Stages: raising a hand, inviting to speak and stepping down, checked
//! against the session before the request goes. The routes are the ones the
//! official web client calls (REA of its public bundle, 2026-10-17; see
//! docs/DISCORD.md); Serein has no stages. The Gateway's
//! `VOICE_STATE_UPDATE` confirms every change.

use rune_i18n::es;
use rune_model::{ChannelId, ChannelKind, Permissions, ServerId, StageSeat, UserId};
use rune_provider::{Command, Snapshot};

use crate::LoginError;

/// The stage a command is about, its server, and where the account sits on it.
fn stage_of(
    snapshot: &Snapshot,
    channel: ChannelId,
) -> Result<(ServerId, Permissions, StageSeat, UserId), LoginError> {
    let found = snapshot
        .channels
        .iter()
        .find(|known| known.id == channel && known.kind == ChannelKind::Stage)
        .ok_or(LoginError::new(es("Ese escenario no está disponible.")))?;
    let server = found
        .server
        .ok_or(LoginError::new(es("Ese escenario no está disponible.")))?;
    let me = snapshot
        .me
        .as_ref()
        .map(|me| me.id)
        .ok_or(crate::wire::invalid())?;
    let seat = snapshot
        .voice
        .iter()
        .find(|state| state.user == me && state.channel == channel)
        .map(|state| state.stage_seat())
        .ok_or(LoginError::new(es("Entra al escenario primero.")))?;
    Ok((server, found.permissions, seat, me))
}

/// Whether a stage command may go now; Discord stays the last word.
pub(crate) fn allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    let refused = |why: &'static str| Err(LoginError::new(why));
    match command {
        Command::RaiseHand { channel, raised } => {
            let (_, permissions, seat, _) = stage_of(snapshot, *channel)?;
            if !*raised {
                return Ok(());
            }
            if !permissions.contains(Permissions::REQUEST_TO_SPEAK) {
                return refused(es(
                    "No tienes permiso para pedir la palabra en este escenario.",
                ));
            }
            if matches!(seat, StageSeat::Speaker | StageSeat::Invited) {
                return refused(es("Ya puedes hablar en este escenario."));
            }
            Ok(())
        }
        Command::SetSpeaker {
            channel,
            user,
            speaker,
        } => {
            let (_, permissions, seat, me) = stage_of(snapshot, *channel)?;
            let moderates = permissions.contains(Permissions::MUTE_MEMBERS);
            if *user == me {
                // Stepping down is always the account's; stepping up needs
                // an invitation or moderation.
                if *speaker && seat != StageSeat::Invited && !moderates {
                    return refused(es("Espera que la moderación te invite a hablar."));
                }
                return Ok(());
            }
            if !moderates {
                return refused(es("Solo la moderación invita a hablar o pasa al público."));
            }
            if !snapshot
                .voice
                .iter()
                .any(|state| state.user == *user && state.channel == *channel)
            {
                return refused(es("Esa persona ya no está en el escenario."));
            }
            Ok(())
        }
        _ => refused(es("Esta acción todavía no está disponible.")),
    }
}

/// The server and the account a stage command goes with, for the request.
pub(crate) fn target(snapshot: &Snapshot, command: &Command) -> Option<(ServerId, UserId)> {
    let (Command::RaiseHand { channel, .. } | Command::SetSpeaker { channel, .. }) = command else {
        return None;
    };
    stage_of(snapshot, *channel)
        .ok()
        .map(|(server, _, _, me)| (server, me))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{Channel, Timestamp, VoiceState};

    fn snapshot(permissions: Permissions, seat: StageSeat) -> Snapshot {
        let me = crate::wire::user(&serde_json::json!({"id": "9", "username": "me"})).unwrap();
        let stage: Channel = serde_json::from_value(serde_json::json!({
            "id": 30, "server": 10, "kind": "Stage", "name": "charla", "topic": null,
            "parent": null, "position": 0, "recipients": [], "permissions": permissions.0
        }))
        .unwrap();
        let state = |user: u64, seat: StageSeat| VoiceState {
            user: UserId(user),
            channel: ChannelId(30),
            muted: false,
            deafened: false,
            speaking: false,
            streaming: false,
            server_muted: false,
            server_deafened: false,
            camera: false,
            suppressed: matches!(seat, StageSeat::Audience | StageSeat::HandRaised),
            speak_request: matches!(seat, StageSeat::HandRaised | StageSeat::Invited)
                .then_some(Timestamp(1)),
        };
        Snapshot {
            me: Some(me),
            channels: vec![stage],
            voice: vec![state(9, seat), state(8, StageSeat::Speaker)],
            ..Default::default()
        }
    }

    #[test]
    fn hands_invitations_and_moderation_follow_the_seat_and_the_permissions() {
        let raise = |raised| Command::RaiseHand {
            channel: ChannelId(30),
            raised,
        };
        let speak = |user, speaker| Command::SetSpeaker {
            channel: ChannelId(30),
            user: UserId(user),
            speaker,
        };
        let audience = snapshot(
            Permissions::VIEW | Permissions::REQUEST_TO_SPEAK,
            StageSeat::Audience,
        );
        assert!(allowed(&audience, &raise(true)).is_ok());
        assert!(allowed(&audience, &speak(9, true)).is_err(), "not invited");
        assert!(allowed(&audience, &speak(9, false)).is_ok());
        assert!(
            allowed(&audience, &speak(8, false)).is_err(),
            "not moderation"
        );
        assert_eq!(
            target(&audience, &raise(false)),
            Some((ServerId(10), UserId(9)))
        );
        let silent = snapshot(Permissions::VIEW, StageSeat::Audience);
        assert!(allowed(&silent, &raise(true)).is_err());
        assert!(
            allowed(&silent, &raise(false)).is_ok(),
            "a hand always comes down"
        );
        let invited = snapshot(Permissions::VIEW, StageSeat::Invited);
        assert!(allowed(&invited, &speak(9, true)).is_ok());
        assert!(allowed(&invited, &raise(true)).is_err());
        let moderator = snapshot(
            Permissions::VIEW | Permissions::MUTE_MEMBERS,
            StageSeat::Speaker,
        );
        assert!(allowed(&moderator, &speak(8, false)).is_ok());
        assert!(
            allowed(&moderator, &speak(7, true)).is_err(),
            "not on the stage"
        );
        let mut away = audience.clone();
        away.voice.clear();
        assert!(allowed(&away, &raise(true)).is_err());
        assert_eq!(target(&away, &raise(true)), None);
    }
}
