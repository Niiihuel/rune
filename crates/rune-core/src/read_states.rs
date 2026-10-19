//! Bounded, session-bound read acknowledgements. A timeout leaves the remote
//! effect unknown; it never rolls back the account-local reading choice.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use rune_i18n::t;
use rune_model::{ChannelId, MessageId, Permissions, ServerId, UserId};
use rune_provider::{AuthenticationState, Command, ConnectionState, Event, Failure, RequestId};

const MAX_PENDING: usize = 256;

#[derive(Clone, Copy)]
pub(super) struct PendingRead {
    account: UserId,
    channel: ChannelId,
    request: RequestId,
    message: MessageId,
    since: Instant,
}

#[derive(Default)]
pub(super) struct ReadReceipts {
    account: Option<UserId>,
    connected: bool,
    readable: HashMap<ChannelId, Option<ServerId>>,
    pending: HashMap<ChannelId, PendingRead>,
}

impl ReadReceipts {
    pub fn prepare(
        &self,
        command: &Command,
        supported: bool,
    ) -> Result<Option<PendingRead>, &'static str> {
        let Command::MarkRead {
            request,
            account,
            channel,
            message,
        } = command
        else {
            return Ok(None);
        };
        if self.pending.values().any(|scope| scope.request == *request) {
            // A conflicting replay must not fail the already admitted intent.
            return Ok(None);
        }
        if !supported {
            return Err(t("El proveedor no admite confirmaciones de lectura."));
        }
        if !self.connected || self.account != Some(*account) || !self.readable.contains_key(channel)
        {
            return Err(t(
                "La cuenta o la conversación ya no está disponible para confirmar la lectura.",
            ));
        }
        if *request == 0 || message.0 == 0 {
            return Err(t("La confirmación de lectura no tiene un destino válido."));
        }
        if self.pending.len() >= MAX_PENDING && !self.pending.contains_key(channel) {
            return Err(t("Hay demasiadas confirmaciones de lectura pendientes."));
        }
        Ok(Some(PendingRead {
            account: *account,
            channel: *channel,
            request: *request,
            message: *message,
            since: Instant::now(),
        }))
    }

    pub fn admitted(&mut self, scope: PendingRead) {
        self.pending.insert(scope.channel, scope);
    }

    pub fn cancel(&mut self, request: RequestId, found: &mut VecDeque<Event>) {
        let channel = self
            .pending
            .iter()
            .find_map(|(channel, scope)| (scope.request == request).then_some(*channel));
        if let Some(scope) = channel.and_then(|channel| self.pending.remove(&channel)) {
            found.push_back(failed(
                scope,
                t("Se canceló la espera de la confirmación de lectura."),
            ));
        }
    }

    pub fn end_session(&mut self, found: &mut VecDeque<Event>) {
        self.fail_all(found, t("La sesión cambió antes de confirmar la lectura."));
        self.account = None;
        self.connected = false;
        self.readable.clear();
    }

    fn fail_all(&mut self, found: &mut VecDeque<Event>, reason: &str) {
        found.extend(self.pending.drain().map(|(_, scope)| failed(scope, reason)));
    }

    pub fn observe(&mut self, event: &Event, supported: bool, found: &mut VecDeque<Event>) -> bool {
        match event {
            Event::Ready(snapshot) => {
                self.pending.clear();
                self.account = snapshot.me.as_ref().map(|me| me.id);
                self.connected = self.account.is_some();
                self.readable = snapshot
                    .channels
                    .iter()
                    .filter(|channel| readable(channel))
                    .map(|channel| (channel.id, channel.server))
                    .collect();
            }
            Event::Authentication(
                AuthenticationState::SignedOut | AuthenticationState::Failed { .. },
            ) => {
                self.pending.clear();
                self.account = None;
                self.connected = false;
                self.readable.clear();
            }
            Event::Connection(ConnectionState::Connected) => {
                self.connected = self.account.is_some()
            }
            Event::Connection(_) => {
                self.connected = false;
                self.fail_all(
                    found,
                    t("Se perdió la conexión antes de confirmar la lectura."),
                );
            }
            Event::ChannelChanged(channel) => {
                if readable(channel) {
                    self.readable.insert(channel.id, channel.server);
                } else {
                    self.revoke(channel.id, found);
                }
            }
            Event::ChannelRemoved { channel } => self.revoke(*channel, found),
            Event::ServerRemoved { server, .. } => {
                let channels: Vec<_> = self
                    .readable
                    .iter()
                    .filter_map(|(channel, owner)| (*owner == Some(*server)).then_some(*channel))
                    .collect();
                for channel in channels {
                    self.revoke(channel, found);
                }
            }
            Event::ReadStateConfirmed {
                account,
                request,
                channel,
                message,
                version,
            } => {
                if !supported
                    || !self.connected
                    || self.account != Some(*account)
                    || !self.readable.contains_key(channel)
                    || message.0 == 0
                    || *version == 0
                {
                    return false;
                }
                if let Some(request) = request {
                    let Some(scope) = self.pending.get(channel) else {
                        return false;
                    };
                    if scope.account != *account
                        || scope.request != *request
                        || *message < scope.message
                    {
                        return false;
                    }
                    self.pending.remove(channel);
                }
            }
            Event::Failed {
                failure: Failure::ReadState { request, channel },
                ..
            } => {
                if self
                    .pending
                    .get(channel)
                    .is_none_or(|scope| scope.request != *request)
                {
                    return false;
                }
                self.pending.remove(channel);
            }
            _ => {}
        }
        true
    }

    fn revoke(&mut self, channel: ChannelId, found: &mut VecDeque<Event>) {
        self.readable.remove(&channel);
        if let Some(scope) = self.pending.remove(&channel) {
            found.push_back(failed(
                scope,
                t("La conversación ya no está disponible para confirmar la lectura."),
            ));
        }
    }

    pub fn expire(&mut self, now: Instant, timeout: Duration, found: &mut VecDeque<Event>) {
        self.pending.retain(|_, scope| {
            if now.saturating_duration_since(scope.since) < timeout {
                return true;
            }
            found.push_back(failed(
                *scope,
                t("No se recibió la confirmación de lectura a tiempo."),
            ));
            false
        });
    }

    pub fn deadline(&self, timeout: Duration) -> Option<Instant> {
        self.pending
            .values()
            .map(|scope| scope.since + timeout)
            .min()
    }

    pub fn resume(&mut self, paused: Instant, now: Instant) {
        for scope in self.pending.values_mut() {
            scope.since += now.saturating_duration_since(paused.max(scope.since));
        }
    }
}

fn readable(channel: &rune_model::Channel) -> bool {
    channel.kind.has_messages()
        && channel
            .permissions
            .contains(Permissions::VIEW | Permissions::READ_HISTORY)
}

fn failed(scope: PendingRead, reason: &str) -> Event {
    Event::Failed {
        failure: Failure::ReadState {
            request: scope.request,
            channel: scope.channel,
        },
        reason: reason.into(),
        ambiguous: true,
    }
}
