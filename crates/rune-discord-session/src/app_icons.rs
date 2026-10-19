//! The picture of an activity that brings none (2026-10-17): its
//! application's icon, read from `GET /applications/{id}/public` as the
//! official client does (REA, `APPLICATION_PUBLIC`, `getIconURL`). One read
//! at a time, kept for the session, a failure too: the official client
//! does not ask again for an application that failed. Fixtures only.

use std::collections::{HashMap, VecDeque};

use rune_model::{Presence, RemoteImage, UserId};
use rune_provider::Event;
use serde_json::Value;

/// Applications kept for the session; past it the oldest are asked again.
const KNOWN: usize = 512;
/// Applications waiting to be read.
const QUEUED: usize = 64;
/// People whose presence waits for an application's icon.
const WAITING: usize = 256;
/// The most an application's public object takes.
pub(crate) const MAX_RESPONSE: usize = 256 * 1024;

#[derive(Default)]
pub(crate) struct AppIcons {
    known: HashMap<u64, Option<RemoteImage>>,
    /// In the order they were learned, for forgetting the oldest.
    order: VecDeque<u64>,
    queue: VecDeque<u64>,
    asking: Option<u64>,
    /// The latest presence of each person waiting, to tell again with it.
    waiting: HashMap<UserId, Presence>,
}

impl AppIcons {
    /// Fill the pictures `event` lacks with the icons known, and remember
    /// whoever waits for one still to be read.
    pub(crate) fn fill(&mut self, event: &mut Event) {
        match event {
            Event::PresenceChanged { user, presence } => self.fill_presence(*user, presence),
            Event::Ready(snapshot) => {
                for (user, presence) in &mut snapshot.presences {
                    self.fill_presence(*user, presence);
                }
            }
            _ => {}
        }
    }

    fn fill_presence(&mut self, user: UserId, presence: &mut Presence) {
        if self.fill_known(presence)
            && (self.waiting.len() < WAITING || self.waiting.contains_key(&user))
        {
            self.waiting.insert(user, presence.clone());
        } else {
            self.waiting.remove(&user);
        }
    }

    /// Whether `presence` still waits for an icon not read yet.
    fn fill_known(&mut self, presence: &mut Presence) -> bool {
        let mut waits = false;
        for activity in &mut presence.activities {
            let Some(application) = activity
                .application
                .filter(|_| activity.image.is_none() && activity.small_image.is_none())
            else {
                continue;
            };
            match self.known.get(&application) {
                Some(icon) => activity.image = icon.clone(),
                None => {
                    waits = true;
                    if self.asking != Some(application)
                        && !self.queue.contains(&application)
                        && self.queue.len() < QUEUED
                    {
                        self.queue.push_back(application);
                    }
                }
            }
        }
        waits
    }

    /// The next application to read, when none is being read.
    pub(crate) fn next(&mut self) -> Option<u64> {
        if self.asking.is_none() {
            self.asking = self.queue.pop_front();
            return self.asking;
        }
        None
    }

    /// The read under way was dropped with its connection: it goes first
    /// when one is asked again.
    pub(crate) fn interrupted(&mut self) {
        if let Some(application) = self.asking.take() {
            self.queue.push_front(application);
        }
    }

    /// What `application`'s read found: the presences that waited for it,
    /// told again with its icon. An answer this session did not ask for
    /// (one of the session before) is dropped.
    pub(crate) fn arrived(&mut self, application: u64, icon: Option<RemoteImage>) -> Vec<Event> {
        if self.asking != Some(application) {
            return Vec::new();
        }
        self.asking = None;
        while self.order.len() >= KNOWN {
            if let Some(oldest) = self.order.pop_front() {
                self.known.remove(&oldest);
            }
        }
        self.known.insert(application, icon.clone());
        self.order.push_back(application);
        if icon.is_none() {
            return Vec::new();
        }
        let ready: Vec<UserId> = self
            .waiting
            .iter()
            .filter(|(_, presence)| {
                presence
                    .activities
                    .iter()
                    .any(|activity| activity.application == Some(application))
            })
            .map(|(user, _)| *user)
            .collect();
        ready
            .into_iter()
            .filter_map(|user| {
                let mut presence = self.waiting.remove(&user)?;
                self.fill_presence(user, &mut presence);
                Some(Event::PresenceChanged { user, presence })
            })
            .collect()
    }
}

/// The icon `value`, the public object of `application`, names: 128 px on
/// the CDN, as an application's other pictures are asked for.
pub(crate) fn icon(application: u64, value: &Value) -> Option<RemoteImage> {
    if crate::wire::id(&value["id"]).ok()? != application {
        return None;
    }
    let icon = value["icon"]
        .as_str()
        .filter(|icon| icon.len() == 32 && icon.bytes().all(|byte| byte.is_ascii_hexdigit()))?;
    Some(RemoteImage {
        url: format!("https://cdn.discordapp.com/app-icons/{application}/{icon}.png?size=128"),
        kind: rune_model::ImageKind::Artwork,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{Activity, ActivityKind, Status};
    use serde_json::json;

    fn playing(application: Option<u64>, image: Option<RemoteImage>) -> Presence {
        Presence {
            status: Status::Online,
            activities: vec![Activity {
                kind: ActivityKind::Playing,
                name: "Juego".into(),
                detail: None,
                state: None,
                image,
                small_image: None,
                started: None,
                ends: None,
                application,
            }],
            ..Default::default()
        }
    }

    fn picture(name: &str) -> RemoteImage {
        RemoteImage {
            url: format!("https://cdn.discordapp.com/{name}.png"),
            kind: rune_model::ImageKind::Artwork,
        }
    }

    fn told(user: u64, presence: Presence) -> Event {
        Event::PresenceChanged {
            user: UserId(user),
            presence,
        }
    }

    fn image_of(event: &Event) -> Option<RemoteImage> {
        match event {
            Event::PresenceChanged { presence, .. } => presence.activities[0].image.clone(),
            _ => None,
        }
    }

    #[test]
    fn an_activity_without_pictures_waits_for_its_applications_icon_once() {
        let mut icons = AppIcons::default();
        let mut first = told(1, playing(Some(7), None));
        let mut second = told(2, playing(Some(7), None));
        icons.fill(&mut first);
        icons.fill(&mut second);
        // One read for both; with its own picture, none.
        let mut own = told(3, playing(Some(8), Some(picture("own"))));
        icons.fill(&mut own);
        assert_eq!(icons.next(), Some(7));
        assert_eq!(icons.next(), None, "one read at a time");
        let again = icons.arrived(7, Some(picture("icon")));
        assert_eq!(again.len(), 2);
        assert!(again
            .iter()
            .all(|event| image_of(event) == Some(picture("icon"))));
        // Known now: filled as it comes, without another read.
        let mut later = told(4, playing(Some(7), None));
        icons.fill(&mut later);
        assert_eq!(image_of(&later), Some(picture("icon")));
        assert_eq!(icons.next(), None);
    }

    #[test]
    fn a_failed_read_is_not_repeated_and_a_newer_presence_wins() {
        let mut icons = AppIcons::default();
        icons.fill(&mut told(1, playing(Some(7), None)));
        assert_eq!(icons.next(), Some(7));
        // Someone stopped playing before it came: nothing to tell them.
        icons.fill(&mut told(1, Presence::default()));
        assert!(icons.arrived(7, Some(picture("icon"))).is_empty());
        icons.fill(&mut told(2, playing(Some(9), None)));
        assert_eq!(icons.next(), Some(9));
        assert!(icons.arrived(9, None).is_empty());
        icons.fill(&mut told(3, playing(Some(9), None)));
        assert_eq!(icons.next(), None, "a failure is kept for the session");
        // An answer nobody asked for is dropped.
        assert!(icons.arrived(11, Some(picture("x"))).is_empty());
        // A read dropped with its connection is asked again, first.
        icons.fill(&mut told(4, playing(Some(12), None)));
        icons.fill(&mut told(5, playing(Some(13), None)));
        assert_eq!(icons.next(), Some(12));
        icons.interrupted();
        assert_eq!(icons.next(), Some(12));
        assert_eq!(icons.arrived(12, Some(picture("doce"))).len(), 1);
    }

    #[test]
    fn the_icon_is_the_applications_own_hash_on_the_cdn() {
        let hash = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            icon(7, &json!({"id": "7", "icon": hash})).unwrap().url,
            format!("https://cdn.discordapp.com/app-icons/7/{hash}.png?size=128")
        );
        assert!(
            icon(7, &json!({"id": "8", "icon": hash})).is_none(),
            "another one's"
        );
        assert!(icon(7, &json!({"id": "7", "icon": "../token"})).is_none());
        assert!(icon(7, &json!({"id": "7", "icon": null})).is_none());
    }
}
