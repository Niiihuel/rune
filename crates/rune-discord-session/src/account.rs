//! What the account keeps with Discord beside its profile, as the session
//! reads it: notifications from READY's `user_guild_settings` and
//! `USER_GUILD_SETTINGS_UPDATE`, notes from `USER_NOTE_UPDATE`, the status
//! of all its sessions from `SESSIONS_REPLACE`, and `USER_SETTINGS_PROTO_UPDATE`
//! as the sign to read `settings-proto/1` again (Serein does the same). Also
//! the checks before a write about them; `http::account` sends it.
//!
//! These rows are untrusted and optional to the account: one that cannot be
//! read is dropped without failing the session.

use rune_i18n::es;
use rune_model::{
    account::{valid_folders, valid_note},
    ChannelKind, ChannelNotifications, NotificationChange, Notifications, NotifyLevel, Presence,
    PrivacyChange, ServerFolder, ServerId, Status, Timestamp, UserId, MUTED_FOREVER,
};
use rune_provider::{Command, Event, Snapshot};
use serde_json::Value;

use crate::{
    settings_proto::{self, Custom, Own},
    wire, LoginError,
};

/// Channels set apart in one server, at most; Serein reads past 4000.
const OVERRIDES_MAX: usize = 10_000;
/// Servers' notification settings read from READY, at most.
const ENTRIES_MAX: usize = 4_096;

/// What the session keeps of the account's settings.
#[derive(Default)]
pub(crate) struct Account {
    /// Its status and custom status as last read or saved: what this
    /// session tells the Gateway. `None` until read.
    pub own: Option<Own>,
    /// What a settings read or a status write filled in, for the session to
    /// take once the request is answered.
    pub read: std::sync::Arc<std::sync::Mutex<Option<Own>>>,
    /// The layout last reported: a write needs Discord to still have it.
    pub folders: Option<Vec<ServerFolder>>,
    /// Read the settings again: a new connection, or another device changed them.
    pub stale: bool,
    /// The status every session shows, as last reported.
    shown: Option<Status>,
}

impl Account {
    /// Keep what a confirmed answer or a read says.
    pub fn observe(&mut self, event: &Event) {
        match event {
            Event::ServerFolders(folders) => self.folders = Some(folders.clone()),
            Event::PresenceChanged { presence, .. } => self.shown = Some(presence.status),
            _ => {}
        }
        if let Some(own) = self.read.lock().ok().and_then(|mut read| read.take()) {
            self.own = Some(own);
        }
    }

    /// What the Gateway is told: the account's own presence, or invisible
    /// until it is read, so an invisible account never shows online.
    pub fn told(&self) -> Own {
        self.own.clone().unwrap_or(Own {
            status: Status::Invisible,
            custom: None,
        })
    }
}

/// Whether `command` is about what the account keeps with Discord.
pub(crate) fn is_account_command(command: &Command) -> bool {
    matches!(
        command,
        Command::LoadAccountSettings
            | Command::SetStatus(_)
            | Command::SetCustomStatus { .. }
            | Command::SetNotifications { .. }
            | Command::SetServerFolders(_)
            | Command::SetMessagingPrivacy(_)
            | Command::LoadNote { .. }
            | Command::SetNote { .. }
            // Desktop session and games (escritorio-sesion-y-juegos).
            | Command::SetActivitySharing(_)
    )
}

/// The status and custom status `command` asks for, over `current`.
pub(crate) fn wanted(current: &Own, command: &Command) -> Own {
    let mut own = current.clone();
    match command {
        Command::SetStatus(status) => own.status = *status,
        Command::SetCustomStatus { text, emoji, until } => {
            let text = text.as_deref().map(str::trim).unwrap_or_default();
            let emoji = emoji.clone().filter(|emoji| !emoji.is_empty());
            own.custom = (!text.is_empty() || emoji.is_some()).then(|| Custom {
                text: text.to_owned(),
                emoji,
                server_emoji: None,
                expires: until.map(|until| until.0),
            });
        }
        _ => {}
    }
    own
}

/// Whether `command`, about what the account keeps, can go out.
pub(crate) fn allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    let refuse = |reason: &'static str| Err(LoginError::new(reason));
    let server_known = |server: ServerId| snapshot.servers.iter().any(|known| known.id == server);
    match command {
        Command::SetCustomStatus { text, emoji, until } => {
            let text = text.as_deref().map(str::trim).unwrap_or_default();
            if text.chars().count() > settings_proto::CUSTOM_MAX
                || text.chars().any(char::is_control)
            {
                return refuse(es(
                    "El estado puede tener hasta 128 caracteres, sin saltos de línea.",
                ));
            }
            if emoji
                .as_deref()
                .is_some_and(|emoji| emoji.len() > 64 || emoji.chars().any(char::is_control))
                || until.is_some_and(|until| until.0 <= 0)
            {
                return Err(wire::invalid());
            }
            Ok(())
        }
        Command::SetNotifications { server, change } => {
            if server.is_some_and(|server| !server_known(server)) {
                return refuse(es("Ya no estás en ese servidor."));
            }
            if let NotificationChange::ChannelLevel(channel, _)
            | NotificationChange::ChannelMute(channel, _) = change
            {
                let fits = snapshot.channels.iter().any(|known| {
                    known.id == *channel
                        && match server {
                            Some(_) => known.server == *server,
                            None => matches!(known.kind, ChannelKind::Direct | ChannelKind::Group),
                        }
                });
                if !fits {
                    return refuse(es("Ese canal ya no está en tu lista."));
                }
            }
            if let NotificationChange::Mute(Some(until))
            | NotificationChange::ChannelMute(_, Some(until)) = change
            {
                if until.0 <= 0 {
                    return Err(wire::invalid());
                }
            }
            Ok(())
        }
        Command::SetServerFolders(folders) => {
            if valid_folders(folders) {
                Ok(())
            } else {
                refuse(es("Esas carpetas no son válidas."))
            }
        }
        Command::SetMessagingPrivacy(
            PrivacyChange::ServerDmsIn(server, _) | PrivacyChange::RequestFilterIn(server, _),
        ) if !server_known(*server) => refuse(es("Ya no estás en ese servidor.")),
        Command::SetNote { user, note } if user.0 == 0 || !valid_note(note.trim()) => {
            refuse(es("La nota puede tener hasta 256 caracteres."))
        }
        Command::LoadNote { user } if user.0 == 0 => Err(wire::invalid()),
        _ => Ok(()),
    }
}

fn level(value: &Value) -> Option<NotifyLevel> {
    match value.as_u64() {
        Some(0) => Some(NotifyLevel::All),
        Some(1) => Some(NotifyLevel::Mentions),
        Some(2) => Some(NotifyLevel::Nothing),
        // 3, null or absent: the one above.
        _ => None,
    }
}

/// Until when something is muted: for good without an end, never when not
/// muted. An end already past reads as not muted wherever it is shown.
fn muted_until(muted: &Value, config: &Value) -> Result<Option<Timestamp>, LoginError> {
    if muted.as_bool() != Some(true) {
        return Ok(None);
    }
    match &config["end_time"] {
        Value::Null => Ok(Some(MUTED_FOREVER)),
        end => wire::timestamp(end).map(Some),
    }
}

/// One server's settings, or the direct conversations' (`guild_id: null`),
/// as READY, `USER_GUILD_SETTINGS_UPDATE` and a `PATCH` answer give them.
pub(crate) fn notifications(value: &Value) -> Result<Notifications, LoginError> {
    let server = match &value["guild_id"] {
        Value::Null => None,
        id => Some(ServerId(wire::id(id)?)),
    };
    let flag = |name: &str| value[name].as_bool().unwrap_or(false);
    let overrides: Vec<&Value> = match &value["channel_overrides"] {
        Value::Array(rows) => rows.iter().collect(),
        Value::Object(rows) => rows.values().collect(),
        Value::Null => Vec::new(),
        _ => return Err(wire::invalid()),
    };
    if overrides.len() > OVERRIDES_MAX {
        return Err(wire::invalid());
    }
    let mut channels = Vec::new();
    for row in overrides {
        let entry = ChannelNotifications {
            channel: rune_model::ChannelId(wire::id(&row["channel_id"])?),
            level: level(&row["message_notifications"]),
            muted_until: muted_until(&row["muted"], &row["mute_config"])?,
        };
        if entry.level.is_some() || entry.muted_until.is_some() {
            channels.retain(|kept: &ChannelNotifications| kept.channel != entry.channel);
            channels.push(entry);
        }
    }
    Ok(Notifications {
        server,
        level: level(&value["message_notifications"]),
        muted_until: muted_until(&value["muted"], &value["mute_config"])?,
        suppress_everyone: flag("suppress_everyone"),
        suppress_roles: flag("suppress_roles"),
        hide_muted: flag("hide_muted_channels"),
        mobile_push: value["mobile_push"].as_bool().unwrap_or(true),
        channels,
    })
}

/// Whether `settings`, the answer to a write of `change`, shows it done.
pub(crate) fn confirms(settings: &Notifications, change: NotificationChange) -> bool {
    let mut wanted = settings.clone();
    wanted.apply(change);
    // Discord dates an end to the millisecond it keeps; only whether it is
    // muted, and for good, has to match.
    let same_mute = |a: Option<Timestamp>, b: Option<Timestamp>| {
        a.is_some() == b.is_some() && (a == Some(MUTED_FOREVER)) == (b == Some(MUTED_FOREVER))
    };
    match change {
        NotificationChange::Mute(until) => same_mute(settings.muted_until, until),
        NotificationChange::ChannelMute(channel, until) => {
            same_mute(settings.channel(channel).muted_until, until)
        }
        _ => wanted == *settings,
    }
}

/// READY's `user_guild_settings`: every server's, replacing what was known.
/// Either `{entries, partial, version}` or the older bare list.
pub(crate) fn ready(data: &Value) -> Option<Event> {
    let settings = &data["user_guild_settings"];
    let entries = settings["entries"].as_array().or(settings.as_array())?;
    let settings: Vec<Notifications> = entries
        .iter()
        .take(ENTRIES_MAX)
        .filter_map(|entry| notifications(entry).ok())
        .collect();
    Some(Event::NotificationSettings {
        // A partial list says only what changed.
        replace: data["user_guild_settings"]["partial"].as_bool() != Some(true),
        settings,
    })
}

/// A note's text, bounded as Discord keeps it; `null` is none.
pub(crate) fn note_of(value: &Value) -> Result<String, LoginError> {
    match value {
        Value::Null => Ok(String::new()),
        Value::String(note) if valid_note(note) => Ok(note.clone()),
        _ => Err(wire::invalid()),
    }
}

/// The status every session of the account shows, from `SESSIONS_REPLACE`:
/// the one Discord sums up as `all`, else the first known.
fn effective_status(data: &Value) -> Option<Status> {
    let sessions = data.as_array()?;
    let status = |session: &Value| settings_proto::status_of(session["status"].as_str()?);
    sessions
        .iter()
        .find(|session| session["session_id"] == "all")
        .and_then(status)
        .or_else(|| sessions.iter().find_map(status))
}

/// What a dispatch about the account's settings changed; `None` for others.
pub(crate) fn dispatch(
    account: &mut Account,
    snapshot: &Snapshot,
    kind: &str,
    data: &Value,
) -> Option<Vec<Event>> {
    Some(match kind {
        "USER_GUILD_SETTINGS_UPDATE" => notifications(data)
            .map(|settings| Event::NotificationSettings {
                settings: vec![settings],
                replace: false,
            })
            .into_iter()
            .collect(),
        "USER_NOTE_UPDATE" => {
            let user = wire::id(&data["id"]).ok().map(UserId);
            let note = note_of(&data["note"]).ok();
            user.zip(note)
                .map(|(user, note)| Event::NoteChanged { user, note })
                .into_iter()
                .collect()
        }
        // Status and folders live here; notifications do not.
        "USER_SETTINGS_PROTO_UPDATE" => {
            if data["settings"]["type"].as_u64() == Some(1) {
                account.stale = true;
            }
            Vec::new()
        }
        // What others see, which other sessions may set; what this one
        // tells the Gateway stays.
        "SESSIONS_REPLACE" => {
            let me = snapshot.me.as_ref()?.id;
            let status = effective_status(data)?;
            if account.shown == Some(status) {
                return Some(Vec::new());
            }
            account.shown = Some(status);
            vec![Event::PresenceChanged {
                user: me,
                presence: Presence {
                    status,
                    custom: account.own.as_ref().and_then(Own::line),
                    custom_emoji: account.own.as_ref().and_then(Own::picture),
                    activities: Vec::new(),
                    platforms: Default::default(),
                },
            }]
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn guild_settings_read_levels_mutes_and_channels() {
        let settings = notifications(&json!({
            "guild_id": "7", "muted": true, "mute_config": {"end_time": null},
            "message_notifications": 1, "suppress_everyone": true,
            "channel_overrides": [
                {"channel_id": "70", "muted": true,
                 "mute_config": {"end_time": "2026-10-17T18:30:00.000Z", "selected_time_window": 3600},
                 "message_notifications": 3},
                {"channel_id": "71", "muted": false, "message_notifications": 2},
                {"channel_id": "72", "muted": false, "message_notifications": 3}
            ]
        }))
        .unwrap();
        assert_eq!(settings.server, Some(ServerId(7)));
        assert_eq!(settings.muted_until, Some(MUTED_FOREVER));
        assert_eq!(settings.level, Some(NotifyLevel::Mentions));
        assert!(settings.suppress_everyone && !settings.hide_muted);
        assert!(
            settings.mobile_push,
            "pushes are on unless Discord says off"
        );
        assert_eq!(settings.channels.len(), 2, "nothing of its own is dropped");
        assert!(settings.channels[0]
            .muted_until
            .is_some_and(|until| until != MUTED_FOREVER));
        assert_eq!(settings.channels[1].level, Some(NotifyLevel::Nothing));
        let direct = notifications(&json!({"guild_id": null, "channel_overrides": {}})).unwrap();
        assert_eq!(direct.server, None);
        assert!(notifications(&json!({"guild_id": "x"})).is_err());
    }

    #[test]
    fn ready_replaces_unless_partial_and_skips_broken_rows() {
        let Some(Event::NotificationSettings { settings, replace }) = ready(&json!({
            "user_guild_settings": {"entries": [{"guild_id": "1"}, {"guild_id": []}], "partial": false}
        })) else {
            panic!()
        };
        assert!(replace);
        assert_eq!(settings.len(), 1);
        assert!(matches!(
            ready(&json!({"user_guild_settings": [{"guild_id": "2"}]})),
            Some(Event::NotificationSettings { replace: true, .. })
        ));
        assert!(ready(&json!({})).is_none());
    }

    #[test]
    fn sessions_replace_reports_the_status_others_see() {
        let snapshot = Snapshot {
            me: Some(rune_model::User {
                identity: Default::default(),
                id: UserId(1),
                name: "yo".into(),
                display_name: None,
                accent: rune_model::Rgb(0),
                bot: false,
                pronouns: None,
                about: None,
                avatar: None,
                banner: Default::default(),
                default_avatar: None,
            }),
            ..Snapshot::default()
        };
        let mut account = Account::default();
        let events = dispatch(
            &mut account,
            &snapshot,
            "SESSIONS_REPLACE",
            &json!([{"session_id": "abc", "status": "online"}, {"session_id": "all", "status": "dnd"}]),
        )
        .unwrap();
        assert!(
            matches!(&events[..], [Event::PresenceChanged { presence, .. }]
            if presence.status == Status::DoNotDisturb)
        );
        // The same again says nothing.
        assert!(dispatch(
            &mut account,
            &snapshot,
            "SESSIONS_REPLACE",
            &json!([{"session_id": "all", "status": "dnd"}])
        )
        .unwrap()
        .is_empty());
        dispatch(
            &mut account,
            &snapshot,
            "USER_SETTINGS_PROTO_UPDATE",
            &json!({"settings": {"type": 1, "proto": ""}, "partial": true}),
        );
        assert!(account.stale);
        assert!(matches!(
            &dispatch(&mut account, &snapshot, "USER_NOTE_UPDATE", &json!({"id": "5", "note": "hola"})).unwrap()[..],
            [Event::NoteChanged { user: UserId(5), note }] if note == "hola"
        ));
    }

    #[test]
    fn writes_are_checked_before_they_go() {
        let snapshot = Snapshot::default();
        assert!(allowed(
            &snapshot,
            &Command::SetNotifications {
                server: Some(ServerId(9)),
                change: NotificationChange::Mute(None)
            }
        )
        .is_err());
        assert!(allowed(
            &snapshot,
            &Command::SetCustomStatus {
                text: Some("x".repeat(129)),
                emoji: None,
                until: None
            }
        )
        .is_err());
        assert!(allowed(
            &snapshot,
            &Command::SetServerFolders(vec![
                ServerFolder::single(ServerId(1)),
                ServerFolder::single(ServerId(1))
            ])
        )
        .is_err());
        assert!(allowed(
            &snapshot,
            &Command::SetNote {
                user: UserId(3),
                note: "x".repeat(257)
            }
        )
        .is_err());
        assert!(allowed(&snapshot, &Command::LoadAccountSettings).is_ok());
    }
}
