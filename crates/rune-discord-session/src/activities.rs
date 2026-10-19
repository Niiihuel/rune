//! Apps launched in voice channels (the service's embedded activities), as
//! the official client reads them (REA of its public web bundle,
//! 2026-10-18): a guild object's `activity_instances` in `GUILD_CREATE` and
//! `READY_SUPPLEMENTAL`, then each `EMBEDDED_ACTIVITY_UPDATE_V2`, which
//! replaces the launch it names and ends it with no participants. Only
//! fixtures, not tried against the real service. A malformed one is
//! decoration: it is dropped, never a disconnect.

use std::collections::HashSet;

use rune_model::{ChannelId, ChannelKind, UserId, VoiceActivity};
use rune_provider::{Event, Snapshot};
use serde_json::Value;

/// The most people of one launch kept: the hint shows six.
const PEOPLE_MAX: usize = 100;
/// The most launches of one guild read.
const LAUNCHES_MAX: usize = 50;

/// One launch, in a voice channel the account knows.
fn launch(snapshot: &Snapshot, value: &Value) -> Option<VoiceActivity> {
    let location = &value["location"];
    let channel = ChannelId(crate::wire::id(&location["channel_id"]).ok()?);
    snapshot
        .channels
        .iter()
        .find(|known| known.id == channel)
        .filter(|known| known.server.is_some() && known.kind == ChannelKind::Voice)?;
    let app = crate::wire::id(&value["application_id"]).ok()?;
    let mut seen = HashSet::new();
    let people = value["participants"]
        .as_array()?
        .iter()
        .take(PEOPLE_MAX)
        .filter_map(|row| crate::wire::id(&row["user_id"]).ok().map(UserId))
        .filter(|user| seen.insert(*user))
        .collect();
    // As the official client keys them: one app per place.
    Some(VoiceActivity {
        id: format!("{channel}:{app}"),
        channel,
        people,
    })
}

/// The launches of a guild object.
pub(crate) fn of_guild(snapshot: &Snapshot, guild: &Value) -> Vec<Event> {
    guild["activity_instances"]
        .as_array()
        .into_iter()
        .flatten()
        .take(LAUNCHES_MAX)
        .filter_map(|value| launch(snapshot, value))
        .filter(|activity| !activity.people.is_empty())
        .map(Event::VoiceActivity)
        .collect()
}

/// The launches of each guild of a `READY_SUPPLEMENTAL`.
pub(crate) fn supplemental(snapshot: &Snapshot, data: &Value) -> Vec<Event> {
    data["guilds"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|guild| of_guild(snapshot, guild))
        .collect()
}

/// An `EMBEDDED_ACTIVITY_UPDATE_V2`.
pub(crate) fn update(snapshot: &Snapshot, data: &Value) -> Option<Event> {
    launch(snapshot, data).map(Event::VoiceActivity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{Channel, ServerId};
    use serde_json::json;

    fn snapshot() -> Snapshot {
        let channel = |id: u64, kind| Channel {
            id: ChannelId(id),
            server: Some(ServerId(1)),
            kind,
            name: "c".into(),
            topic: None,
            topic_pictures: Vec::new(),
            parent: None,
            position: 0,
            recipients: Vec::new(),
            owner: None,
            permissions: rune_model::Permissions::VIEW,
            overwrites: Vec::new(),
            slowmode_secs: 0,
            nsfw: false,
            icon: None,
            thread: None,
            tags: Vec::new(),
            forum: Default::default(),
        };
        Snapshot {
            channels: vec![
                channel(40, ChannelKind::Voice),
                channel(41, ChannelKind::Text),
            ],
            ..Snapshot::default()
        }
    }

    fn instance(channel: &str, users: &[&str]) -> Value {
        json!({
            "application_id": "880218394199220334",
            "launch_id": "1",
            "composite_instance_id": "i-1",
            "location": {"id": format!("gc-1-{channel}"), "kind": "gc",
                "channel_id": channel, "guild_id": "1"},
            "participants": users.iter().map(|user| json!({"user_id": user, "session_id": "s"}))
                .collect::<Vec<_>>(),
        })
    }

    /// What else the rail reads of a guild from READY: its members' default
    /// level and its idle channel, before its settings are ever opened.
    #[test]
    fn ready_reads_a_server_s_default_level_and_idle_channel() {
        let me = crate::wire::user(&json!({"id": "42", "username": "me"})).unwrap();
        let ready = |properties: Value| {
            crate::bootstrap::ready(
                &json!({
                    "user": {"id": "42"},
                    "guilds": [{"id": "10", "properties": properties,
                        "roles": [{"id": "10", "name": "@everyone", "permissions": "0"}],
                        "channels": [{"id": "40", "type": 2, "name": "afk",
                            "permission_overwrites": []}]}],
                }),
                &me,
            )
            .unwrap()
            .servers[0]
                .options
                .clone()
        };
        let options = ready(json!({"name": "F", "default_message_notifications": 1,
            "afk_channel_id": "40"}));
        assert!(options.mentions_only);
        assert_eq!(options.afk_channel, Some(ChannelId(40)));
        let options = ready(json!({"name": "F", "default_message_notifications": 0}));
        assert!(!options.mentions_only && options.afk_channel.is_none());
    }

    #[test]
    fn launches_are_read_from_guilds_and_updates_and_end_with_nobody() {
        let snapshot = snapshot();
        let guild = json!({"id": "1", "activity_instances": [
            instance("40", &["7", "8", "7"]),
            // Not a voice channel the account knows: left out.
            instance("41", &["9"]),
            instance("99", &["9"]),
            {"location": "nonsense"},
        ]});
        let events = of_guild(&snapshot, &guild);
        assert_eq!(
            events,
            [Event::VoiceActivity(VoiceActivity {
                id: "40:880218394199220334".into(),
                channel: ChannelId(40),
                people: vec![UserId(7), UserId(8)],
            })]
        );
        assert_eq!(supplemental(&snapshot, &json!({"guilds": [guild]})), events);
        let ended = update(&snapshot, &instance("40", &[])).unwrap();
        assert!(
            matches!(ended, Event::VoiceActivity(found) if found.people.is_empty()
            && found.id == "40:880218394199220334")
        );
        assert!(update(&snapshot, &json!({"participants": []})).is_none());
    }
}
