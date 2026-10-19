//! What the simulated account keeps with its service: how each server and
//! channel notifies, its folders of servers, its messaging privacy and its
//! private notes. It starts the same every time: two servers in a folder,
//! direct messages off in one server, and a note on Ada.

use std::collections::HashMap;

use rune_i18n::t;
use rune_model::{
    account::{valid_folders, valid_note},
    ChannelKind, MessagingPrivacy, NotificationChange, Notifications, PrivacyChange, Rgb,
    ServerFolder, ServerId, UserId,
};

use super::World;
use crate::{Command, Event};

type Outcome = Result<Vec<Event>, String>;

/// The simulated account's settings.
#[derive(Debug)]
pub(super) struct AccountSim {
    notifications: HashMap<Option<ServerId>, Notifications>,
    folders: Vec<ServerFolder>,
    privacy: MessagingPrivacy,
    notes: HashMap<UserId, String>,
    // Desktop session and games (escritorio-sesion-y-juegos).
    /// Whether the account shows its activity: on, as a new account's.
    pub(super) sharing: bool,
}

impl AccountSim {
    /// Taller de Rust and Laboratorio limn on their own, Café Nocturno and
    /// Club de Fotografía in a folder; nobody of Club de Fotografía may
    /// write directly; a note on Ada.
    pub(super) fn starting() -> Self {
        let mut privacy = MessagingPrivacy::default();
        privacy.apply(PrivacyChange::ServerDmsIn(ServerId(4), false));
        Self {
            notifications: HashMap::new(),
            folders: vec![
                ServerFolder::single(ServerId(1)),
                ServerFolder::single(ServerId(2)),
                ServerFolder {
                    id: Some(1),
                    name: Some("Comunidades".into()),
                    color: Some(Rgb(0x8A_6C_D9)),
                    servers: vec![ServerId(3), ServerId(4)],
                },
            ],
            privacy,
            notes: HashMap::from([(UserId(10), "Conocimos a Ada en la RustConf.".into())]),
            sharing: true,
        }
    }
}

impl World {
    /// What a session reports of the account's settings once it is ready.
    pub(super) fn account_events(&self) -> Vec<Event> {
        let account = &self.account;
        vec![
            Event::NotificationSettings {
                settings: account.notifications.values().cloned().collect(),
                replace: true,
            },
            Event::ServerFolders(account.folders.clone()),
            Event::MessagingPrivacy(account.privacy.clone()),
            Event::ActivitySharing(account.sharing),
        ]
    }

    /// Carry out a command about the account's settings, or `None` for others.
    pub(super) fn account_command(&mut self, command: &Command) -> Option<Outcome> {
        Some(match command {
            Command::LoadAccountSettings => {
                let mut events = self.account_events();
                // The status is read again with them, as a service keeps it.
                events.remove(0);
                events.insert(
                    0,
                    Event::PresenceChanged {
                        user: super::ME,
                        presence: self.presences.get(&super::ME).cloned().unwrap_or_default(),
                    },
                );
                Ok(events)
            }
            Command::SetNotifications { server, change } => self.notify(*server, *change),
            Command::SetServerFolders(folders) => {
                if !valid_folders(folders) {
                    return Some(Err(t("esas carpetas no son válidas").into()));
                }
                self.account.folders = folders.clone();
                Ok(vec![Event::ServerFolders(folders.clone())])
            }
            Command::SetMessagingPrivacy(change) => {
                if let PrivacyChange::ServerDmsIn(server, _)
                | PrivacyChange::RequestFilterIn(server, _) = change
                {
                    if self.server(*server).is_err() {
                        return Some(Err(t("el servidor no existe").into()));
                    }
                }
                self.account.privacy.apply(*change);
                Ok(vec![Event::MessagingPrivacy(self.account.privacy.clone())])
            }
            Command::LoadNote { user } => Ok(vec![Event::NoteChanged {
                user: *user,
                note: self.account.notes.get(user).cloned().unwrap_or_default(),
            }]),
            Command::SetNote { user, note } => {
                if !valid_note(note) {
                    return Some(Err(t("la nota puede tener hasta 256 caracteres").into()));
                }
                let note = note.trim().to_owned();
                if note.is_empty() {
                    self.account.notes.remove(user);
                } else {
                    self.account.notes.insert(*user, note.clone());
                }
                Ok(vec![Event::NoteChanged { user: *user, note }])
            }
            // Desktop session and games (escritorio-sesion-y-juegos): the
            // activity shown, or hidden, follows the choice at once.
            Command::SetActivitySharing(on) => {
                self.account.sharing = *on;
                let mut events = vec![Event::ActivitySharing(*on)];
                events.extend(self.local_visibility());
                Ok(events)
            }
            _ => return None,
        })
    }

    fn notify(&mut self, server: Option<ServerId>, change: NotificationChange) -> Outcome {
        if server.is_some_and(|server| self.server(server).is_err()) {
            return Err(t("el servidor no existe").into());
        }
        if let NotificationChange::ChannelLevel(channel, _)
        | NotificationChange::ChannelMute(channel, _) = change
        {
            let fits = self.channel(channel).is_some_and(|found| match server {
                Some(_) => found.server == server,
                None => matches!(found.kind, ChannelKind::Direct | ChannelKind::Group),
            });
            if !fits {
                return Err(t("el canal no es de ese servidor").into());
            }
        }
        let settings = self
            .account
            .notifications
            .entry(server)
            .or_insert_with(|| Notifications {
                server,
                ..Notifications::default()
            });
        settings.apply(change);
        Ok(vec![Event::NotificationSettings {
            settings: vec![settings.clone()],
            replace: false,
        }])
    }
}

#[cfg(test)]
mod tests {
    use rune_model::{ChannelId, NotifyLevel, MUTED_FOREVER};

    use super::*;
    use crate::mock::{MockConfig, MockProvider};

    #[test]
    fn the_account_starts_with_a_folder_and_keeps_what_changes() {
        let provider = MockProvider::new(MockConfig::default());
        let events = provider.manage(Command::LoadAccountSettings);
        assert!(matches!(&events[0], Event::PresenceChanged { .. }));
        assert!(events.iter().any(|event| matches!(event,
            Event::ServerFolders(folders) if folders[2].name.as_deref() == Some("Comunidades"))));
        let muted = provider.manage(Command::SetNotifications {
            server: Some(ServerId(1)),
            change: NotificationChange::Mute(Some(MUTED_FOREVER)),
        });
        let Event::NotificationSettings { settings, replace } = &muted[0] else {
            panic!("{muted:?}")
        };
        assert!(!replace);
        assert_eq!(settings[0].muted_until, Some(MUTED_FOREVER));
        let leveled = provider.manage(Command::SetNotifications {
            server: Some(ServerId(1)),
            change: NotificationChange::Level(NotifyLevel::Mentions),
        });
        assert!(
            matches!(&leveled[0], Event::NotificationSettings { settings, .. }
            if settings[0].muted_until == Some(MUTED_FOREVER)
                && settings[0].level == Some(NotifyLevel::Mentions))
        );
        // A direct conversation is not a channel of a server.
        let refused = provider.manage(Command::SetNotifications {
            server: Some(ServerId(1)),
            change: NotificationChange::ChannelMute(ChannelId(1), None),
        });
        assert!(matches!(&refused[0], Event::Failed { .. }), "{refused:?}");
    }

    #[test]
    fn notes_privacy_and_folders_answer_with_what_was_kept() {
        let provider = MockProvider::new(MockConfig::default());
        let ada = UserId(10);
        assert!(
            matches!(&provider.manage(Command::LoadNote { user: ada })[0],
            Event::NoteChanged { note, .. } if note.contains("RustConf"))
        );
        assert!(
            matches!(&provider.manage(Command::SetNote { user: ada, note: "  ".into() })[0],
            Event::NoteChanged { note, .. } if note.is_empty())
        );
        let privacy = provider.manage(Command::SetMessagingPrivacy(PrivacyChange::ServerDmsIn(
            ServerId(4),
            true,
        )));
        assert!(matches!(&privacy[0], Event::MessagingPrivacy(kept) if kept.dms_in(ServerId(4))));
        let folders = vec![
            ServerFolder::single(ServerId(2)),
            ServerFolder::single(ServerId(1)),
        ];
        assert_eq!(
            provider.manage(Command::SetServerFolders(folders.clone())),
            vec![Event::ServerFolders(folders)]
        );
        let repeated = provider.manage(Command::SetServerFolders(vec![
            ServerFolder::single(ServerId(1)),
            ServerFolder::single(ServerId(1)),
        ]));
        assert!(matches!(&repeated[0], Event::Failed { .. }));
    }

    // Desktop session and games (escritorio-sesion-y-juegos).
    #[test]
    fn activity_sharing_is_read_with_the_settings_and_hides_the_activity_shown() {
        use rune_model::ActivityVisibility;
        let provider = MockProvider::new(MockConfig::default());
        let read = provider.manage(Command::LoadAccountSettings);
        assert!(read.contains(&Event::ActivitySharing(true)));
        // Nothing of this device shows yet: nothing to see or hide.
        assert_eq!(
            provider.manage(Command::SetActivitySharing(false)),
            vec![Event::ActivitySharing(false)]
        );
        provider
            .world
            .lock()
            .unwrap()
            .local_activity(Some(rune_model::LocalActivity {
                name: Some("Ajedrez de prueba".into()),
                ..Default::default()
            }));
        assert_eq!(
            provider.manage(Command::SetActivitySharing(true)),
            vec![
                Event::ActivitySharing(true),
                Event::ActivityVisibility(ActivityVisibility::Listed)
            ]
        );
    }
}
