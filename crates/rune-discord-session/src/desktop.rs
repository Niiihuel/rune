//! What this device says the person is doing (P6, 2026-10-17): another
//! program's Rich Presence or a game recognized by its process, published
//! as an activity of the presence update (op 3), as Serein does
//! (`discord-gateway/src/activity.rs`, `apps/desktop/src/game_activity.rs`,
//! read 2026-10-17). A program names only its application: its name and
//! the keys of its pictures are looked up first (`http::desktop`). And the
//! games Discord recognizes, from its public list. Fixtures only.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rune_model::{ActivityVisibility, DetectableGame, LocalActivity};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::time::Instant;

/// The largest activity sent, as Serein bounds it; past it the pictures go.
pub(crate) const MAX_ACTIVITY: usize = 3072;
/// An application's picture keys are asked for again at most this often.
pub(crate) const ASSETS_AGAIN: Duration = Duration::from_secs(60);
/// Addresses turned into Discord's own media paths, remembered.
const EXTERNAL_KEPT: usize = 8;
/// What a game of the list keeps, as Serein.
const EXECUTABLES: usize = 16;
const MAX_GAMES: usize = 50_000;

/// The session's lookups and the last activity resolved by them.
#[derive(Clone, Default)]
pub(crate) struct LocalPresence {
    pub(crate) cache: Arc<Mutex<Cache>>,
    resolved: Arc<Mutex<Option<Value>>>,
}

#[derive(Default)]
pub(crate) struct Cache {
    /// Applications' names, once read.
    pub(crate) names: HashMap<u64, String>,
    /// Each application's picture keys (`id`, `name`), and when they were read.
    pub(crate) assets: HashMap<u64, (Instant, Vec<(String, String)>)>,
    /// Addresses and the `mp:` path Discord gave each.
    pub(crate) external: VecDeque<(String, String)>,
}

impl Cache {
    pub(crate) fn remember_external(&mut self, url: String, path: String) {
        self.external.retain(|(known, _)| *known != url);
        self.external.push_back((url, path));
        if self.external.len() > EXTERNAL_KEPT {
            self.external.pop_front();
        }
    }
}

impl LocalPresence {
    pub(crate) fn resolved(&self, value: Value) {
        if let Ok(mut resolved) = self.resolved.lock() {
            *resolved = Some(value);
        }
    }

    /// The activity a finished lookup resolved, once.
    pub(crate) fn take(&self) -> Option<Value> {
        self.resolved
            .lock()
            .ok()
            .and_then(|mut resolved| resolved.take())
    }
}

/// The activity as op 3 carries it, when nothing has to be looked up: it
/// has a name of its own and its pictures, if any, are `mp:` paths.
pub(crate) fn ready(activity: &LocalActivity) -> Option<Value> {
    let name = activity.name.as_deref()?;
    let lookup = [&activity.large_image, &activity.small_image]
        .into_iter()
        .flatten()
        .any(|image| !image.starts_with("mp:"));
    if lookup {
        return None;
    }
    build(activity, name, |image| Some(image.to_owned()))
}

/// The activity named `name`, with each picture `image` resolves; one that
/// does not resolve goes with its text. `None` when even without pictures
/// it is too large.
pub(crate) fn build(
    activity: &LocalActivity,
    name: &str,
    image: impl Fn(&str) -> Option<String>,
) -> Option<Value> {
    let mut value = json!({"name": name, "type": activity.kind.number()});
    if let Some(application) = activity.application {
        value["application_id"] = json!(application.to_string());
    }
    if let Some(details) = &activity.details {
        value["details"] = json!(details);
    }
    if let Some(state) = &activity.state {
        value["state"] = json!(state);
    }
    let mut timestamps = json!({});
    if let Some(start) = activity.start {
        timestamps["start"] = json!(start);
    }
    if let Some(end) = activity.end {
        timestamps["end"] = json!(end);
    }
    if timestamps != json!({}) {
        value["timestamps"] = timestamps;
    }
    let mut assets = json!({});
    for (picture, text, key) in [
        (&activity.large_image, &activity.large_text, "large"),
        (&activity.small_image, &activity.small_text, "small"),
    ] {
        if let Some(resolved) = picture.as_deref().and_then(&image) {
            assets[format!("{key}_image")] = json!(resolved);
            if let Some(text) = text {
                assets[format!("{key}_text")] = json!(text);
            }
        }
    }
    if assets != json!({}) {
        value["assets"] = assets;
    }
    let size = |value: &Value| serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len());
    if size(&value) > MAX_ACTIVITY {
        if let Some(object) = value.as_object_mut() {
            object.remove("assets");
        }
    }
    (size(&value) <= MAX_ACTIVITY).then_some(value)
}

// Desktop session and games (escritorio-sesion-y-juegos).
/// Sessions and activities read from one `SESSIONS_REPLACE`, at most, as
/// Serein bounds them.
const SESSIONS_READ: usize = 16;

/// Whether the account's sessions, as `SESSIONS_REPLACE` lists them, show
/// `published` (the activity op 3 carries), as Serein reads them
/// (`discord-protocol/src/activity_sessions.rs`, 2026-10-18): the summary
/// of every session (`all`) lists it or hides it; with no summary, the one
/// session there is this one. `None` for a list that does not read.
pub(crate) fn visibility(sessions: &Value, published: &Value) -> Option<ActivityVisibility> {
    let sessions = sessions
        .as_array()
        .filter(|list| list.len() <= SESSIONS_READ)?;
    let all = sessions
        .iter()
        .find(|session| session["session_id"] == "all");
    let (session, summary) = match (all, sessions.as_slice()) {
        (Some(all), _) => (all, true),
        (None, [only]) => (only, false),
        (None, _) => return Some(ActivityVisibility::Unconfirmed),
    };
    let kind = published["type"].as_u64();
    let application = published["application_id"].as_str();
    let name = published["name"].as_str();
    // One the account added has no application: it is known by its name.
    let lists = |key: &str| -> Option<bool> {
        let activities = session[key].as_array()?;
        Some(activities.iter().take(SESSIONS_READ).any(|activity| {
            let theirs = activity["application_id"].as_str().filter(|id| *id != "0");
            activity["type"].as_u64() == kind
                && match application {
                    Some(application) => theirs == Some(application),
                    None => theirs.is_none() && activity["name"].as_str() == name,
                }
        }))
    };
    Some(match (lists("activities"), lists("hidden_activities")) {
        (Some(true), _) if summary => ActivityVisibility::Listed,
        (Some(true), _) => ActivityVisibility::Received,
        (_, Some(true)) => ActivityVisibility::Hidden,
        (Some(false), _) if summary => ActivityVisibility::Missing,
        _ => ActivityVisibility::Unconfirmed,
    })
}

/// An application's name as `/applications/{id}/rpc` answers it.
pub(crate) fn application_name(id: u64, answer: &Value) -> Option<String> {
    let name = answer["name"].as_str()?.trim();
    (answer["id"].as_str() == Some(id.to_string().as_str())
        && !name.is_empty()
        && name.chars().count() <= 128
        && !name.chars().any(char::is_control))
    .then(|| name.to_owned())
}

/// The picture keys `/oauth2/applications/{id}/assets` lists.
pub(crate) fn asset_keys(answer: &Value) -> Vec<(String, String)> {
    answer
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|asset| {
            let id = asset["id"].as_str()?;
            let name = asset["name"].as_str()?;
            (id.bytes().all(|byte| byte.is_ascii_digit()) && !id.is_empty())
                .then(|| (id.to_owned(), name.to_lowercase()))
        })
        .take(1024)
        .collect()
}

/// A path Discord's media proxy gave, made `mp:`; `None` when it is odd.
pub(crate) fn media_path(path: &str) -> Option<String> {
    (!path.is_empty() && path.len() <= 512 && path.bytes().all(|byte| byte.is_ascii_graphic()))
        .then(|| format!("mp:{}", path.trim_start_matches('/')))
}

#[derive(Deserialize)]
struct Detectable {
    id: String,
    name: String,
    #[serde(default)]
    executables: Option<Vec<Executable>>,
}

#[derive(Deserialize)]
struct Executable {
    name: String,
    #[serde(default)]
    is_launcher: bool,
}

/// The games of Discord's public list (`/applications/detectable`) by
/// their programs, launchers left out as Serein does; `None` when it does
/// not read. Run it off the runtime: the list is large.
pub(crate) fn detectable(bytes: &[u8]) -> Option<Vec<DetectableGame>> {
    let listed: Vec<Detectable> = serde_json::from_slice(bytes).ok()?;
    Some(
        listed
            .into_iter()
            .filter_map(|game| {
                let name = game.name.trim();
                if name.is_empty()
                    || name.chars().count() > 128
                    || name.chars().any(char::is_control)
                {
                    return None;
                }
                let executables: Vec<String> = game
                    .executables
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|executable| {
                        !executable.is_launcher
                            && !executable.name.trim().is_empty()
                            && executable.name.len() <= 256
                    })
                    .map(|executable| executable.name)
                    .take(EXECUTABLES)
                    .collect();
                (!executables.is_empty()).then(|| DetectableGame {
                    application: game.id.parse().ok(),
                    name: name.to_owned(),
                    executables,
                })
            })
            .take(MAX_GAMES)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{ActivitySource, LocalActivityKind};

    fn program() -> LocalActivity {
        LocalActivity {
            application: Some(55),
            kind: LocalActivityKind::Listening,
            name: Some("Reproductor".into()),
            details: Some("Tema".into()),
            state: None,
            start: Some(1_000),
            end: None,
            large_image: Some("mp:external/abc".into()),
            large_text: Some("Disco".into()),
            small_image: None,
            small_text: Some("sin imagen".into()),
            source: ActivitySource::Program,
        }
    }

    #[test]
    fn an_activity_with_a_name_and_media_paths_needs_no_lookup() {
        assert_eq!(
            ready(&program()),
            Some(json!({
                "name": "Reproductor", "type": 2, "application_id": "55",
                "details": "Tema", "timestamps": {"start": 1000},
                "assets": {"large_image": "mp:external/abc", "large_text": "Disco"},
            })),
            "a text without its picture goes"
        );
        let mut keyed = program();
        keyed.large_image = Some("cover".into());
        assert_eq!(ready(&keyed), None, "a key is looked up");
        keyed.name = None;
        keyed.large_image = None;
        assert_eq!(ready(&keyed), None, "the name is looked up");
    }

    #[test]
    fn a_large_activity_loses_its_pictures_first() {
        let mut big = program();
        big.large_text = Some("x".repeat(MAX_ACTIVITY));
        let built = ready(&big).unwrap();
        assert!(built.get("assets").is_none());
        big.details = Some("y".repeat(MAX_ACTIVITY));
        assert_eq!(ready(&big), None);
    }

    #[test]
    fn answers_are_checked() {
        assert_eq!(
            application_name(5, &json!({"id": "5", "name": " Juego "})).as_deref(),
            Some("Juego")
        );
        assert_eq!(
            application_name(5, &json!({"id": "6", "name": "Otro"})),
            None
        );
        assert_eq!(
            asset_keys(&json!([{"id": "9", "name": "Cover"}, {"id": "x", "name": "bad"}])),
            [("9".to_owned(), "cover".to_owned())]
        );
        assert_eq!(
            media_path("external/a/b").as_deref(),
            Some("mp:external/a/b")
        );
        assert_eq!(media_path("has space"), None);
        let list = br#"[{"id": "1", "name": "Juego", "executables": [
            {"name": "launcher.exe", "is_launcher": true}, {"name": "juego.exe", "os": "win32"}]},
            {"id": "2", "name": "Sin programas", "executables": null},
            {"id": "3", "name": "Solo lanzador", "executables": [{"name": "l.exe", "is_launcher": true}]}]"#;
        assert_eq!(
            detectable(list),
            Some(vec![DetectableGame {
                application: Some(1),
                name: "Juego".into(),
                executables: vec!["juego.exe".into()],
            }])
        );
        assert_eq!(detectable(b"{"), None);
    }

    // Desktop session and games (escritorio-sesion-y-juegos).
    #[test]
    fn the_sessions_say_whether_the_game_is_listed_hidden_or_missing() {
        use ActivityVisibility::*;
        let game = json!({"name": "Juego", "type": 0, "application_id": "42"});
        let theirs = json!({"name": "Juego", "type": 0, "application_id": "42"});
        let seen = |sessions: Value| visibility(&sessions, &game);
        assert_eq!(
            seen(json!([{"session_id": "abc", "activities": [theirs]}])),
            Some(Received)
        );
        assert_eq!(
            seen(json!([{"session_id": "all", "activities": [theirs], "hidden_activities": []}])),
            Some(Listed)
        );
        assert_eq!(
            seen(json!([{"session_id": "all", "activities": [], "hidden_activities": [theirs]}])),
            Some(Hidden)
        );
        assert_eq!(
            seen(json!([{"session_id": "abc", "activities": [theirs]},
                        {"session_id": "all", "activities": []}])),
            Some(Missing)
        );
        assert_eq!(seen(json!([{"session_id": "all"}])), Some(Unconfirmed));
        assert_eq!(
            seen(json!([{"session_id": "a"}, {"session_id": "b"}])),
            Some(Unconfirmed),
            "two sessions and no summary: which one is this is not known"
        );
        assert_eq!(seen(json!({"session_id": "all"})), None);
        assert_eq!(seen(json!(vec![json!({}); 17])), None);
        // A game the person added has no application: found by its name.
        let added = json!({"name": "Mi juego", "type": 0});
        assert_eq!(
            visibility(
                &json!([{"session_id": "all", "activities": [{"name": "Mi juego", "type": 0}]}]),
                &added
            ),
            Some(Listed)
        );
        assert_eq!(
            visibility(
                &json!([{"session_id": "all", "activities": [{"name": "Mi juego", "type": 0, "application_id": "9"}]}]),
                &added
            ),
            Some(Missing)
        );
    }
}
