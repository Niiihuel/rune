//! Message actions (P4), checked against what the session knows before they
//! leave: forwards, polls, who reacted and who voted, and conversations a
//! link names. Discord stays the last word. Voice moderation is checked
//! with the rest of the administration (`admin`).

use rune_i18n::es;
use rune_model::{Channel, ChannelId, Permissions};
use rune_provider::{Command, Snapshot};

use crate::{wire, LoginError};

/// How many people a page of who reacted or voted asks for: Discord's most.
pub(crate) const PEOPLE_PAGE: usize = 100;

/// Whether `command` is one of the message actions this module checks.
/// Ending a poll also needs the poll loaded and the account's: see `messages`.
pub(crate) fn is_action(command: &Command) -> bool {
    matches!(
        command,
        Command::ForwardMessage { .. }
            | Command::CreatePoll { .. }
            | Command::EndPoll { .. }
            | Command::LoadReactions { .. }
            | Command::LoadPollVoters { .. }
            | Command::LoadChannel { .. }
    )
}

/// Whether `command` only reads: its failure is never ambiguous.
pub(crate) fn is_read(command: &Command) -> bool {
    matches!(
        command,
        Command::LoadReactions { .. }
            | Command::LoadPollVoters { .. }
            | Command::LoadChannel { .. }
    )
}

fn known(snapshot: &Snapshot, id: ChannelId) -> Result<&Channel, LoginError> {
    snapshot
        .channels
        .iter()
        .find(|known| known.id == id)
        .ok_or(LoginError::new(es(
            "Esa conversación ya no está disponible.",
        )))
}

/// What the account may do in `channel`, beyond what is always allowed
/// in a direct conversation.
fn needs(channel: &Channel, needed: Permissions) -> Result<(), LoginError> {
    if channel.permissions.contains(needed) {
        Ok(())
    } else {
        Err(LoginError::new(es(
            "No tienes permiso para esta acción en esa conversación.",
        )))
    }
}

/// The bit that lets the account write in `channel`.
fn send(channel: &Channel) -> Permissions {
    if channel.is_thread() {
        Permissions::SEND_IN_THREADS
    } else {
        Permissions::SEND
    }
}

pub(crate) fn allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    match command {
        Command::ForwardMessage {
            channel,
            message,
            to,
            ..
        } => {
            if message.0 == 0 || channel == to {
                return Err(wire::invalid());
            }
            needs(
                known(snapshot, *channel)?,
                Permissions::VIEW | Permissions::READ_HISTORY,
            )?;
            let found = known(snapshot, *to)?;
            needs(found, Permissions::VIEW | send(found))?;
            crate::messages::unrestricted(snapshot, *to)
        }
        Command::CreatePoll { channel, poll, .. } => {
            if poll.problem().is_some() {
                return Err(LoginError::new(es("La encuesta no está completa.")));
            }
            let found = known(snapshot, *channel)?;
            // Outside servers anyone of the conversation starts polls.
            let polls = if found.server.is_some() {
                Permissions::SEND_POLLS
            } else {
                Permissions::NONE
            };
            needs(found, Permissions::VIEW | send(found) | polls)?;
            crate::messages::unrestricted(snapshot, *channel)
        }
        Command::EndPoll { channel, .. } => needs(known(snapshot, *channel)?, Permissions::VIEW),
        Command::LoadReactions { channel, emoji, .. } => {
            crate::messages::validate_emoji(emoji)?;
            needs(
                known(snapshot, *channel)?,
                Permissions::VIEW | Permissions::READ_HISTORY,
            )
        }
        Command::LoadPollVoters { channel, .. } => needs(
            known(snapshot, *channel)?,
            Permissions::VIEW | Permissions::READ_HISTORY,
        ),
        // Whether the account may see it is Discord's to say.
        Command::LoadChannel { channel, .. } if channel.0 != 0 => Ok(()),
        _ => Err(wire::invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{ChannelKind, MessageId, ServerId};
    use rune_provider::{NewPoll, NewPollAnswer};

    fn channel(id: u64, server: Option<u64>, permissions: Permissions) -> Channel {
        Channel {
            id: ChannelId(id),
            server: server.map(ServerId),
            kind: if server.is_some() {
                ChannelKind::Text
            } else {
                ChannelKind::Direct
            },
            name: "c".into(),
            topic: None,
            topic_pictures: Vec::new(),
            parent: None,
            position: 0,
            recipients: Vec::new(),
            owner: None,
            permissions,
            overwrites: Vec::new(),
            slowmode_secs: 0,
            nsfw: false,
            icon: None,
            thread: None,
            tags: Vec::new(),
            forum: Default::default(),
        }
    }

    fn snapshot() -> Snapshot {
        let reader = Permissions::VIEW | Permissions::READ_HISTORY;
        Snapshot {
            channels: vec![
                channel(1, Some(9), reader | Permissions::SEND),
                channel(2, Some(9), reader),
                channel(3, None, reader | Permissions::SEND),
            ],
            ..Default::default()
        }
    }

    fn poll() -> NewPoll {
        let answer = |text: &str| NewPollAnswer {
            text: text.into(),
            emoji: None,
        };
        NewPoll {
            question: "¿?".into(),
            answers: vec![answer("a"), answer("b")],
            hours: 1,
            multiselect: false,
        }
    }

    #[test]
    fn forwards_need_reading_here_and_writing_there() {
        let forward = |from, to| Command::ForwardMessage {
            channel: ChannelId(from),
            message: MessageId(5),
            to: ChannelId(to),
            nonce: 1,
        };
        assert!(allowed(&snapshot(), &forward(2, 1)).is_ok());
        assert!(allowed(&snapshot(), &forward(2, 3)).is_ok());
        assert!(
            allowed(&snapshot(), &forward(1, 2)).is_err(),
            "cannot write"
        );
        assert!(allowed(&snapshot(), &forward(1, 1)).is_err(), "same place");
        assert!(allowed(&snapshot(), &forward(1, 77)).is_err(), "unknown");
    }

    #[test]
    fn polls_need_their_bit_only_in_servers() {
        let create = |channel, poll| Command::CreatePoll {
            channel: ChannelId(channel),
            poll,
            nonce: 1,
        };
        assert!(allowed(&snapshot(), &create(1, poll())).is_err());
        assert!(allowed(&snapshot(), &create(3, poll())).is_ok());
        let mut open = snapshot();
        open.channels[0].permissions = open.channels[0].permissions | Permissions::SEND_POLLS;
        assert!(allowed(&open, &create(1, poll())).is_ok());
        let mut empty = poll();
        empty.question.clear();
        assert!(allowed(&open, &create(1, empty)).is_err());
    }

    #[test]
    fn lists_of_people_need_the_history() {
        let mut shut = snapshot();
        shut.channels[1].permissions = Permissions::VIEW;
        let reactions = Command::LoadReactions {
            request: 1,
            channel: ChannelId(2),
            message: MessageId(5),
            emoji: "👍".into(),
            after: None,
        };
        assert!(allowed(&snapshot(), &reactions).is_ok());
        assert!(allowed(&shut, &reactions).is_err());
        assert!(allowed(
            &snapshot(),
            &Command::LoadChannel {
                request: 1,
                channel: ChannelId(0),
            }
        )
        .is_err());
    }
}
