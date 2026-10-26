//! Request-scoped, explicitly partial navigation windows.

use std::sync::Arc;

use rune_i18n::t;
use rune_model::{CachedContext, ChannelId, HistorySource, Message, MessageId};
use rune_provider::{AuthenticationState, ConnectionState, Failure, RequestId};

use crate::{Applied, Permit, Store};

#[derive(Debug)]
pub(super) struct ContextRead {
    channel: ChannelId,
    request: RequestId,
    anchor: MessageId,
    source: HistorySource,
    dirty: bool,
}

impl Store {
    /// Admit one navigation window before dispatching its local or remote read.
    pub fn begin_context(
        &mut self,
        channel: ChannelId,
        request: RequestId,
        anchor: MessageId,
        source: HistorySource,
    ) -> Permit {
        let permit = self.context_permit(channel, source);
        if !permit.allowed() {
            return permit;
        }
        if anchor.0 == 0 || request == 0 || request <= self.context_request {
            return Permit::Denied(t("Esta solicitud de contexto ya venció").into());
        }
        self.context_request = request;
        self.context_read = Some(ContextRead {
            channel,
            request,
            anchor,
            source,
            dirty: false,
        });
        Permit::Allowed
    }

    pub fn context_permit(&self, channel: ChannelId, source: HistorySource) -> Permit {
        let permit = self
            .abilities(channel)
            .map(|abilities| match source {
                HistorySource::Cached => abilities.read_history,
                HistorySource::Provider => abilities.message_context,
            })
            .unwrap_or_else(|| Permit::Denied(t("Esta conversación ya no está disponible").into()));
        if !permit.allowed() {
            return permit;
        }
        if self.me().is_none() {
            return Permit::Denied(t("Conecta una cuenta para consultar su historial").into());
        }
        if source == HistorySource::Provider
            && (self.from_cache
                || self.connection != ConnectionState::Connected
                || self
                    .provider
                    .as_ref()
                    .is_some_and(|provider| provider.capabilities.authentication)
                    && !matches!(self.authentication, AuthenticationState::SignedIn { .. }))
        {
            return Permit::Denied(
                t("Espera a que la sesión esté disponible para cargar el contexto").into(),
            );
        }
        Permit::Allowed
    }

    pub(super) fn expects_context(&self, channel: ChannelId, request: RequestId) -> bool {
        self.context_read
            .as_ref()
            .is_some_and(|read| read.channel == channel && read.request == request)
    }

    /// An obsolete cancellation cannot discard a newer navigation.
    pub fn cancel_context(&mut self, request: RequestId) {
        if self
            .context_read
            .as_ref()
            .is_some_and(|read| read.request == request)
        {
            self.context_read = None;
        }
    }

    pub(super) fn dirty_context(&mut self, channel: ChannelId) {
        if let Some(read) = self
            .context_read
            .as_mut()
            .filter(|read| read.channel == channel)
        {
            read.dirty = true;
        }
    }

    pub(super) fn forget_context(&mut self, channel: ChannelId) {
        if self
            .context_read
            .as_ref()
            .is_some_and(|read| read.channel == channel)
        {
            self.context_read = None;
        }
        if let Some(timeline) = self.timelines.get_mut(&channel) {
            timeline.clear_context();
        }
    }

    /// Cached neighbors have unknown provider coverage and never reconcile a
    /// deletion range. Their absence cannot delete anything from storage.
    pub fn apply_cached_context(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        context: CachedContext,
    ) -> Applied {
        self.loaded_context(
            channel,
            request,
            context.anchor,
            context.messages,
            HistorySource::Cached,
        )
    }

    pub(super) fn loaded_context(
        &mut self,
        channel: ChannelId,
        request: RequestId,
        anchor: MessageId,
        messages: Vec<Message>,
        source: HistorySource,
    ) -> Applied {
        if !self.expects_context(channel, request)
            || !self
                .context_read
                .as_ref()
                .is_some_and(|read| read.source == source)
        {
            return Applied::Nothing;
        }
        let read = self.context_read.take().unwrap();
        let permit = self.context_permit(channel, source);
        let failed = |reason: &str| Applied::Failed {
            failure: Failure::Context { channel, request },
            reason: reason.into(),
            ambiguous: false,
        };
        if let Some(reason) = permit.reason() {
            return failed(reason);
        }
        if read.dirty {
            return failed(t(
                "Los mensajes cambiaron durante la carga del contexto. Vuelve a intentarlo.",
            ));
        }
        if anchor != read.anchor
            || messages.len() > 51
            || !messages.iter().any(|message| message.id == anchor)
            || messages.iter().any(|message| {
                message.channel != channel || message.id.0 == 0 || message.delivery.is_local()
            })
            || messages.windows(2).any(|pair| pair[0].id >= pair[1].id)
        {
            return failed(t("El contexto recibido no es válido o no contiene el mensaje buscado. Vuelve a intentarlo."));
        }
        if self
            .timelines
            .get(&channel)
            .is_some_and(|timeline| timeline.is_deleted(anchor))
        {
            return failed(t(
                "Ese mensaje ya no está disponible en el contexto recibido.",
            ));
        }
        let mut shown = Vec::with_capacity(messages.len());
        let mut undo = Vec::new();
        for message in messages {
            if self
                .timelines
                .get(&channel)
                .is_some_and(|timeline| timeline.is_deleted(message.id))
            {
                continue;
            }
            let original = self.before_change.get(&(channel, message.id));
            let visible = self
                .timelines
                .get(&channel)
                .and_then(|timeline| timeline.get(message.id));
            let retained = original
                .or(visible)
                .map(Arc::as_ref)
                .into_iter()
                .chain(self.retained_pin(channel, message.id))
                .max_by_key(|known| known.edited);
            if retained
                .is_some_and(|known| known.author != message.author || known.delivery.is_local())
            {
                return failed(t(
                    "No pudimos verificar la autoría de un mensaje del contexto.",
                ));
            }
            let confirmed = retained
                .filter(|known| known.edited > message.edited)
                .unwrap_or(&message)
                .clone();
            let mut current = confirmed.clone();
            if let (Some(original), Some(visible)) = (original, visible) {
                current = crate::message_patches::rebase_local(&confirmed, original, visible);
                undo.push((
                    (channel, message.id),
                    (current != confirmed).then(|| Arc::new(confirmed)),
                ));
            }
            shown.push(current);
        }
        for (key, confirmed) in undo {
            if let Some(confirmed) = confirmed {
                self.before_change.insert(key, confirmed);
            } else {
                self.before_change.remove(&key);
            }
        }
        self.timelines
            .entry(channel)
            .or_default()
            .apply_context(shown, source);
        self.revision += 1;
        Applied::Context {
            channel,
            request,
            anchor,
        }
    }
}
