//! Request-scoped reconciliation of pins snapshots with confirmed live changes.

use std::collections::HashMap;

use rune_i18n::t;
use rune_model::{ChannelId, Message, MessageId, MessagePatch};
use rune_provider::{AuthenticationState, ConnectionState, Failure, RequestId};

use crate::{Applied, Permit, Store};

pub(super) const PIN_DELTAS: usize = 256;

#[derive(Debug, Default)]
pub(super) struct PinnedMessages {
    completed_request: Option<RequestId>,
    messages: Vec<Message>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PinState {
    Pinned,
    Unpinned,
    Deleted,
    Refreshed,
}

#[derive(Debug)]
pub(super) struct PinsRead {
    channel: ChannelId,
    request: RequestId,
    deltas: HashMap<MessageId, PinState>,
    invalid: Option<PinsInvalidation>,
}

#[derive(Debug)]
enum PinsInvalidation {
    DeltaOverflow,
    UnknownBaseline,
}

impl Store {
    /// Register a read before forwarding its command. Only one pins popout
    /// can be open, so a newer read replaces the previous request's scope.
    pub fn begin_pins(&mut self, channel: ChannelId, request: RequestId) -> Permit {
        self.prune_pins();
        let permit = self.pins_permit(channel);
        if !permit.allowed() {
            return permit;
        }
        if request == 0
            || self
                .pins_read
                .as_ref()
                .is_some_and(|read| read.request >= request)
            || self
                .pins
                .get(&channel)
                .and_then(|pins| pins.completed_request)
                .is_some_and(|completed| completed >= request)
        {
            return Permit::Denied(t("Esta solicitud de mensajes fijados ya venció").into());
        }
        if let Some(previous) = self.pins_read.as_ref().map(|read| read.request) {
            self.cancel_pins(previous);
        }
        self.pins.entry(channel).or_default();
        self.pins_read = Some(PinsRead {
            channel,
            request,
            deltas: HashMap::new(),
            invalid: None,
        });
        self.revision += 1;
        Permit::Allowed
    }

    fn pins_permit(&self, channel: ChannelId) -> Permit {
        let permit = self
            .abilities(channel)
            .map(|a| a.view_pins)
            .unwrap_or_else(|| Permit::Denied(t("Esta conversación ya no está disponible").into()));
        if !permit.allowed() {
            return permit;
        }
        if self.me().is_none()
            || self.from_cache
            || self.connection != ConnectionState::Connected
            || self
                .provider
                .as_ref()
                .is_some_and(|p| p.capabilities.authentication)
                && !matches!(self.authentication, AuthenticationState::SignedIn { .. })
        {
            return Permit::Denied(
                t("Espera a que la sesión esté disponible para consultar los mensajes fijados")
                    .into(),
            );
        }
        Permit::Allowed
    }

    pub(super) fn expects_pins(&self, channel: ChannelId, request: RequestId) -> bool {
        self.pins_read
            .as_ref()
            .is_some_and(|read| read.channel == channel && read.request == request)
    }

    /// Cancel only this read. A late cancellation cannot erase its replacement.
    pub fn cancel_pins(&mut self, request: RequestId) {
        if self
            .pins_read
            .as_ref()
            .is_some_and(|read| read.request == request)
        {
            let read = self.pins_read.take().unwrap();
            self.drop_provisional_pins(read.channel);
            self.revision += 1;
        }
    }

    /// The last completed pins list, newest first and kept current with live
    /// events. A provisional live baseline does not imply an empty result.
    pub fn pins(&self, channel: ChannelId) -> Option<&[Message]> {
        self.pins
            .get(&channel)
            .filter(|pins| pins.completed_request.is_some())
            .map(|pins| pins.messages.as_slice())
    }

    fn drop_provisional_pins(&mut self, channel: ChannelId) {
        if self
            .pins
            .get(&channel)
            .is_some_and(|pins| pins.completed_request.is_none())
        {
            self.pins.remove(&channel);
        }
    }

    pub(super) fn forget_pins(&mut self, channel: ChannelId) {
        self.pins.remove(&channel);
        if self
            .pins_read
            .as_ref()
            .is_some_and(|read| read.channel == channel)
        {
            self.pins_read = None;
        }
    }

    pub(super) fn prune_pins(&mut self) {
        if !self.provider.as_ref().is_some_and(|p| p.capabilities.pins) {
            self.pins.clear();
            self.pins_read = None;
        } else if let Some(channel) = self.pins_read.as_ref().map(|read| read.channel) {
            if !self
                .abilities(channel)
                .is_some_and(|a| a.view_pins.allowed())
            {
                self.forget_pins(channel);
            }
        }
    }

    fn pin_delta(&mut self, channel: ChannelId, id: MessageId, state: PinState) {
        let Some(read) = self
            .pins_read
            .as_mut()
            .filter(|read| read.channel == channel)
        else {
            return;
        };
        if read.invalid.is_some() || read.deltas.get(&id) == Some(&PinState::Deleted) {
            return;
        }
        if state == PinState::Refreshed && read.deltas.contains_key(&id) {
            return;
        }
        if !read.deltas.contains_key(&id) && read.deltas.len() >= PIN_DELTAS {
            read.invalid = Some(PinsInvalidation::DeltaOverflow);
            read.deltas.clear();
            self.drop_provisional_pins(channel);
            return;
        }
        read.deltas.insert(id, state);
    }

    pub(super) fn delete_pinned(&mut self, channel: ChannelId, id: MessageId) {
        self.pin_delta(channel, id, PinState::Deleted);
        if let Some(pins) = self.pins.get_mut(&channel) {
            pins.messages.retain(|m| m.id != id);
        }
    }

    pub(super) fn retained_pin(&self, channel: ChannelId, id: MessageId) -> Option<&Message> {
        self.pins
            .get(&channel)?
            .messages
            .iter()
            .find(|message| message.id == id)
    }

    fn confirmed_timeline(&self, channel: ChannelId, id: MessageId) -> Option<&Message> {
        self.before_change
            .get(&(channel, id))
            .or_else(|| {
                self.timelines
                    .get(&channel)
                    .and_then(|timeline| timeline.get(id))
            })
            .map(|message| message.as_ref())
    }

    pub(super) fn unknown_pin_patch(&mut self, patch: &MessagePatch) {
        if patch.author.is_none() && patch.pinned == Some(false) {
            self.pin_delta(patch.channel, patch.id, PinState::Unpinned);
        } else if patch.author.is_some()
            || patch.content.is_some()
            || patch.edited.is_some()
            || patch.reply_to.is_some()
            || patch.pinned.is_some()
            || patch.mentions.is_some()
            || patch.reactions.is_some()
            || patch.attachments.is_some()
            || patch.stickers.is_some()
            || patch.text_references.is_some()
        {
            if let Some(read) = self
                .pins_read
                .as_mut()
                .filter(|read| read.channel == patch.channel)
            {
                read.invalid
                    .get_or_insert(PinsInvalidation::UnknownBaseline);
                read.deltas.clear();
                self.drop_provisional_pins(patch.channel);
            }
        }
    }

    pub(super) fn patch_pin(&mut self, message: &Message, membership: bool) {
        if membership {
            self.pin_delta(
                message.channel,
                message.id,
                if message.pinned {
                    PinState::Pinned
                } else {
                    PinState::Unpinned
                },
            );
        } else {
            self.pin_delta(message.channel, message.id, PinState::Refreshed);
        }
        self.replace_pin(message);
    }

    fn replace_pin(&mut self, message: &Message) {
        let Some(pins) = self.pins.get_mut(&message.channel) else {
            return;
        };
        pins.messages.retain(|known| known.id != message.id);
        if message.pinned {
            let at = pins.messages.partition_point(|known| known.id > message.id);
            pins.messages.insert(at, message.clone());
        }
    }

    /// Keep the canonical live baseline; the delta retains only membership IDs.
    pub(super) fn keep_pinned(&mut self, message: &Message) {
        if self
            .timelines
            .get(&message.channel)
            .is_some_and(|timeline| timeline.is_deleted(message.id))
        {
            return;
        }
        self.pin_delta(
            message.channel,
            message.id,
            if message.pinned {
                PinState::Pinned
            } else {
                PinState::Unpinned
            },
        );
        let Some(pins) = self.pins.get(&message.channel) else {
            return;
        };
        let latest = pins
            .messages
            .iter()
            .find(|known| known.id == message.id)
            .into_iter()
            .chain(self.confirmed_timeline(message.channel, message.id))
            .max_by_key(|known| known.edited);
        let mut current = latest
            .filter(|known| known.edited > message.edited)
            .unwrap_or(message)
            .clone();
        current.pinned = message.pinned;
        self.replace_pin(&current);
    }

    pub(super) fn loaded_pins(
        &mut self,
        channel: ChannelId,
        request: RequestId,
        messages: Vec<Message>,
    ) -> Applied {
        if !self.expects_pins(channel, request) {
            return Applied::Nothing;
        }
        let permit = self.pins_permit(channel);
        let read = self.pins_read.take().unwrap();
        let failure = |reason: &str| Applied::Failed {
            failure: Failure::Pins { channel, request },
            reason: reason.into(),
            ambiguous: false,
        };
        if let Some(reason) = permit.reason() {
            self.drop_provisional_pins(channel);
            return failure(reason);
        }
        let reject = if let Some(invalid) = read.invalid {
            Some(match invalid {
                PinsInvalidation::DeltaOverflow => t("Los mensajes fijados cambiaron demasiado durante la carga. Vuelve a intentarlo."),
                PinsInvalidation::UnknownBaseline => t("No pudimos verificar un cambio porque no conservamos el mensaje original. Vuelve a intentarlo."),
            })
        } else if messages
            .iter()
            .any(|message| message.channel != channel || message.id.0 == 0 || !message.pinned)
        {
            Some(t(
                "La lista de mensajes fijados recibida no es válida. Vuelve a intentarlo.",
            ))
        } else {
            None
        };
        if let Some(reason) = reject {
            self.drop_provisional_pins(channel);
            return failure(reason);
        }
        let baseline = self.pins.get(&channel);
        let mut merged = HashMap::<MessageId, Message>::new();
        for mut message in messages {
            if self
                .timelines
                .get(&channel)
                .is_some_and(|timeline| timeline.is_deleted(message.id))
            {
                continue;
            }
            if let Some(current) = baseline
                .and_then(|pins| pins.messages.iter().find(|known| known.id == message.id))
                .into_iter()
                .chain(self.confirmed_timeline(channel, message.id))
                .max_by_key(|current| current.edited)
                .filter(|current| current.edited > message.edited)
            {
                message = current.clone();
                message.pinned = true;
            }
            if merged
                .get(&message.id)
                .is_none_or(|known| known.edited <= message.edited)
            {
                merged.insert(message.id, message);
            }
        }
        for (id, state) in read.deltas {
            match state {
                PinState::Pinned | PinState::Refreshed => {
                    if state == PinState::Refreshed && !merged.contains_key(&id) {
                        continue;
                    }
                    let Some(current) = baseline.and_then(|pins| {
                        pins.messages
                            .iter()
                            .find(|known| known.id == id && known.pinned)
                    }) else {
                        self.drop_provisional_pins(channel);
                        return failure(t("La lista de mensajes fijados cambió durante la carga. Vuelve a intentarlo."));
                    };
                    if merged
                        .get(&id)
                        .is_none_or(|held| held.edited <= current.edited)
                    {
                        merged.insert(id, current.clone());
                    }
                }
                PinState::Unpinned | PinState::Deleted => {
                    merged.remove(&id);
                }
            }
        }
        let mut messages: Vec<_> = merged.into_values().collect();
        messages.sort_by_key(|message| std::cmp::Reverse(message.id));
        self.pins.insert(
            channel,
            PinnedMessages {
                completed_request: Some(request),
                messages,
            },
        );
        Applied::Pins { channel, request }
    }
}
