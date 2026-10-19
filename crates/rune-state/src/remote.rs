//! What is looked up at the provider beyond what is loaded: messages through
//! a whole history, and members by name.

use rune_i18n::{t, tr};
use rune_model::{ChannelId, Member, ServerId};
use rune_provider::{AuthenticationState, ConnectionState};

use crate::{Applied, Permit, Store};

impl Store {
    /// Whether the account can ask the provider now, and why not.
    fn lookup_ready(&self, what: &str) -> Permit {
        if self.me().is_none() {
            Permit::Denied(tr!("Conectá una cuenta para {what}", what))
        } else if self.from_cache || self.connection != ConnectionState::Connected {
            Permit::Denied(t("Esperá a que la conexión esté disponible").into())
        } else if self
            .provider
            .as_ref()
            .is_some_and(|provider| provider.capabilities.authentication)
            && !matches!(self.authentication, AuthenticationState::SignedIn { .. })
        {
            Permit::Denied(tr!("Iniciá sesión para {what}", what))
        } else {
            Permit::Allowed
        }
    }

    /// Whether `channel`'s whole history, or its server's, can be searched
    /// at the provider.
    pub fn remote_search(&self, channel: ChannelId) -> Permit {
        if !self
            .provider
            .as_ref()
            .is_some_and(|provider| provider.capabilities.message_search)
        {
            return Permit::Denied(t("Este proveedor no busca en todo el historial").into());
        }
        let ready = self.lookup_ready(t("buscar mensajes"));
        if !ready.allowed() {
            return ready;
        }
        match self.abilities(channel) {
            Some(abilities) => abilities.read_history,
            None => Permit::Denied(t("Esta conversación ya no está disponible").into()),
        }
    }

    /// Whether `server`'s members can be looked up by name at the provider.
    pub fn member_search(&self, server: ServerId) -> Permit {
        if !self
            .provider
            .as_ref()
            .is_some_and(|provider| provider.capabilities.member_search)
        {
            return Permit::Denied(t("Este proveedor no busca miembros").into());
        }
        if self.server(server).is_none() {
            return Permit::Denied(t("Ya no estás en ese servidor").into());
        }
        self.lookup_ready(t("buscar miembros"))
    }

    /// Members found by name join the ones known, once each.
    pub(super) fn members_found(
        &mut self,
        server: ServerId,
        query: String,
        members: Vec<Member>,
    ) -> Applied {
        if self.server(server).is_none() || members.iter().any(|member| member.server != server) {
            return Applied::Nothing;
        }
        for member in members {
            self.apply_management(rune_provider::Event::MemberChanged(member));
        }
        Applied::MembersFound { server, query }
    }
}
