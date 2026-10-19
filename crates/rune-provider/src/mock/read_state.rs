//! The simulation's authoritative read cursors, independent of local unread choices.

use super::{World, ME, MESSAGES_PER_CHANNEL};
use crate::{Event, Failure, RequestId};
use rune_i18n::t;
use rune_model::{ChannelId, MessageId, Permissions, ServerId, UserId};

pub(super) struct ReadCursor {
    pub(super) message: Option<MessageId>,
    pub(super) version: u64,
}

impl Default for ReadCursor {
    fn default() -> Self {
        Self {
            message: None,
            version: 1,
        }
    }
}

impl World {
    pub(super) fn confirm_read(
        &mut self,
        request: RequestId,
        account: UserId,
        channel: ChannelId,
        message: MessageId,
    ) -> Event {
        match self.advance_read(request, account, channel, message) {
            Ok((message, version)) => Event::ReadStateConfirmed {
                account,
                request: Some(request),
                channel,
                message,
                version,
            },
            Err(reason) => Event::Failed {
                failure: Failure::ReadState { request, channel },
                reason: reason.into(),
                ambiguous: false,
            },
        }
    }

    fn advance_read(
        &mut self,
        request: RequestId,
        account: UserId,
        channel: ChannelId,
        target: MessageId,
    ) -> Result<(MessageId, u64), &'static str> {
        if request == 0 || account != ME {
            return Err(t(
                "Esta solicitud de lectura no corresponde a la cuenta simulada.",
            ));
        }
        if !self.channel(channel).is_some_and(|channel| {
            channel.kind.has_messages()
                && channel
                    .permissions
                    .contains(Permissions::VIEW | Permissions::READ_HISTORY)
        }) {
            return Err(t("No se puede confirmar la lectura de esta conversación."));
        }
        // This history is contiguous. Deletion changes availability, never
        // whether an inclusive through boundary was issued in this channel.
        let issued = self.sims.get(&channel).is_some_and(|sim| {
            let index = channel
                .0
                .checked_mul(MESSAGES_PER_CHANNEL)
                .and_then(|base| target.0.checked_sub(base));
            let total = sim.generated.checked_add(sim.extra.len() as u64);
            index.zip(total).is_some_and(|(index, total)| index < total)
        });
        if target.0 == 0 || !issued {
            return Err(t(
                "El límite de lectura no corresponde a un mensaje emitido en esa conversación.",
            ));
        }
        let cursor = self.read.entry(channel).or_default();
        let confirmed = cursor.message.max(Some(target)).unwrap();
        if cursor.message != Some(confirmed) {
            let version = cursor
                .version
                .checked_add(1)
                .ok_or(t("La versión de lectura no admite otro avance."))?;
            cursor.message = Some(confirmed);
            cursor.version = version;
        }
        Ok((confirmed, cursor.version))
    }

    /// The cursor goes back to just before `message`, as the account's other
    /// devices are told; the reminder is the account's own.
    pub(super) fn mark_unread(
        &mut self,
        account: UserId,
        channel: ChannelId,
        message: MessageId,
        mentions: u32,
    ) -> Event {
        let readable = self.channel(channel).is_some_and(|found| {
            found.kind.has_messages()
                && found
                    .permissions
                    .contains(Permissions::VIEW | Permissions::READ_HISTORY)
        });
        let base = channel.0.checked_mul(MESSAGES_PER_CHANNEL);
        let index = base.and_then(|base| message.0.checked_sub(base));
        let total = self
            .sims
            .get(&channel)
            .map(|sim| sim.generated + sim.extra.len() as u64);
        let issued = index.zip(total).is_some_and(|(index, total)| index < total);
        if account != ME || !readable || !issued {
            return Event::Failed {
                failure: Failure::Unread { channel },
                reason: t("No se puede marcar como no leído ese mensaje.").into(),
                ambiguous: false,
            };
        }
        // Simulated ids follow each other: the one before is the cursor.
        let through = index
            .and_then(|index| index.checked_sub(1))
            .zip(base)
            .map(|(index, base)| MessageId(base + index));
        let cursor = self.read.entry(channel).or_default();
        cursor.message = through;
        cursor.version = cursor.version.saturating_add(1);
        Event::MarkedUnread {
            account,
            channel,
            through,
            anchor: Some(message),
            mentions,
            version: cursor.version,
        }
    }

    /// Every readable channel of `server` read through its newest message;
    /// each answers on its own, as nobody waits on the whole.
    pub(super) fn mark_server_read(&mut self, account: UserId, server: ServerId) -> Vec<Event> {
        if account != ME {
            return vec![Event::Failed {
                failure: Failure::ServerRead { server },
                reason: t("Esta solicitud de lectura no corresponde a la cuenta simulada.").into(),
                ambiguous: false,
            }];
        }
        let channels: Vec<ChannelId> = self
            .channels
            .iter()
            .filter(|channel| {
                channel.server == Some(server)
                    && channel.kind.has_messages()
                    && channel
                        .permissions
                        .contains(Permissions::VIEW | Permissions::READ_HISTORY)
            })
            .map(|channel| channel.id)
            .collect();
        let mut events = Vec::new();
        for channel in channels {
            let Some(total) = self
                .sims
                .get(&channel)
                .map(|sim| sim.generated + sim.extra.len() as u64)
                .filter(|total| *total > 0)
            else {
                continue;
            };
            let newest = MessageId(channel.0 * MESSAGES_PER_CHANNEL + total - 1);
            let cursor = self.read.entry(channel).or_default();
            if cursor.message >= Some(newest) {
                continue;
            }
            cursor.message = Some(newest);
            cursor.version = cursor.version.saturating_add(1);
            events.push(Event::ReadStateConfirmed {
                account,
                request: None,
                channel,
                message: newest,
                version: cursor.version,
            });
        }
        events
    }
}
