//! What the account keeps with Discord beside its profile, by the routes
//! Serein uses: `settings-proto/1` for the status, the folders and the
//! messaging privacy (`discord-api/src/activity_sharing.rs`,
//! `guild_folders.rs`, `messaging_permissions.rs`), the guild settings for
//! notifications (`server_actions.rs`, `channel_actions.rs`) and the notes
//! (`user_actions.rs`), all read 2026-10-17 and reimplemented. Each write is
//! one attempt: a `settings-proto` write reads first and sends
//! `required_data_version`, so it lands only on what it read, and what the
//! answer says is what is reported. Fixtures only: none of these routes has
//! been tried against Discord.

use reqwest::Method;
use rune_i18n::es;
use rune_model::{
    MessagingPrivacy, NotificationChange, NotifyLevel, Presence, ServerFolder, UserId,
    MUTED_FOREVER,
};
use rune_provider::{Command, Event};
use serde_json::{json, Value};

use super::{Http, RestError};
use crate::{
    account,
    settings_proto::{self, Own, Settings},
    LoginError, SessionSecret,
};

const SETTINGS: &str = "/users/@me/settings-proto/1";
const GUILD_SETTINGS_ANSWER: usize = 512 * 1024;
const NOTE_ANSWER: usize = 4 * 1024;

/// What the session last reported, for a write to build on.
#[derive(Clone)]
pub(crate) struct Known {
    pub me: UserId,
    /// The layout the interface was shown; a write needs Discord to still have it.
    pub folders: Option<Vec<ServerFolder>>,
    /// Where the account's status, as read or saved, is left for the session.
    pub own: std::sync::Arc<std::sync::Mutex<Option<Own>>>,
}

impl Known {
    fn keep(&self, own: &Own) {
        if let Ok(mut slot) = self.own.lock() {
            *slot = Some(own.clone());
        }
    }
}

fn unconfirmed() -> RestError {
    RestError::response(
        LoginError::new(es(
            "Discord no confirmó el cambio; se volvió a leer cómo quedó.",
        )),
        true,
    )
}

fn unreadable() -> RestError {
    RestError::response(
        LoginError::new(es(
            "Discord respondió con ajustes de la cuenta que RUNE no entiende.",
        )),
        false,
    )
}

/// The wire's `message_notifications` for `level`; 3 takes the one above.
fn level_number(level: Option<NotifyLevel>) -> u64 {
    match level {
        Some(NotifyLevel::All) => 0,
        Some(NotifyLevel::Mentions) => 1,
        Some(NotifyLevel::Nothing) => 2,
        None => 3,
    }
}

/// `muted` and its `mute_config`, as the official client and Serein send it:
/// a window in seconds and its end, or -1 and no end for good. Turning it
/// back on clears the config: a leftover window would keep it muted.
fn mute(until: Option<rune_model::Timestamp>, now: i64) -> Value {
    match until {
        None => json!({"muted": false, "mute_config": null}),
        Some(until) if until == MUTED_FOREVER => {
            json!({"muted": true, "mute_config": {"end_time": null, "selected_time_window": -1}})
        }
        Some(until) => {
            let end = chrono::DateTime::from_timestamp_millis(until.0)
                .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
            let window = ((until.0 - now) / 1000).max(1);
            json!({"muted": true, "mute_config": {"end_time": end, "selected_time_window": window}})
        }
    }
}

/// The body of a guild settings `PATCH` for `change`.
pub(crate) fn notification_body(change: NotificationChange, now: i64) -> Value {
    match change {
        NotificationChange::Level(level) => {
            json!({"message_notifications": level_number(Some(level))})
        }
        NotificationChange::Mute(until) => mute(until, now),
        NotificationChange::SuppressEveryone(on) => json!({"suppress_everyone": on}),
        NotificationChange::SuppressRoles(on) => json!({"suppress_roles": on}),
        NotificationChange::HideMuted(on) => json!({"hide_muted_channels": on}),
        NotificationChange::ChannelLevel(channel, level) => json!({"channel_overrides": {
            channel.to_string(): {"message_notifications": level_number(level)}
        }}),
        NotificationChange::ChannelMute(channel, until) => json!({"channel_overrides": {
            channel.to_string(): mute(until, now)
        }}),
    }
}

impl Http {
    async fn read_settings(&self, secret: &SessionSecret) -> Result<Settings, RestError> {
        let value = self
            .request(
                secret,
                Method::GET,
                SETTINGS,
                None,
                settings_proto::MAX_RESPONSE,
            )
            .await?;
        settings_proto::decode(&value).map_err(|_| unreadable())
    }

    /// Write `number`'s subtree over `read`, and read back what Discord kept.
    async fn write_settings(
        &self,
        secret: &SessionSecret,
        read: &Settings,
        number: u64,
        subtree: Vec<u8>,
    ) -> Result<Settings, RestError> {
        let body = settings_proto::patch(read, number, subtree).map_err(|_| {
            RestError::rejected(es("Ese cambio no entra en los ajustes de la cuenta."))
        })?;
        let value = self
            .request(
                secret,
                Method::PATCH,
                SETTINGS,
                Some(body),
                settings_proto::MAX_RESPONSE,
            )
            .await?;
        settings_proto::decode(&value).map_err(|_| unconfirmed())
    }

    pub(crate) async fn account(
        &self,
        secret: &SessionSecret,
        command: &Command,
        known: &Known,
    ) -> Result<Vec<Event>, RestError> {
        let now = crate::messages::wall_now().0;
        match command {
            Command::LoadAccountSettings => {
                let settings = self.read_settings(secret).await?;
                if let Ok(own) = settings.own(now) {
                    known.keep(&own);
                }
                Ok(settings_events(&settings, Some(known.me)))
            }
            Command::SetStatus(_) | Command::SetCustomStatus { .. } => {
                let read = self.read_settings(secret).await?;
                let current = read.own(now).map_err(|_| unreadable())?;
                let wanted = account::wanted(&current, command);
                let saved = if wanted == current {
                    current
                } else {
                    let subtree = read.with_own(&wanted).map_err(|_| unreadable())?;
                    let kept = self.write_settings(secret, &read, 11, subtree).await?;
                    let saved = kept.own(now).map_err(|_| unconfirmed())?;
                    // Its creation time and other fields are Discord's to set.
                    if saved.status != wanted.status || saved.line() != wanted.line() {
                        return Err(unconfirmed());
                    }
                    saved
                };
                known.keep(&saved);
                Ok(vec![presence(known.me, &saved)])
            }
            Command::SetServerFolders(folders) => {
                let read = self.read_settings(secret).await?;
                let current = read.folders().map_err(|_| unreadable())?;
                if known
                    .folders
                    .as_ref()
                    .is_some_and(|known| *known != current)
                {
                    return Err(RestError::rejected(
                        es("Las carpetas cambiaron en otro dispositivo. Ya se ven como quedaron: volvé a intentarlo."),
                    ));
                }
                let subtree = read
                    .with_folders(folders)
                    .map_err(|_| RestError::rejected(es("Esas carpetas no son válidas.")))?;
                let kept = self.write_settings(secret, &read, 14, subtree).await?;
                let saved = kept.folders().map_err(|_| unconfirmed())?;
                if saved != *folders {
                    return Err(unconfirmed());
                }
                Ok(vec![Event::ServerFolders(saved)])
            }
            Command::SetMessagingPrivacy(change) => {
                let read = self.read_settings(secret).await?;
                let mut wanted = read.privacy().map_err(|_| unreadable())?;
                wanted.apply(*change);
                if Some(&wanted) == read.privacy().ok().as_ref() {
                    return Ok(vec![Event::MessagingPrivacy(wanted)]);
                }
                let (number, subtree) = read.with_privacy(*change).map_err(|_| {
                    RestError::rejected(es("Ese cambio de privacidad no es válido."))
                })?;
                let kept = self.write_settings(secret, &read, number, subtree).await?;
                let saved: MessagingPrivacy = kept.privacy().map_err(|_| unconfirmed())?;
                if saved != wanted {
                    return Err(unconfirmed());
                }
                Ok(vec![Event::MessagingPrivacy(saved)])
            }
            Command::SetNotifications { server, change } => {
                let target = server.map_or_else(|| "@me".to_owned(), |server| server.to_string());
                let value = self
                    .request(
                        secret,
                        Method::PATCH,
                        &format!("/users/@me/guilds/{target}/settings"),
                        Some(notification_body(*change, now)),
                        GUILD_SETTINGS_ANSWER,
                    )
                    .await?;
                let settings = account::notifications(&value).map_err(|_| unconfirmed())?;
                if settings.server != *server || !account::confirms(&settings, *change) {
                    return Err(unconfirmed());
                }
                Ok(vec![Event::NotificationSettings {
                    settings: vec![settings],
                    replace: false,
                }])
            }
            Command::LoadNote { user } => {
                let value = self
                    .request(
                        secret,
                        Method::GET,
                        &format!("/users/@me/notes/{user}"),
                        None,
                        NOTE_ANSWER,
                    )
                    .await?;
                let note = account::note_of(&value["note"])
                    .map_err(|reason| RestError::response(reason, false))?;
                Ok(vec![Event::NoteChanged { user: *user, note }])
            }
            Command::SetNote { user, note } => {
                let note = note.trim();
                self.request_answer(
                    secret,
                    Method::PUT,
                    &format!("/users/@me/notes/{user}"),
                    Some(json!({ "note": note })),
                    NOTE_ANSWER,
                    true,
                )
                .await?;
                // `USER_NOTE_UPDATE` says it again; the answer has no body.
                Ok(vec![Event::NoteChanged {
                    user: *user,
                    note: note.to_owned(),
                }])
            }
            // Desktop session and games (escritorio-sesion-y-juegos): only
            // `show_current_game` of the status subtree, as Serein writes it.
            Command::SetActivitySharing(on) => {
                let read = self.read_settings(secret).await?;
                if read.shows_activity().map_err(|_| unreadable())? == *on {
                    return Ok(vec![Event::ActivitySharing(*on)]);
                }
                let subtree = read.with_activity_shown(*on).map_err(|_| unreadable())?;
                let kept = self.write_settings(secret, &read, 11, subtree).await?;
                let saved = kept.shows_activity().map_err(|_| unconfirmed())?;
                if saved != *on {
                    return Err(unconfirmed());
                }
                Ok(vec![Event::ActivitySharing(saved)])
            }
            _ => Err(RestError::rejected(es(
                "Esta acción todavía no está disponible.",
            ))),
        }
    }
}

/// The account's own presence as an event: its status and its line.
pub(crate) fn presence(me: UserId, own: &Own) -> Event {
    Event::PresenceChanged {
        user: me,
        presence: Presence {
            status: own.status,
            custom: own.line(),
            custom_emoji: own.picture(),
            activities: Vec::new(),
            platforms: Default::default(),
        },
    }
}

/// What `settings` says, each part that reads: the account's presence
/// (when `me` is given), its folders and its privacy.
pub(crate) fn settings_events(settings: &Settings, me: Option<UserId>) -> Vec<Event> {
    let now = crate::messages::wall_now().0;
    let mut events = Vec::new();
    if let (Some(me), Ok(own)) = (me, settings.own(now)) {
        events.push(presence(me, &own));
    }
    if let Ok(folders) = settings.folders() {
        events.push(Event::ServerFolders(folders));
    }
    if let Ok(privacy) = settings.privacy() {
        events.push(Event::MessagingPrivacy(privacy));
    }
    // Desktop session and games (escritorio-sesion-y-juegos).
    if let Ok(shown) = settings.shows_activity() {
        events.push(Event::ActivitySharing(shown));
    }
    events
}
