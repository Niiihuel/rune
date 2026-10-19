//! Exact, bounded provider context reads, independent of history pagination.

use std::time::Instant;

use rune_i18n::t;
use rune_model::{ChannelId, MessageId, Permissions};
use rune_provider::{AuthenticationState, Command, ConnectionState, Event, Failure, RequestId};
use rune_storage::MessageMutation;

use crate::Client;

#[derive(Clone, Copy, Debug)]
pub(super) struct LoadingContext {
    pub channel: ChannelId,
    pub request: RequestId,
    pub anchor: MessageId,
    pub limit: usize,
    pub since: Instant,
    pub dirty: bool,
}

impl Client {
    pub(super) fn fail_context(&mut self, reason: &str) {
        if let Some(context) = self.context.take() {
            // Cancellation is best effort; correlation is invalidated locally.
            let _ = self.commands.try_send(Command::Cancel(context.request));
            self.found.push_back(Event::Failed {
                failure: Failure::Context {
                    request: context.request,
                    channel: context.channel,
                },
                reason: reason.into(),
                ambiguous: false,
            });
        }
    }

    pub(super) fn accept_context(
        &mut self,
        event: &Event,
        save: &mut Vec<MessageMutation>,
    ) -> bool {
        let changed_channel = match event {
            Event::MessageCreated { message, .. } | Event::MessageUpdated(message) => {
                Some(message.channel)
            }
            Event::MessagePatched(patch) => Some(patch.channel),
            Event::MessageDeleted { channel, .. } => Some(*channel),
            // A live reaction or vote changes a neighbor the window may hold.
            Event::ReactionChanged { channel, .. }
            | Event::ReactionsCleared { channel, .. }
            | Event::PollVoteChanged { channel, .. }
            | Event::BurstReactionChanged { channel, .. } => Some(*channel),
            Event::HistoryLoaded {
                request, channel, ..
            } if self.live.contains(request) => Some(*channel),
            _ => None,
        };
        if let Some(scope) = self
            .context
            .as_mut()
            .filter(|scope| Some(scope.channel) == changed_channel)
        {
            scope.dirty = true;
        }
        match event {
            Event::ContextLoaded {
                request,
                channel,
                anchor,
                messages,
            } => {
                let Some(scope) = self
                    .context
                    .filter(|scope| scope.request == *request && scope.channel == *channel)
                else {
                    return false;
                };
                if scope.dirty {
                    self.fail_context(t(
                        "Los mensajes cambiaron durante la carga del contexto. Volvé a intentarlo.",
                    ));
                    return false;
                }
                if scope.anchor != *anchor
                    || messages.is_empty()
                    || messages.len() > scope.limit
                    || !messages.iter().any(|message| message.id == *anchor)
                    || messages.iter().any(|message| {
                        message.channel != *channel
                            || message.id.0 == 0
                            || !matches!(message.delivery, rune_model::Delivery::Sent)
                    })
                    || messages.windows(2).any(|pair| pair[0].id >= pair[1].id)
                {
                    self.fail_context(t(
                        "La respuesta de contexto no corresponde al mensaje solicitado.",
                    ));
                    return false;
                }
                self.context = None;
                if self.storage_active {
                    save.extend(
                        messages
                            .iter()
                            .cloned()
                            .map(|message| MessageMutation::Upsert(Box::new(message))),
                    );
                }
            }
            Event::Failed {
                failure: Failure::Context { request, channel },
                ..
            } => {
                if !self
                    .context
                    .is_some_and(|scope| scope.request == *request && scope.channel == *channel)
                {
                    return false;
                }
                self.context = None;
            }
            Event::Ready(_)
            | Event::Authentication(
                AuthenticationState::SignedOut | AuthenticationState::Failed { .. },
            ) => self.context = None,
            Event::Connection(
                ConnectionState::Connecting
                | ConnectionState::Disconnected { .. }
                | ConnectionState::Reconnecting { .. },
            ) => {
                self.fail_context(t("La conexión cambió antes de abrir el mensaje."));
            }
            Event::MessageDeleted { channel, message }
                if self
                    .context
                    .is_some_and(|scope| scope.channel == *channel && scope.anchor == *message) =>
            {
                self.fail_context(t("El mensaje ya no está disponible en este contexto."));
            }
            Event::ChannelRemoved { channel }
                if self.context.is_some_and(|scope| scope.channel == *channel) =>
            {
                self.fail_context(t("La conversación ya no está disponible."));
            }
            Event::ChannelChanged(channel)
                if self
                    .context
                    .is_some_and(|scope| scope.channel == channel.id)
                    && (!channel.kind.has_messages()
                        || !channel
                            .permissions
                            .contains(Permissions::VIEW | Permissions::READ_HISTORY)) =>
            {
                self.fail_context(t("Ya no se puede leer el historial de esta conversación."));
            }
            _ => {}
        }
        true
    }
}
