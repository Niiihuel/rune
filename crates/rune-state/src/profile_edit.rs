//! What the account may change of its own profile, across servers or in
//! one, and the profile it saved. As with the other abilities, the interface
//! disables what is denied and says why with the same reason.

use rune_i18n::{es, t, tr};
use rune_model::{Perks, RemoteImage, ServerId, UserId, UserProfile};

use crate::profiles::restricted;
use crate::{Applied, Permit, Store};

/// Why the username, email and password are never changed here: the service
/// asks for the password to change them, and RUNE never asks for it.
pub const CREDENTIALS_DENIED: &str = es("Cambiar el nombre de usuario, el correo o la contraseña pide tu contraseña, y RUNE nunca la pide: hazlo desde el servicio.");

/// What the account may change of its profile, in one server or across all.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileAbilities {
    /// Across servers: name, pronouns, «Sobre mí» and color. In a server:
    /// its «Sobre mí» there.
    pub text: Permit,
    /// The picture and the banner.
    pub images: Permit,
    /// The name the account goes by in the server; denied across servers.
    pub nick: Permit,
    /// Username, email and password: always denied, with the reason.
    pub credentials: Permit,
    // Profile editing (edicion-de-perfil).
    /// A new banner: as `images`, and denied when the account's plan is
    /// known not to allow one, before the service would refuse it.
    pub banner: Permit,
    /// A moving avatar, likewise; a moving banner needs only `banner`.
    /// Denied, a moving picture chosen keeps only its first frame.
    pub animated: Permit,
}

/// `first`, or `then` once `first` allows it.
fn both(first: Permit, then: Permit) -> Permit {
    if first.allowed() {
        then
    } else {
        first
    }
}

impl Store {
    /// The picture `user` shows in `server`: the one set there, else their
    /// account's, else their service's default.
    pub fn avatar_in(&self, server: Option<ServerId>, user: UserId) -> Option<&RemoteImage> {
        server
            .and_then(|server| self.member_of(server, user)?.avatar.as_ref())
            .or_else(|| self.user(user)?.picture())
    }

    /// What the account may change of its profile in `server`, or of the
    /// one it shows everywhere (`None`).
    pub fn profile_abilities(&self, server: Option<ServerId>) -> ProfileAbilities {
        let denied = |reason: &str| Permit::Denied(reason.into());
        let credentials = denied(t(CREDENTIALS_DENIED));
        let (Some(info), Some(_)) = (self.provider.as_ref(), self.me_user()) else {
            let offline = denied(t("La cuenta todavía no se conectó."));
            return ProfileAbilities {
                text: offline.clone(),
                images: offline.clone(),
                nick: offline.clone(),
                credentials,
                banner: offline.clone(),
                animated: offline,
            };
        };
        let can = |allowed: bool, reason: &str| {
            if allowed {
                Permit::Allowed
            } else {
                denied(reason)
            }
        };
        let capabilities = info.capabilities;
        let Some(server) = server else {
            let images = can(
                capabilities.profile_images,
                t("Este proveedor no permite cambiar el avatar ni el banner."),
            );
            return ProfileAbilities {
                text: can(
                    capabilities.edit_profile,
                    t("Este proveedor no permite cambiar el perfil."),
                ),
                nick: denied(t("El apodo es de cada servidor.")),
                credentials,
                banner: both(
                    images.clone(),
                    self.plan(|perks| perks.profile_banner, t("un banner")),
                ),
                animated: both(
                    images.clone(),
                    self.plan(|perks| perks.animated_avatar, t("un avatar animado")),
                ),
                images,
            };
        };
        let (Some(known), Some(abilities)) = (self.server(server), self.server_abilities(server))
        else {
            let gone = denied(t("Ya no estás en ese servidor."));
            return ProfileAbilities {
                text: gone.clone(),
                images: gone.clone(),
                nick: gone.clone(),
                credentials,
                banner: gone.clone(),
                animated: gone,
            };
        };
        let scoped = match self.restriction(server) {
            Some(restriction) => restricted(restriction, &known.name),
            None => both(
                can(
                    capabilities.server_profiles,
                    t("Este proveedor no permite un perfil por servidor."),
                ),
                // Only the name there is free.
                self.plan(
                    |perks| perks.server_profiles,
                    t("un perfil propio en cada servidor"),
                ),
            ),
        };
        ProfileAbilities {
            text: scoped.clone(),
            images: scoped.clone(),
            nick: if capabilities.server_profiles {
                abilities.change_nick
            } else {
                denied(t("Este proveedor no permite un perfil por servidor."))
            },
            credentials,
            banner: scoped.clone(),
            animated: scoped,
        }
    }

    /// Allowed unless the account's plan is known and lacks `has`: then the
    /// service would refuse it, and the reason names what unlocks it, as
    /// with emoji (P5). Unknown, the service answers as before.
    fn plan(&self, has: impl Fn(&Perks) -> bool, what: &str) -> Permit {
        match self.perks() {
            Some(perks) if !has(perks) => Permit::Denied(match &perks.upgrade {
                // A plan's name may be a provider's text kept in Spanish.
                Some(upgrade) => tr!(
                    "Necesitas {upgrade} para usar {what}",
                    upgrade = t(upgrade),
                    what
                ),
                None => tr!("Tu cuenta no puede usar {what}", what),
            }),
            _ => Permit::Allowed,
        }
    }

    /// The account's profile as it saved it. Saved across servers, its text,
    /// color and banner replace those of every profile of the account kept;
    /// saved in a server, what it set there replaces that server's. Dates and
    /// mutuals are kept.
    pub(crate) fn profile_changed(&mut self, saved: UserProfile) -> Applied {
        if self.me != Some(saved.user) {
            return Applied::Nothing;
        }
        let server = saved.server;
        if server.is_some()
            && !self
                .profiles
                .iter()
                .any(|known| known.user == saved.user && known.server == server)
        {
            self.profiles.push(UserProfile {
                user: saved.user,
                server,
                ..Default::default()
            });
        }
        for known in self
            .profiles
            .iter_mut()
            .filter(|known| known.user == saved.user)
        {
            match server {
                None => {
                    known.about = saved.about.clone();
                    known.pronouns = saved.pronouns.clone();
                    known.accent = saved.accent;
                    known.banner = saved.banner.clone();
                }
                Some(_) if known.server == server => {
                    known.nick = saved.nick.clone();
                    known.server_about = saved.server_about.clone();
                    known.server_banner = saved.server_banner.clone();
                }
                Some(_) => {}
            }
        }
        Applied::ProfileSaved { server }
    }
}
