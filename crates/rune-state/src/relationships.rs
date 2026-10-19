//! Friends, requests and blocks, as the provider reports them, and what the
//! account may do about each person. The list is keyed by person, so a
//! repeated or late report replaces what was known instead of adding a row.
//! Until the provider reports a whole list it stays unknown: a single event
//! must not pass for the complete list.

use std::collections::HashMap;

use rune_i18n::{t, tr};
use rune_model::{ChannelId, ChannelKind, Relationship, RelationshipKind, User, UserId};
use rune_provider::{AuthenticationState, ConnectionState};

use crate::{Applied, Permit, Store};

/// What the account may do about one person.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SocialAbilities {
    /// Accept the request they sent.
    pub accept: Permit,
    /// Remove a friend, ignore their request or cancel the account's.
    pub remove: Permit,
    pub block: Permit,
    pub unblock: Permit,
    /// Open the direct conversation with them, starting it if needed.
    pub message: Permit,
    /// Give a friend a private name, or take it away.
    pub nickname: Permit,
}

impl Store {
    pub(crate) fn ready_relationships(&mut self, relationships: Option<Vec<Relationship>>) {
        self.relationships = relationships.map(|list| {
            list.into_iter()
                .map(|relationship| (relationship.user, relationship))
                .collect::<HashMap<_, _>>()
        });
    }

    pub(crate) fn relationship_changed(&mut self, relationship: Relationship) -> Applied {
        let user = relationship.user;
        match self.relationships.as_mut() {
            Some(known) if Some(user) != self.me => {
                known.insert(user, relationship);
                Applied::Relationship(user)
            }
            _ => Applied::Nothing,
        }
    }

    pub(crate) fn relationship_removed(&mut self, user: UserId) -> Applied {
        match self
            .relationships
            .as_mut()
            .and_then(|known| known.remove(&user))
        {
            Some(_) => Applied::Relationship(user),
            None => Applied::Nothing,
        }
    }

    /// Everyone the account has a relationship with, by name; `None` while
    /// the provider has not reported the list, or cannot.
    pub fn relationships(&self) -> Option<Vec<&Relationship>> {
        let known = self.relationships.as_ref()?;
        let mut list: Vec<(String, &Relationship)> = known
            .values()
            .map(|relationship| {
                (
                    self.user(relationship.user)
                        .map_or_else(String::new, |user| user.shown_name().to_lowercase()),
                    relationship,
                )
            })
            .collect();
        list.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.user.cmp(&b.1.user)));
        Some(
            list.into_iter()
                .map(|(_, relationship)| relationship)
                .collect(),
        )
    }

    pub fn relationship(&self, user: UserId) -> Option<&Relationship> {
        self.relationships.as_ref()?.get(&user)
    }

    /// Whether the account blocked `user`.
    pub fn blocked(&self, user: UserId) -> bool {
        self.relationship(user)
            .is_some_and(|relationship| relationship.kind == RelationshipKind::Blocked)
    }

    /// The direct conversation with `user`, when there is one.
    pub fn direct_with(&self, user: UserId) -> Option<ChannelId> {
        self.channels
            .values()
            .find(|channel| channel.kind == ChannelKind::Direct && channel.recipients == [user])
            .map(|channel| channel.id)
    }

    /// Whether the list of friends can be shown, and why not.
    pub fn friends_list(&self) -> Permit {
        if !self.capabilities().relationships {
            Permit::Denied(t("Esta conexión no ofrece la lista de amigos").into())
        } else if self.relationships.is_none() {
            Permit::Denied(t("La lista de amigos todavía no llegó").into())
        } else {
            Permit::Allowed
        }
    }

    /// Whether a friend request can be sent by handle.
    pub fn add_friend(&self) -> Permit {
        self.social_write(
            self.capabilities().friend_requests,
            t("enviar solicitudes de amistad"),
        )
    }

    /// What the account may do about `user`.
    pub fn social(&self, user: UserId) -> SocialAbilities {
        let kind = self
            .relationship(user)
            .map(|relationship| relationship.kind);
        let capabilities = self.capabilities();
        let requests = self.social_write(capabilities.friend_requests, t("gestionar amistades"));
        let blocks = self.social_write(capabilities.block_users, t("bloquear personas"));
        let when = |permit: &Permit, ok: bool, otherwise: &str| match permit {
            Permit::Allowed if !ok => Permit::Denied(otherwise.into()),
            permit => permit.clone(),
        };
        SocialAbilities {
            accept: when(
                &requests,
                kind == Some(RelationshipKind::IncomingRequest),
                t("No hay una solicitud de esta persona"),
            ),
            remove: when(
                &requests,
                kind.is_some_and(|kind| kind != RelationshipKind::Blocked),
                t("No hay una amistad ni una solicitud con esta persona"),
            ),
            block: when(
                &blocks,
                kind != Some(RelationshipKind::Blocked) && Some(user) != self.me,
                t("Ya bloqueaste a esta persona"),
            ),
            unblock: when(
                &blocks,
                kind == Some(RelationshipKind::Blocked),
                t("No bloqueaste a esta persona"),
            ),
            message: self.open_direct(user),
            nickname: when(
                &self.social_write(
                    capabilities.friend_nicknames,
                    t("poner apodos a tus amigos"),
                ),
                kind == Some(RelationshipKind::Friend),
                t("Solo tus amigos pueden tener un apodo"),
            ),
        }
    }

    /// Requests waiting for the account's answer, outside the spam folder.
    pub fn pending_requests(&self) -> usize {
        self.relationships.as_ref().map_or(0, |known| {
            known.values().filter(|r| r.awaits_answer()).count()
        })
    }

    /// Whether the provider says who of the account's friends is around.
    pub fn friends_presence(&self) -> bool {
        let capabilities = self.capabilities();
        capabilities.presence || capabilities.friend_presence
    }

    /// Whether the direct conversation with `user` can be opened: one that
    /// exists always can; a new one needs the provider and no block.
    pub fn open_direct(&self, user: UserId) -> Permit {
        if Some(user) == self.me {
            Permit::Denied(t("No podés escribirte a vos").into())
        } else if self.direct_with(user).is_some() {
            Permit::Allowed
        } else if self.blocked(user) {
            Permit::Denied(t("Bloqueaste a esta persona: desbloqueala para escribirle").into())
        } else {
            self.social_write(self.capabilities().open_direct, t("iniciar conversaciones"))
        }
    }

    /// A write about people: supported, with an account, connected.
    pub(crate) fn social_write(&self, supported: bool, what: &str) -> Permit {
        if !supported {
            Permit::Denied(tr!("Esta conexión no permite {what}", what))
        } else if self.me.is_none() {
            Permit::Denied(t("Conectá una cuenta primero").into())
        } else if self.from_cache || self.connection != ConnectionState::Connected {
            Permit::Denied(t("Esperá a que la conexión esté disponible").into())
        } else if self.capabilities().authentication
            && !matches!(self.authentication, AuthenticationState::SignedIn { .. })
        {
            Permit::Denied(t("Iniciá sesión primero").into())
        } else {
            Permit::Allowed
        }
    }

    pub(crate) fn capabilities(&self) -> rune_provider::Capabilities {
        self.provider
            .as_ref()
            .map(|info| info.capabilities)
            .unwrap_or_default()
    }

    /// The person on the other side of a direct conversation the account
    /// blocked, if it is one.
    pub(crate) fn blocked_recipient(&self, channel: ChannelId) -> Option<&User> {
        let channel = self.channel(channel)?;
        match channel.recipients[..] {
            [user] if channel.kind == ChannelKind::Direct && self.blocked(user) => self.user(user),
            _ => None,
        }
    }
}
