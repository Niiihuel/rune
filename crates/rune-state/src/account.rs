//! What the account keeps with its provider beside its profile, as the
//! provider reported it: how servers and channels notify, the folders of the
//! rail, its messaging privacy and its private notes on people. The
//! interface asks [`Store::account_abilities`] whether it may change each,
//! and reads these instead of what it keeps on this machine when the
//! provider says it keeps them.

use std::collections::HashMap;

use rune_i18n::t;
use rune_model::{
    ChannelId, MessagingPrivacy, Notifications, Server, ServerFolder, ServerId, UserId,
};
use rune_provider::{AuthenticationState, ConnectionState};

use crate::{Applied, Permit, Store};

/// The account's settings as last reported; `None` until reported.
#[derive(Debug, Default)]
pub(crate) struct AccountState {
    notifications: Option<HashMap<Option<ServerId>, Notifications>>,
    folders: Option<Vec<ServerFolder>>,
    privacy: Option<MessagingPrivacy>,
    notes: HashMap<UserId, String>,
}

/// What the account may change of what it keeps with the provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountAbilities {
    pub notifications: Permit,
    pub folders: Permit,
    pub privacy: Permit,
    pub notes: Permit,
}

/// One entry of the rail, as the account laid it out.
#[derive(Clone, Debug, PartialEq)]
pub enum RailEntry<'a> {
    Server(&'a Server),
    Folder {
        folder: &'a ServerFolder,
        servers: Vec<&'a Server>,
    },
}

impl Store {
    /// Whether the provider keeps how servers and channels notify, so the
    /// interface reads [`Store::notifications`] instead of its own.
    pub fn synced_notifications(&self) -> bool {
        self.capabilities().synced_notifications
    }

    /// How `server` notifies, or the direct conversations (`None`), as the
    /// account keeps it: the defaults when it was never set apart.
    pub fn notifications(&self, server: Option<ServerId>) -> Option<&Notifications> {
        self.account.notifications.as_ref()?.get(&server)
    }

    /// The server's settings of `channel`, when it has some of its own.
    pub fn channel_notifications(
        &self,
        channel: ChannelId,
    ) -> Option<rune_model::ChannelNotifications> {
        let server = self.channel(channel).and_then(|found| found.server);
        self.notifications(server)
            .map(|settings| settings.channel(channel))
            .filter(|found| found.level.is_some() || found.muted_until.is_some())
    }

    pub fn folders(&self) -> Option<&[ServerFolder]> {
        self.account.folders.as_deref()
    }

    pub fn messaging_privacy(&self) -> Option<&MessagingPrivacy> {
        self.account.privacy.as_ref()
    }

    /// The account's private note on `user`, once read; empty for none.
    pub fn note(&self, user: UserId) -> Option<&str> {
        self.account.notes.get(&user).map(String::as_str)
    }

    /// The rail's entries: servers the layout does not name first, as the
    /// service shows newly joined ones, then the layout's, skipping
    /// servers the account is no longer in and folders left empty. Without
    /// a layout, the servers in their order.
    pub fn rail(&self) -> Vec<RailEntry<'_>> {
        let servers = self.servers();
        let Some(folders) = self.folders() else {
            return servers.iter().map(RailEntry::Server).collect();
        };
        let named: std::collections::HashSet<ServerId> = folders
            .iter()
            .flat_map(|folder| folder.servers.iter().copied())
            .collect();
        let mut entries: Vec<RailEntry<'_>> = servers
            .iter()
            .filter(|server| !named.contains(&server.id))
            .map(RailEntry::Server)
            .collect();
        for folder in folders {
            let inside: Vec<&Server> = folder
                .servers
                .iter()
                .filter_map(|id| self.server(*id))
                .collect();
            match (folder.id, inside.as_slice()) {
                (_, []) => {}
                (None, [server, ..]) => entries.push(RailEntry::Server(server)),
                (Some(_), _) => entries.push(RailEntry::Folder {
                    folder,
                    servers: inside,
                }),
            }
        }
        entries
    }

    /// What the account may change of what it keeps with the provider.
    pub fn account_abilities(&self) -> AccountAbilities {
        let capabilities = self.capabilities();
        let connected = self.me().is_some()
            && !self.from_cache
            && self.connection == ConnectionState::Connected
            && (!capabilities.authentication
                || matches!(self.authentication, AuthenticationState::SignedIn { .. }));
        let permit = |declared: bool, reason: &str| {
            if !declared {
                Permit::Denied(reason.into())
            } else if !connected {
                Permit::Denied(t("Espera a que la conexión esté disponible").into())
            } else {
                Permit::Allowed
            }
        };
        AccountAbilities {
            notifications: permit(
                capabilities.synced_notifications,
                t("Este proveedor no guarda las notificaciones en la cuenta"),
            ),
            folders: permit(
                capabilities.server_folders,
                t("Este proveedor no tiene carpetas de servidores"),
            ),
            privacy: permit(
                capabilities.messaging_privacy,
                t("Este proveedor no tiene ajustes de privacidad de mensajes"),
            ),
            notes: permit(capabilities.notes, t("Este proveedor no guarda notas")),
        }
    }

    pub(crate) fn notifications_changed(
        &mut self,
        settings: Vec<Notifications>,
        replace: bool,
    ) -> Applied {
        let kept = self.account.notifications.get_or_insert_with(HashMap::new);
        if replace {
            kept.clear();
        }
        for entry in settings {
            kept.insert(entry.server, entry);
        }
        Applied::AccountSettings
    }

    pub(crate) fn folders_changed(&mut self, folders: Vec<ServerFolder>) -> Applied {
        self.account.folders = Some(folders);
        Applied::AccountSettings
    }

    pub(crate) fn privacy_changed(&mut self, privacy: MessagingPrivacy) -> Applied {
        self.account.privacy = Some(privacy);
        Applied::AccountSettings
    }

    pub(crate) fn note_changed(&mut self, user: UserId, note: String) -> Applied {
        self.account.notes.insert(user, note);
        Applied::Note(user)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use rune_model::{NotificationChange, Rgb, MUTED_FOREVER};
    use rune_provider::mock::{MockConfig, MockProvider};
    use rune_provider::{Event, Provider};

    use super::*;

    fn store() -> (Store, MockProvider) {
        let provider = MockProvider::new(MockConfig::default());
        let mut store = Store::new();
        store.provider = Some(provider.info());
        store.apply(Event::Ready(Box::new(provider.snapshot())), Instant::now());
        (store, provider)
    }

    #[test]
    fn the_rail_follows_the_layout_and_lists_unnamed_servers_first() {
        let (mut store, _) = store();
        let ids = |store: &Store| -> Vec<String> {
            store
                .rail()
                .iter()
                .map(|entry| match entry {
                    RailEntry::Server(server) => server.id.to_string(),
                    RailEntry::Folder { folder, servers } => format!(
                        "{}{:?}",
                        folder.name.clone().unwrap_or_default(),
                        servers.iter().map(|server| server.id.0).collect::<Vec<_>>()
                    ),
                })
                .collect()
        };
        assert_eq!(ids(&store), ["1", "2", "3", "4"]);
        store.apply(
            Event::ServerFolders(vec![
                ServerFolder::single(ServerId(2)),
                ServerFolder {
                    id: Some(5),
                    name: Some("Juntos".into()),
                    color: Some(Rgb(0xFF0000)),
                    // A server left long ago stays in the layout, unseen.
                    servers: vec![ServerId(3), ServerId(99), ServerId(1)],
                },
                ServerFolder {
                    id: Some(6),
                    name: Some("Vacía".into()),
                    color: None,
                    servers: vec![ServerId(98)],
                },
            ]),
            Instant::now(),
        );
        assert_eq!(ids(&store), ["4", "2", "Juntos[3, 1]"]);
    }

    #[test]
    fn notifications_replace_or_change_one_server_and_go_with_the_account() {
        let (mut store, _) = store();
        let muted = |server| Notifications {
            server: Some(ServerId(server)),
            muted_until: Some(MUTED_FOREVER),
            ..Notifications::default()
        };
        store.apply(
            Event::NotificationSettings {
                settings: vec![muted(1), muted(2)],
                replace: true,
            },
            Instant::now(),
        );
        let mut changed = muted(2);
        changed.apply(NotificationChange::Mute(None));
        store.apply(
            Event::NotificationSettings {
                settings: vec![changed],
                replace: false,
            },
            Instant::now(),
        );
        assert!(store.notifications(Some(ServerId(1))).is_some());
        assert_eq!(
            store.notifications(Some(ServerId(2))).unwrap().muted_until,
            None
        );
        store.apply(
            Event::NotificationSettings {
                settings: vec![muted(3)],
                replace: true,
            },
            Instant::now(),
        );
        assert!(store.notifications(Some(ServerId(1))).is_none());
        store.apply(
            Event::Authentication(AuthenticationState::SignedOut),
            Instant::now(),
        );
        assert!(store.notifications(Some(ServerId(3))).is_none());
    }

    #[test]
    fn abilities_need_the_capability_and_a_connection() {
        let (mut store, _) = store();
        let abilities = store.account_abilities();
        assert!(abilities.notifications.allowed() && abilities.notes.allowed());
        if let Some(info) = store.provider.as_mut() {
            info.capabilities.notes = false;
        }
        assert_eq!(
            store.account_abilities().notes.reason(),
            Some("Este proveedor no guarda notas")
        );
        store.connection = ConnectionState::Connecting;
        assert!(store
            .account_abilities()
            .folders
            .reason()
            .is_some_and(|reason| reason.starts_with("Espera")));
    }
}
