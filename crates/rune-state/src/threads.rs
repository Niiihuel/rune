//! Threads and forum posts beyond reading and replying: what the account may
//! start, follow and change, a channel's threads read a page at a time, and
//! a thread's count of messages as they arrive. Every permit joins what the
//! provider can do with what the account may do there, like
//! [`Abilities`](crate::Abilities); the interface asks again right before
//! sending, and the provider is the last word.

use std::collections::HashSet;

use rune_i18n::{t, tr};
use rune_model::{
    Channel, ChannelId, ChannelKind, Message, Permissions, ServerId, TagId, MAX_APPLIED_TAGS,
};
use rune_provider::{Command, ConnectionState, Failure, RequestId, ThreadScope};

use crate::{Applied, Permit, Store};

/// What the account may do with one thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadAbilities {
    /// Follow it, or stop.
    pub follow: Permit,
    /// Mark everything in it read.
    pub mark_read: Permit,
    /// Copy an address that opens it.
    pub link: Permit,
    /// Close it (archive) or open it again.
    pub close: Permit,
    /// Lock it to all but moderation, or unlock it.
    pub lock: Permit,
    /// Rename it and, for a post, change its tags.
    pub edit: Permit,
    pub delete: Permit,
    // Threads, forums and stages (2026-10-18).
    /// Silence it for a while or for good, or hear it again: only while
    /// the account follows it.
    pub mute: Permit,
}

/// What has been read of one channel's or forum's threads, open ones first,
/// then archived ones: a service may tell a session only of the open threads
/// the account follows.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThreadPages {
    /// The scope the next page belongs to, and its cursor; `None` before
    /// that scope's first page.
    pub scope: ThreadScope,
    pub next: Option<String>,
    /// The page being read.
    pub loading: Option<RequestId>,
    /// Every scope was read to its end.
    pub done: bool,
}

impl Store {
    /// Why nothing can be changed now, whatever the permissions say.
    pub(crate) fn offline(&self) -> Option<Permit> {
        (self.from_cache || self.connection != ConnectionState::Connected)
            .then(|| Permit::Denied(t("Espera a que la conexión esté disponible").into()))
    }

    /// Why the account takes no part in `server` now, if it does not.
    pub(crate) fn held(&self, server: Option<ServerId>) -> Option<Permit> {
        let server = server?;
        let restriction = self.restriction(server)?;
        Some(crate::profiles::restricted(
            restriction,
            &self.server(server)?.name,
        ))
    }

    /// Whether a thread can be started in `channel`: `private`, and from a
    /// message (`from_message`) or on its own.
    pub fn create_thread(&self, channel: ChannelId, private: bool, from_message: bool) -> Permit {
        let Some(parent) = self.channel(channel).filter(|c| Self::visible(c)) else {
            return Permit::Denied(t("El canal ya no está disponible").into());
        };
        let kind_ok = match parent.kind {
            ChannelKind::Text => !(private && from_message),
            ChannelKind::Announcement => !private,
            _ => false,
        };
        let needed = if private {
            Permissions::CREATE_PRIVATE_THREADS
        } else {
            Permissions::CREATE_THREADS
        };
        let needed = if from_message {
            needed | Permissions::READ_HISTORY
        } else {
            needed
        };
        if !self.capabilities().create_threads {
            Permit::Denied(t("Este proveedor no permite crear hilos").into())
        } else if !kind_ok {
            Permit::Denied(if private {
                t("Los hilos privados se abren en un canal de texto, sin partir de un mensaje")
                    .into()
            } else {
                t("En este canal no se abren hilos").into()
            })
        } else if !parent.permissions.contains(Permissions::VIEW | needed) {
            Permit::Denied(if private {
                tr!(
                    "No tienes permiso para crear hilos privados en #{channel}",
                    channel = parent.name
                )
            } else {
                tr!(
                    "No tienes permiso para crear hilos en #{channel}",
                    channel = parent.name
                )
            })
        } else if let Some(denied) = self.offline().or_else(|| self.held(parent.server)) {
            denied
        } else {
            Permit::Allowed
        }
    }

    /// Whether a post can be started in `forum`.
    pub fn create_post(&self, forum: ChannelId) -> Permit {
        let Some(parent) = self.channel(forum).filter(|c| Self::visible(c)) else {
            return Permit::Denied(t("El foro ya no está disponible").into());
        };
        if !self.capabilities().create_threads {
            Permit::Denied(t("Este proveedor no permite crear publicaciones").into())
        } else if parent.kind != ChannelKind::Forum {
            Permit::Denied(t("Solo un foro tiene publicaciones").into())
        } else if !parent
            .permissions
            .contains(Permissions::VIEW | Permissions::SEND)
        {
            Permit::Denied(tr!(
                "No tienes permiso para publicar en #{channel}",
                channel = parent.name
            ))
        } else if let Some(denied) = self.offline().or_else(|| self.held(parent.server)) {
            denied
        } else {
            Permit::Allowed
        }
    }

    /// Whether a new post in `forum` may carry files.
    pub fn post_files(&self, forum: ChannelId) -> Permit {
        match self.create_post(forum) {
            Permit::Allowed if !self.capabilities().attachments => {
                Permit::Denied(t("Este proveedor no permite adjuntar archivos").into())
            }
            Permit::Allowed
                if !self
                    .channel(forum)
                    .is_some_and(|found| found.permissions.contains(Permissions::ATTACH)) =>
            {
                Permit::Denied(t("No tienes permiso para adjuntar archivos en este foro").into())
            }
            permit => permit,
        }
    }

    /// Whether a post of `forum` may carry exactly `tags`, `before` being the
    /// ones it carries now: the forum's own, at most five, at least one if
    /// the forum asks, and new moderated ones only for moderation.
    pub fn post_tags(&self, forum: ChannelId, before: &[TagId], tags: &[TagId]) -> Permit {
        let Some(forum) = self.channel(forum) else {
            return Permit::Denied(t("El foro ya no está disponible").into());
        };
        let distinct: HashSet<&TagId> = tags.iter().collect();
        let moderates = forum.permissions.contains(Permissions::MANAGE_THREADS);
        if distinct.len() != tags.len()
            || tags
                .iter()
                .any(|tag| !forum.tags.iter().any(|t| t.id == *tag))
        {
            Permit::Denied(t("Esas etiquetas no son del foro").into())
        } else if tags.len() > MAX_APPLIED_TAGS {
            Permit::Denied(tr!(
                "Una publicación lleva hasta {max} etiquetas",
                max = MAX_APPLIED_TAGS
            ))
        } else if forum.forum.require_tag && tags.is_empty() {
            Permit::Denied(t("Este foro pide al menos una etiqueta").into())
        } else if !moderates
            && tags.iter().any(|tag| {
                !before.contains(tag) && forum.tags.iter().any(|t| t.id == *tag && t.moderated)
            })
        {
            Permit::Denied(t("Esa etiqueta es solo para la moderación").into())
        } else {
            Permit::Allowed
        }
    }

    /// What the account may do with `thread`; `None` for what is not a
    /// thread it sees.
    pub fn thread_abilities(&self, thread: ChannelId) -> Option<ThreadAbilities> {
        let channel = self
            .channel(thread)
            .filter(|c| c.is_thread() && Self::visible(c))?;
        let info = channel.thread.clone().unwrap_or_default();
        let capabilities = self.capabilities();
        let moderates = channel.permissions.contains(Permissions::MANAGE_THREADS);
        let owns = info.owner.is_some() && info.owner == self.me();
        let unavailable = self.offline().or_else(|| self.held(channel.server));
        let gate = |provider: bool, what: &str, may: bool, why: &str| {
            if !provider {
                Permit::Denied(tr!("Este proveedor no permite {what}", what))
            } else if !may {
                Permit::Denied(why.to_owned())
            } else if let Some(denied) = &unavailable {
                denied.clone()
            } else {
                Permit::Allowed
            }
        };
        let only_moderation = t("Solo la moderación puede hacerlo en este servidor");
        Some(ThreadAbilities {
            follow: if !capabilities.follow_threads {
                Permit::Denied(t("Este proveedor no permite seguir hilos").into())
            } else if info.archived {
                Permit::Denied(t("Abre el hilo otra vez para seguirlo o dejar de seguirlo").into())
            } else {
                // Seeing it is enough, even while timed out.
                self.offline().unwrap_or(Permit::Allowed)
            },
            mark_read: if self.read(thread).unread() {
                Permit::Allowed
            } else {
                Permit::Denied(t("No hay nada sin leer").into())
            },
            link: match (capabilities.channel_links, channel.server) {
                (Some(_), Some(_)) => Permit::Allowed,
                _ => Permit::Denied(t("Este proveedor no tiene enlaces a los hilos").into()),
            },
            close: gate(
                capabilities.manage_threads,
                t("cerrar hilos"),
                moderates || (owns && !info.locked),
                if info.locked {
                    t("El hilo está bloqueado: solo la moderación lo abre o lo cierra")
                } else {
                    t("Solo quien abrió el hilo o la moderación lo cierra")
                },
            ),
            lock: gate(
                capabilities.manage_threads,
                t("bloquear hilos"),
                moderates,
                only_moderation,
            ),
            edit: gate(
                capabilities.manage_threads,
                t("editar hilos"),
                moderates || owns,
                t("Solo quien abrió el hilo o la moderación lo edita"),
            ),
            delete: gate(
                capabilities.manage_threads,
                t("eliminar hilos"),
                moderates,
                only_moderation,
            ),
            mute: if !capabilities.mute_threads {
                Permit::Denied(t("Este proveedor no permite silenciar hilos").into())
            } else if !info.joined {
                Permit::Denied(t("Sigue el hilo para silenciarlo").into())
            } else {
                // Seeing it is enough, as with following it.
                self.offline().unwrap_or(Permit::Allowed)
            },
        })
    }

    /// The address that opens `thread` elsewhere, when the provider has one.
    pub fn thread_link(&self, thread: ChannelId) -> Option<String> {
        let base = self.capabilities().channel_links?;
        let server = self.channel(thread)?.server?;
        Some(format!("{base}/{server}/{thread}"))
    }

    /// What has been read of `parent`'s threads.
    pub fn thread_pages(&self, parent: ChannelId) -> Option<&ThreadPages> {
        self.thread_pages.get(&parent)
    }

    /// Whether another page of `parent`'s threads can be asked for.
    pub fn more_threads(&self, parent: ChannelId) -> Permit {
        let Some(found) = self.channel(parent).filter(|c| Self::visible(c)) else {
            return Permit::Denied(t("El canal ya no está disponible").into());
        };
        if !self.capabilities().thread_pages {
            return Permit::Denied(t("Este proveedor no permite buscar más hilos").into());
        }
        if !matches!(
            found.kind,
            ChannelKind::Text | ChannelKind::Announcement | ChannelKind::Forum
        ) {
            return Permit::Denied(t("Este canal no tiene hilos").into());
        }
        if !found.permissions.contains(Permissions::READ_HISTORY) {
            return Permit::Denied(tr!(
                "No tienes permiso para leer el historial de #{channel}",
                channel = found.name
            ));
        }
        match self.thread_pages.get(&parent) {
            Some(pages) if pages.done => Permit::Denied(t("No hay más hilos archivados").into()),
            Some(pages) if pages.loading.is_some() => Permit::Denied(t("Cargando hilos…").into()),
            _ => self.offline().unwrap_or(Permit::Allowed),
        }
    }

    /// Ask for the next page of `parent`'s threads as `request`, when it may
    /// be; one page at a time.
    pub fn load_threads(&mut self, parent: ChannelId, request: RequestId) -> Option<Command> {
        if !self.more_threads(parent).allowed() {
            return None;
        }
        let pages = self.thread_pages.entry(parent).or_default();
        pages.loading = Some(request);
        Some(Command::LoadThreads {
            request,
            parent,
            scope: pages.scope,
            before: pages.next.clone(),
        })
    }

    /// The page asked for with `request` arrived: keep where it left, and go
    /// on to the next scope once one ends: open ones, archived public ones,
    /// then archived private ones. A page nobody waits for any more changes
    /// nothing, and a thread named twice is one channel all the same.
    pub(crate) fn threads_page(
        &mut self,
        request: RequestId,
        parent: ChannelId,
        scope: ThreadScope,
        next: Option<String>,
    ) -> Applied {
        // Private threads hang only from text channels; all of them only
        // moderation reads, the others the ones the account is in.
        let after_archived = self.channel(parent).and_then(|found| {
            (found.kind == ChannelKind::Text).then(|| {
                if found.permissions.contains(Permissions::MANAGE_THREADS) {
                    ThreadScope::Private
                } else {
                    ThreadScope::Joined
                }
            })
        });
        let Some(pages) = self
            .thread_pages
            .get_mut(&parent)
            .filter(|pages| pages.loading == Some(request) && pages.scope == scope)
        else {
            return Applied::Nothing;
        };
        pages.loading = None;
        match next {
            Some(next) => pages.next = Some(next),
            None => {
                pages.next = None;
                match (scope, after_archived) {
                    (ThreadScope::Active, _) => pages.scope = ThreadScope::Archived,
                    (ThreadScope::Archived, Some(scope)) => pages.scope = scope,
                    _ => pages.done = true,
                }
            }
        }
        // Its threads arrived before it, as channels.
        Applied::Nothing
    }

    /// After a new session of the same account: the threads of `before`
    /// that it does not name stay, as the pages read them (a service may
    /// tell a session only of those the account follows), under a parent
    /// it still names and sees, with what that parent allows now. An open
    /// one it followed goes: the session would have named it. One deleted
    /// meanwhile stays until the provider says so.
    pub(crate) fn keep_paged_threads(&mut self, before: impl Iterator<Item = Channel>) {
        for mut thread in before {
            let followed_open = thread
                .thread
                .as_ref()
                .is_some_and(|info| info.joined && !info.archived);
            if !thread.is_thread() || followed_open || self.channels.contains_key(&thread.id) {
                continue;
            }
            let Some(parent) = thread
                .parent
                .and_then(|parent| self.channels.get(&parent))
                .filter(|parent| parent.server == thread.server && Self::visible(parent))
            else {
                continue;
            };
            thread.permissions = parent.permissions;
            self.channels.insert(thread.id, thread);
        }
    }

    /// A page that failed can be asked for again.
    pub(crate) fn threads_failed(&mut self, failure: &Failure) {
        if let Failure::ThreadList { request, parent } = failure {
            if let Some(pages) = self
                .thread_pages
                .get_mut(parent)
                .filter(|pages| pages.loading == Some(*request))
            {
                pages.loading = None;
            }
        }
    }

    /// A new message counts in its thread, once: only one newer than any
    /// known there (`fresh`) moves the count and the last activity.
    pub(crate) fn thread_message(&mut self, message: &Message, fresh: bool) {
        if !fresh {
            return;
        }
        // Other channels' messages keep the index: only a thread's activity sorts it.
        if !self
            .channels
            .get(&message.channel)
            .is_some_and(Channel::is_thread)
        {
            return;
        }
        if let Some(thread) = self
            .channels
            .get_mut(&message.channel)
            .and_then(|c| c.thread.as_mut())
        {
            thread.messages = thread.messages.saturating_add(1);
            thread.last_activity = thread.last_activity.max(message.timestamp);
        }
    }
}
