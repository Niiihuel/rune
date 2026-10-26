//! Profiles and member restrictions in the simulation: when people joined,
//! when their accounts were made, what they share with the account, and who
//! is timed out or waiting on verification. Everything draws from its own
//! seeds, so the rest of the world stays as it was.

use rune_i18n::t;
use rune_model::{
    ChannelId, ImageKind, Permissions, ProfileBanner, RelationshipKind, RemoteImage, Restriction,
    ServerId, Timestamp, UserId, UserProfile,
};

use super::{mix, MockConfig, World, ME};
use crate::{Event, ProfileImage, RequestId, ServerProfileEdit};

const DAY_MINUTES: i64 = 24 * 60;
/// The server where the account itself is timed out: Café Nocturno.
const TIMED_OUT_HERE: ServerId = ServerId(3);
/// How long the account's timeout lasts from the start of the simulation.
const TIMEOUT_MINUTES: i64 = 95;

impl World {
    /// Date memberships, and restrict a few: the account in one server, and
    /// someone timed out and someone still verifying in the first.
    pub(super) fn restrict(&mut self, config: &MockConfig) {
        let start = (config.clock)();
        let (seed, end) = (self.seed, self.history_end);
        for member in &mut self.members {
            let days = mix(&[seed, member.server.0, member.user.0, 0x501]).range(3, 900);
            member.joined = Some(end.plus_minutes(-days * DAY_MINUTES));
        }
        let mut pick = mix(&[seed, 0x7105]);
        let plain: Vec<UserId> = self
            .members
            .iter()
            .filter(|m| m.server == ServerId(1) && m.user != ME && m.roles.is_empty())
            .map(|m| m.user)
            .collect();
        let timed = plain.get(pick.below(plain.len() as u64) as usize).copied();
        let pending = plain.iter().copied().find(|user| Some(*user) != timed);
        let tagged = plain
            .iter()
            .copied()
            .find(|user| Some(*user) != timed && Some(*user) != pending);
        for member in &mut self.members {
            if member.server == TIMED_OUT_HERE && member.user == ME {
                member.restrictions.timeout_until = Some(start.plus_minutes(TIMEOUT_MINUTES));
            } else if member.server == ServerId(1) && Some(member.user) == timed {
                member.restrictions.timeout_until = Some(start.plus_minutes(6 * 60));
            } else if member.server == ServerId(1) && Some(member.user) == pending {
                member.restrictions.pending = true;
            } else if member.server == ServerId(1) && Some(member.user) == tagged {
                member.restrictions.tag_quarantined = true;
            }
        }
    }

    /// Whether the account may not take part in `channel`'s server at `now`.
    pub(super) fn restricted_in(&self, channel: ChannelId, now: Timestamp) -> Option<Restriction> {
        let server = self.channel(channel)?.server?;
        self.members
            .iter()
            .find(|m| m.server == server && m.user == ME)?
            .restrictions
            .at(now)
    }

    /// What `user`'s profile adds, seen in `server`: the account's creation,
    /// the servers both are in, and some of the account's friends.
    pub(super) fn load_profile(
        &self,
        request: RequestId,
        user: UserId,
        server: Option<ServerId>,
        effect: bool,
    ) -> Result<Vec<Event>, String> {
        if !self.users.iter().any(|known| known.id == user) {
            return Err(t("esa persona no existe").into());
        }
        let mut rng = mix(&[self.seed, user.0, 0xC4EA7ED]);
        let created = self
            .history_end
            .plus_minutes(-rng.range(400, 3_000) * DAY_MINUTES);
        let shared = |of: UserId| {
            self.members
                .iter()
                .filter(move |m| m.user == of)
                .map(|m| m.server)
        };
        let (mutual_servers, mutual_friends) = if user == ME {
            (None, None)
        } else {
            let mine: Vec<ServerId> = shared(ME).collect();
            let friends = self
                .relationships
                .iter()
                .filter(|r| r.kind == RelationshipKind::Friend && r.user != user)
                .map(|r| r.user)
                .filter(|_| rng.chance(0.5))
                .collect();
            (
                Some(shared(user).filter(|s| mine.contains(s)).collect()),
                Some(friends),
            )
        };
        // What the account set for itself in that server, as it saved it.
        let (server_about, server_banner) = server
            .filter(|_| user == ME)
            .and_then(|server| self.server_profiles.get(&server).cloned())
            .unwrap_or_default();
        // A bio naming the server's emoji (2026-10-17).
        let (about, pictures) = super::identity::bio(user).unzip();
        Ok(vec![Event::ProfileLoaded {
            request,
            profile: UserProfile {
                user,
                server,
                created: Some(created),
                mutual_servers,
                mutual_friends,
                server_about,
                server_banner,
                // Badges and accounts elsewhere, simulated (P5).
                badges: Some(super::identity::badges(user)),
                connections: Some(super::identity::connections(user)),
                effect: super::identity::effect(user).filter(|_| effect),
                about,
                pictures: pictures.unwrap_or_default(),
                ..Default::default()
            },
        }])
    }

    /// Change the account's profile in `server`, checked as a service would:
    /// a member, not restricted there, allowed to change its own name for a
    /// new one, and within the lengths.
    pub(super) fn edit_server_profile(
        &mut self,
        server: ServerId,
        edit: ServerProfileEdit,
    ) -> Result<Vec<Event>, String> {
        let restricted = self
            .members
            .iter()
            .find(|m| m.server == server && m.user == ME)
            .ok_or(t("no eres miembro de ese servidor"))?
            .restrictions
            .at(self.now);
        if restricted.is_some() {
            return Err(t("tu cuenta está restringida en ese servidor").into());
        }
        let clean = |text: Option<String>| {
            text.map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty())
        };
        let nick = edit.nick.map(clean);
        if nick.is_some() {
            self.require(server, Permissions::CHANGE_NICK)?;
        }
        if nick
            .as_ref()
            .is_some_and(|nick| nick.as_ref().is_some_and(|n| n.chars().count() > 32))
        {
            return Err(t("el apodo puede tener hasta 32 caracteres").into());
        }
        let about = edit.about.map(clean);
        if about
            .as_ref()
            .is_some_and(|about| about.as_ref().is_some_and(|a| a.chars().count() > 190))
        {
            return Err(t("«Sobre mí» puede tener hasta 190 caracteres").into());
        }
        let version = self.fresh_id();
        let avatar = edit.avatar.map(|avatar| {
            avatar.map(|image| self.host(&format!("avatar-servidor-{server}"), version, &image))
        });
        let banner = edit.banner.map(|banner| match banner {
            Some(image) => ProfileBanner::Image(self.host(
                &format!("banner-servidor-{server}"),
                version,
                &image,
            )),
            None => ProfileBanner::Absent,
        });
        let member = self
            .members
            .iter_mut()
            .find(|m| m.server == server && m.user == ME)
            .ok_or(t("no eres miembro de ese servidor"))?;
        if let Some(nick) = nick {
            member.nick = nick;
        }
        if let Some(avatar) = avatar {
            member.avatar = avatar;
        }
        let member = member.clone();
        let kept = self.server_profiles.entry(server).or_default();
        if let Some(about) = about {
            kept.0 = about;
        }
        if let Some(banner) = banner {
            kept.1 = banner;
        }
        let (server_about, server_banner) = kept.clone();
        Ok(vec![
            Event::MemberChanged(member.clone()),
            Event::ProfileChanged(UserProfile {
                user: ME,
                server: Some(server),
                nick: member.nick,
                server_about,
                server_banner,
                ..Default::default()
            }),
        ])
    }
}

impl World {
    /// Name a picture the account saved as its `what` and keep its file in
    /// place of the one it had there, for as long as the simulation runs:
    /// the core reads it back through `Provider::hosted_image`.
    pub(super) fn host(&mut self, what: &str, version: u64, image: &ProfileImage) -> RemoteImage {
        let slot = format!("https://simulacion.invalid/perfiles/{ME}/{what}/");
        let picture = RemoteImage {
            url: format!(
                "{slot}{version}-{}.{}",
                image.bytes().len(),
                image.mime().trim_start_matches("image/")
            ),
            kind: if what.starts_with("banner") {
                ImageKind::Banner
            } else {
                ImageKind::Artwork
            },
        };
        if let Ok(mut hosted) = self.hosted.write() {
            hosted.retain(|url, _| !url.starts_with(&slot));
            hosted.insert(
                picture.url.clone(),
                bytes::Bytes::copy_from_slice(image.bytes()),
            );
        }
        picture
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockProvider;
    use crate::Command;

    #[test]
    fn the_simulation_restricts_the_account_in_one_server_and_two_people_in_the_first() {
        let provider = MockProvider::new(MockConfig {
            clock: || Timestamp(1_000_000_000_000),
            ..MockConfig::default()
        });
        let snapshot = provider.snapshot();
        let restricted: Vec<_> = snapshot
            .members
            .iter()
            // Rules and onboarding (2026-10-18): finishing it restricts nothing.
            .filter(|m| {
                rune_model::MemberRestrictions {
                    onboarded: false,
                    ..m.restrictions.clone()
                } != Default::default()
            })
            .map(|m| (m.server, m.user, m.restrictions.clone()))
            .collect();
        assert_eq!(restricted.len(), 4, "{restricted:?}");
        assert!(restricted
            .iter()
            .any(|(server, _, r)| *server == ServerId(1) && r.tag_quarantined));
        assert!(restricted
            .iter()
            .any(|(server, user, r)| *server == TIMED_OUT_HERE
                && *user == ME
                && r.timeout_until
                    == Some(Timestamp(1_000_000_000_000).plus_minutes(TIMEOUT_MINUTES))));
        assert!(snapshot.members.iter().all(|m| m.joined.is_some()));
        let events = provider.manage(Command::LoadProfile {
            request: 3,
            user: UserId(10),
            server: Some(ServerId(1)),
            effect: true,
        });
        let [Event::ProfileLoaded {
            request: 3,
            profile,
        }] = &events[..]
        else {
            panic!("{events:?}");
        };
        assert!(profile.answers(UserId(10), Some(ServerId(1))));
        assert!(profile.created.is_some() && profile.mutual_servers.is_some());
        assert!(profile.effect.is_some(), "the effect she wears");
        // With effects hidden on this device, none is read.
        let hidden = provider.manage(Command::LoadProfile {
            request: 5,
            user: UserId(10),
            server: Some(ServerId(1)),
            effect: false,
        });
        assert!(matches!(&hidden[..], [Event::ProfileLoaded { profile, .. }]
            if profile.effect.is_none()));
        // The account's own profile reports no mutual data: not "none".
        let own = provider.manage(Command::LoadProfile {
            request: 4,
            user: ME,
            server: None,
            effect: true,
        });
        assert!(matches!(&own[..], [Event::ProfileLoaded { profile, .. }]
            if profile.mutual_servers.is_none() && profile.mutual_friends.is_none()));
    }
}

#[cfg(test)]
mod edit_tests {
    use super::*;
    use crate::mock::MockProvider;
    use crate::{Command, ProfileEdit};

    fn png() -> ProfileImage {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(64u32.to_be_bytes());
        bytes.extend(64u32.to_be_bytes());
        ProfileImage::new(bytes.into()).unwrap()
    }

    #[test]
    fn pictures_and_the_profile_of_a_server_are_saved_and_reported_back() {
        let provider = MockProvider::new(MockConfig::default());
        let saved = provider.manage(Command::EditProfile(crate::ProfileEdit {
            avatar: Some(Some(png())),
            banner: Some(Some(png())),
            ..ProfileEdit::default()
        }));
        let [Event::UserChanged(me), Event::ProfileChanged(profile)] = &saved[..] else {
            panic!("{saved:?}");
        };
        assert!(me
            .avatar
            .as_ref()
            .is_some_and(|a| a.url.contains("simulacion.invalid")));
        assert!(
            matches!(&me.banner, ProfileBanner::Image(image) if image.kind == ImageKind::Banner)
        );
        assert_eq!((profile.user, profile.server), (ME, None));
        // Taking the banner away says it is gone, not unknown.
        let cleared = provider.manage(Command::EditProfile(ProfileEdit {
            banner: Some(None),
            ..ProfileEdit::default()
        }));
        assert!(
            matches!(&cleared[0], Event::UserChanged(me) if me.banner == ProfileBanner::Absent)
        );

        // The account owns the first server, so it may rename itself there.
        let server = ServerId(1);
        let edited = provider.manage(Command::EditServerProfile {
            server,
            edit: ServerProfileEdit {
                nick: Some(Some("  Laboratorio  ".into())),
                about: Some(Some("Aquí pruebo cosas".into())),
                avatar: Some(Some(png())),
                banner: Some(None),
            },
        });
        let [Event::MemberChanged(member), Event::ProfileChanged(profile)] = &edited[..] else {
            panic!("{edited:?}");
        };
        assert_eq!(member.nick.as_deref(), Some("Laboratorio"));
        assert!(member.avatar.is_some());
        assert_eq!(profile.server, Some(server));
        assert_eq!(profile.server_about.as_deref(), Some("Aquí pruebo cosas"));
        assert_eq!(profile.server_banner, ProfileBanner::Absent);
        // A later read of that profile reports what was saved there.
        let read = provider.manage(Command::LoadProfile {
            request: 9,
            user: ME,
            server: Some(server),
            effect: true,
        });
        assert!(matches!(&read[..], [Event::ProfileLoaded { profile, .. }]
            if profile.server_about.as_deref() == Some("Aquí pruebo cosas")));
        // Too long, or where the account is timed out, is refused with why.
        let long = provider.manage(Command::EditServerProfile {
            server,
            edit: ServerProfileEdit {
                about: Some(Some("x".repeat(191))),
                ..Default::default()
            },
        });
        assert!(matches!(&long[..], [Event::Failed { .. }]));
        let timed_out = provider.manage(Command::EditServerProfile {
            server: TIMED_OUT_HERE,
            edit: ServerProfileEdit {
                about: Some(Some("hola".into())),
                ..Default::default()
            },
        });
        assert!(matches!(
            &timed_out[..],
            [Event::Failed {
                failure: crate::Failure::ServerProfile { .. },
                ..
            }]
        ));
    }
}
