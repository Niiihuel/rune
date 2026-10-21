//! Rich presence, visual identity and the account's perks (P5): where
//! someone is connected from and how far along their activities are, the
//! marks a service draws around a name or a picture, the badges and
//! accounts a profile shows, the standard sticker packs, and what the
//! account may use beyond the conversation's own server. Pictures are
//! [`RemoteImage`]s their provider built; every list is bounded.

use serde::{Deserialize, Serialize};

use crate::{Activity, ActivityKind, RemoteImage, Rgb, ServerId, Sticker, Timestamp};

/// Activities kept per person, at most, as Serein keeps them.
pub const MAX_ACTIVITIES: usize = 4;
/// Badges and connected accounts kept per profile, at most.
pub const MAX_BADGES: usize = 16;
pub const MAX_CONNECTIONS: usize = 16;
/// Standard sticker packs kept, at most, and stickers in each.
pub const MAX_PACKS: usize = 128;
pub const MAX_PACK_STICKERS: usize = 500;

/// Where someone is connected from, among the places that report it.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Platforms {
    pub desktop: bool,
    pub mobile: bool,
    pub web: bool,
}

impl Platforms {
    /// Only on a phone: lists draw a phone instead of their dot.
    pub fn mobile_only(self) -> bool {
        self.mobile && !self.desktop && !self.web
    }
}

impl Activity {
    /// How far along it is at `now`, from 0 to 1, when it has a start and
    /// an end: a track playing.
    pub fn progress(&self, now: Timestamp) -> Option<f32> {
        let (start, end) = (self.started?, self.ends?);
        let length = end.0.checked_sub(start.0).filter(|length| *length > 0)?;
        Some(((now.0 - start.0) as f32 / length as f32).clamp(0.0, 1.0))
    }

    /// Music with its length known: drawn with a bar from start to end.
    pub fn is_track(&self) -> bool {
        self.kind == ActivityKind::Listening && self.started.is_some() && self.ends.is_some()
    }
}

/// The marks around someone's name and picture, as their service shows
/// them. A member's own, in one server, replace their account's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    /// A frame drawn over their picture, a fifth larger than it.
    #[serde(default)]
    pub decoration: Option<RemoteImage>,
    /// Artwork behind their name in lists.
    #[serde(default)]
    pub nameplate: Option<Nameplate>,
    /// The tag of a server they show beside their name.
    #[serde(default)]
    pub tag: Option<ServerTag>,
}

impl Identity {
    /// This one, with what `over` sets in its place: a member's over their
    /// account's.
    pub fn with(&self, over: &Identity) -> Identity {
        Identity {
            decoration: over.decoration.clone().or_else(|| self.decoration.clone()),
            nameplate: over.nameplate.clone().or_else(|| self.nameplate.clone()),
            tag: over.tag.clone().or_else(|| self.tag.clone()),
        }
    }
}

/// Artwork behind a name in a list, on a ground of its colors.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Nameplate {
    pub image: RemoteImage,
    /// The ground's colors, from the left to the right, when it has them.
    #[serde(default)]
    pub colors: Option<(Rgb, Rgb)>,
    /// What it is called, for assistive technology.
    #[serde(default)]
    pub label: Option<String>,
    /// The moving artwork, shown while its row is under the pointer or
    /// chosen and motion is not reduced (2026-10-18).
    #[serde(default)]
    pub animated: Option<RemoteImage>,
}

/// A server's tag someone shows beside their name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerTag {
    pub server: ServerId,
    /// A few letters.
    pub tag: String,
    #[serde(default)]
    pub badge: Option<RemoteImage>,
}

/// The longest tag shown.
pub const TAG_MAX: usize = 4;

/// What a role adds beside a name besides its color: a picture or an
/// emoji, and more colors for a gradient.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleStyle {
    #[serde(default)]
    pub icon: Option<RemoteImage>,
    /// A Unicode emoji, when it has no picture.
    #[serde(default)]
    pub emoji: Option<String>,
    /// Colors after the role's own, from the left to the right: one or two.
    #[serde(default)]
    pub gradient: Vec<Rgb>,
}

impl RoleStyle {
    /// Whether it shows something beside a name.
    pub fn has_mark(&self) -> bool {
        self.icon.is_some() || self.emoji.is_some()
    }
}

/// A badge a profile shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Badge {
    pub id: String,
    /// What it means, shown as its tooltip.
    pub description: String,
    pub icon: Option<RemoteImage>,
    /// Where it leads, when its provider gives an HTTPS address.
    pub link: Option<String>,
}

/// An account elsewhere that someone connected to their profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// The service's key, lowercase: "github", "steam", "domain".
    pub service: String,
    /// The service as people know it: "GitHub".
    pub label: String,
    /// Their name there.
    pub name: String,
    /// The service confirmed it is theirs.
    pub verified: bool,
    /// Their page there, when its provider builds one.
    pub link: Option<String>,
}

/// Layers of an effect kept, at most.
pub const MAX_EFFECT_LAYERS: usize = 8;

/// An effect drawn over someone's profile card: pictures as wide as the
/// card, each at its place and in its time, and a still one for reduced
/// motion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileEffect {
    /// The canvas, in its own pixels: 450×880 as its service draws it.
    pub width: u32,
    pub height: u32,
    /// The lowest first.
    pub layers: Vec<EffectLayer>,
    pub still: Option<RemoteImage>,
}

/// One picture of an effect, at its place on the canvas, shown from
/// `start` for `duration` milliseconds after the card opens; one that
/// loops shows again each `duration` plus `loop_delay` (2026-10-17: every
/// layer showed at once, for good).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectLayer {
    pub image: RemoteImage,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub start: u32,
    pub duration: u32,
    pub looping: bool,
    pub loop_delay: u32,
}

impl EffectLayer {
    /// Whether it shows `elapsed` milliseconds after the card opened.
    pub fn shows(&self, elapsed: i64) -> bool {
        let (start, duration) = (i64::from(self.start), i64::from(self.duration));
        if elapsed < start {
            return false;
        }
        if !self.looping {
            return elapsed <= start + duration;
        }
        self.loop_delay == 0
            || duration == 0
            || (elapsed - start) % (duration + i64::from(self.loop_delay)) < duration
    }

    /// How long after `elapsed` it next appears or goes, if it ever does.
    pub fn changes_in(&self, elapsed: i64) -> Option<i64> {
        let (start, duration) = (i64::from(self.start), i64::from(self.duration));
        if elapsed < start {
            return Some(start - elapsed);
        }
        if !self.looping {
            return (elapsed <= start + duration).then(|| start + duration - elapsed + 1);
        }
        if self.loop_delay == 0 || duration == 0 {
            return None;
        }
        let period = duration + i64::from(self.loop_delay);
        let phase = (elapsed - start) % period;
        Some(if phase < duration {
            duration - phase
        } else {
            period - phase
        })
    }
}

/// Pictures a profile's texts name, at most.
pub const MAX_TEXT_PICTURES: usize = 64;

/// A picture a text names by a token, such as a server's emoji written
/// `<a:name:id>` in someone's «Sobre mí»: the token as written, the name
/// shown while the picture loads, and the picture, moving when it does
/// (2026-10-17: the token was shown as written).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextPicture {
    pub token: String,
    pub name: String,
    pub image: RemoteImage,
}

/// A pack of stickers its service offers everyone.
#[derive(Clone, Debug, PartialEq)]
pub struct StickerPack {
    pub id: u64,
    pub name: String,
    pub stickers: Vec<Sticker>,
}

/// What the account may use beyond the conversation's own server, as its
/// plan with its service allows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Perks {
    /// Custom emoji of any of its servers, and in direct conversations.
    pub emoji_anywhere: bool,
    pub animated_emoji: bool,
    /// Stickers of any of its servers, and in direct conversations.
    pub stickers_anywhere: bool,
    /// What unlocks what is missing, as its service calls it: "Nitro".
    pub upgrade: Option<String>,
    // Profile editing (edicion-de-perfil).
    /// A moving picture for the account.
    pub animated_avatar: bool,
    /// A banner on its profile, still or moving.
    pub profile_banner: bool,
    /// A picture, a banner and «Sobre mí» of its own in each server.
    pub server_profiles: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ImageKind;

    fn image(name: &str) -> RemoteImage {
        RemoteImage {
            url: format!("https://cdn.example.test/{name}.png"),
            kind: ImageKind::Artwork,
        }
    }

    #[test]
    fn an_effects_intro_plays_once_and_its_loop_waits_between_rounds() {
        let layer = |start, duration, looping, loop_delay| EffectLayer {
            image: image("hoja"),
            x: 0,
            y: 0,
            width: 450,
            height: 880,
            start,
            duration,
            looping,
            loop_delay,
        };
        let intro = layer(0, 2_000, false, 0);
        assert!(intro.shows(0) && intro.shows(2_000) && !intro.shows(2_001));
        assert_eq!(intro.changes_in(500), Some(1_501));
        assert_eq!(intro.changes_in(2_500), None, "gone for good");
        let rounds = layer(1_000, 3_000, true, 2_000);
        assert!(!rounds.shows(999) && rounds.shows(1_000) && rounds.shows(3_999));
        assert!(!rounds.shows(4_000) && !rounds.shows(5_999) && rounds.shows(6_000));
        assert_eq!(rounds.changes_in(0), Some(1_000));
        assert_eq!(rounds.changes_in(2_000), Some(2_000));
        assert_eq!(rounds.changes_in(5_000), Some(1_000));
        let always = layer(0, 0, true, 0);
        assert!(always.shows(123_456) && always.changes_in(5).is_none());
    }

    #[test]
    fn a_track_knows_how_far_along_it_is_and_others_do_not() {
        let mut track = Activity {
            kind: ActivityKind::Listening,
            name: "Música".into(),
            detail: None,
            state: None,
            image: None,
            small_image: None,
            started: Some(Timestamp(1_000)),
            ends: Some(Timestamp(5_000)),
            application: None,
        };
        assert!(track.is_track());
        assert_eq!(track.progress(Timestamp(2_000)), Some(0.25));
        assert_eq!(track.progress(Timestamp(9_000)), Some(1.0), "past its end");
        track.ends = Some(Timestamp(1_000));
        assert_eq!(track.progress(Timestamp(2_000)), None, "no length");
        track.kind = ActivityKind::Playing;
        assert!(!track.is_track());
    }

    #[test]
    fn a_members_marks_replace_only_what_they_set() {
        let account = Identity {
            decoration: Some(image("account")),
            nameplate: None,
            tag: Some(ServerTag {
                server: ServerId(1),
                tag: "RUNE".into(),
                badge: None,
            }),
        };
        let member = Identity {
            decoration: Some(image("server")),
            ..Default::default()
        };
        let shown = account.with(&member);
        assert_eq!(shown.decoration, Some(image("server")));
        assert_eq!(shown.tag, account.tag);
        assert_ne!(shown, Identity::default());
        assert!(Platforms {
            mobile: true,
            ..Default::default()
        }
        .mobile_only());
        assert!(!Platforms {
            mobile: true,
            web: true,
            ..Default::default()
        }
        .mobile_only());
    }

    #[test]
    fn rows_kept_before_identities_read_without_them() {
        let user: crate::User = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "Legacy", "display_name": null, "accent": 0, "bot": false
        }))
        .unwrap();
        assert_eq!(user.identity, Identity::default());
        let role: crate::Role = serde_json::from_value(serde_json::json!({
            "id": 2, "name": "Viejo", "color": null, "position": 1, "hoisted": false
        }))
        .unwrap();
        assert_eq!(role.style, RoleStyle::default());
        let presence: crate::Presence =
            serde_json::from_value(serde_json::json!({"status": "Online", "custom": null}))
                .unwrap();
        assert!(presence.activities.is_empty() && presence.platforms == Platforms::default());
    }
}
