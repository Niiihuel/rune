//! Member restrictions on what the account may do, and profiles loaded on
//! demand. A restriction is read at the store's clock, so a timeout lifts by
//! itself once it ends; [`Store::restriction_expires`] says when to look
//! again. A loaded profile is kept only when it answers the one request in
//! flight, for the person and the scope that request named.

use std::time::Duration;

use rune_i18n::{t, tr};
use rune_model::{MemberList, Restriction, ServerId, Timestamp, UserId, UserProfile};
use rune_provider::{ConnectionState, RequestId};

use crate::{Abilities, Applied, Permit, ServerAbilities, Store};

/// Loaded profiles kept, at most: the cards opened lately.
const PROFILES_KEPT: usize = 32;
/// How long a loaded profile is current: the official client asks again
/// for one older than a minute as its popout opens (REA, 2026-10-17).
const PROFILE_FRESH: Duration = Duration::from_secs(60);

/// The system's wall clock.
fn system_wall() -> Timestamp {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Timestamp(elapsed.as_millis() as i64)
}

/// Why a restriction keeps the account from taking part in `server`.
pub(crate) fn restricted(restriction: Restriction, server: &str) -> Permit {
    Permit::Denied(match restriction {
        Restriction::TimedOut { .. } => {
            tr!("Estás aislado en {server}: podés leer, pero no participar", server)
        }
        Restriction::Pending => tr!("Aceptá las reglas de {server} para participar", server),
        Restriction::Quarantined => {
            tr!(
                "La moderación automática de {server} retiene tu cuenta: no podés participar",
                server
            )
        }
        Restriction::TagQuarantined => tr!(
            "La moderación automática de {server} bloqueó la etiqueta de servidor que mostrás: cambiala o dejá de mostrarla para participar",
            server
        ),
    })
}

impl Store {
    /// The time restrictions are read at: the injected clock, else the system's.
    pub fn wall(&self) -> Timestamp {
        self.clock.map_or_else(system_wall, |clock| clock())
    }

    /// What keeps `user` from taking part in `server` now, if anything.
    pub fn restriction_of(&self, server: ServerId, user: UserId) -> Option<Restriction> {
        self.member_of(server, user)?.restrictions.at(self.wall())
    }

    /// What keeps the account from taking part in `server` now, if anything.
    pub fn restriction(&self, server: ServerId) -> Option<Restriction> {
        self.restriction_of(server, self.me?)
    }

    /// When the account's restrictions should be read again: as one ends,
    /// and at the next whole minute while one lasts, so a remaining time
    /// shown stays right. `None` while no timeout of the account lasts.
    pub fn restriction_expires(&self) -> Option<Duration> {
        let me = self.me?;
        let wall = self.wall();
        // Per frame: only the account's own memberships, by index.
        let until = self
            .servers
            .iter()
            .filter_map(|server| self.member_of(server.id, me)?.restrictions.timeout_until)
            .filter(|until| *until > wall)
            .min()?;
        let minute = 60_000 - wall.0.rem_euclid(60_000);
        Some(Duration::from_millis((until.0 - wall.0).min(minute) as u64))
    }

    /// Take part in nothing while restricted in the channel's server: only
    /// reading, and taking back one's own messages, stay.
    pub(crate) fn restrict(&self, server: Option<ServerId>, abilities: &mut Abilities) {
        let Some((server, restriction)) =
            server.and_then(|server| Some((self.server(server)?, self.restriction(server)?)))
        else {
            return;
        };
        // Rules and onboarding (2026-10-18): answered rules say what came of them.
        let denied = (restriction == Restriction::Pending)
            .then(|| self.answered_reason(server.id, &server.name))
            .flatten()
            .unwrap_or_else(|| restricted(restriction, &server.name));
        for permit in [
            &mut abilities.write,
            &mut abilities.typing,
            &mut abilities.react,
            &mut abilities.vote,
            &mut abilities.edit_own,
            &mut abilities.delete_others,
            &mut abilities.attach,
            &mut abilities.stickers,
            &mut abilities.join_voice,
            &mut abilities.watch_stream,
            &mut abilities.watch_camera,
            &mut abilities.speak,
            &mut abilities.manage,
            &mut abilities.manage_permissions,
            &mut abilities.pin,
            // Message actions (P4).
            &mut abilities.create_poll,
            // Server administration, rest (P7).
            &mut abilities.forum_settings,
        ] {
            if permit.allowed() {
                *permit = denied.clone();
            }
        }
    }

    /// The same across the server: leaving stays.
    pub(crate) fn restrict_server(&self, server: ServerId, abilities: &mut ServerAbilities) {
        let Some((name, restriction)) = self
            .server(server)
            .map(|known| known.name.as_str())
            .zip(self.restriction(server))
        else {
            return;
        };
        let denied = (restriction == Restriction::Pending)
            .then(|| self.answered_reason(server, name))
            .flatten()
            .unwrap_or_else(|| restricted(restriction, name));
        for permit in [
            &mut abilities.manage_server,
            &mut abilities.manage_channels,
            &mut abilities.manage_roles,
            &mut abilities.kick,
            &mut abilities.ban,
            &mut abilities.manage_nicks,
            &mut abilities.change_nick,
            &mut abilities.view_audit_log,
            &mut abilities.timeout,
            &mut abilities.create_invites,
            &mut abilities.upload_expressions,
            &mut abilities.prune,
            &mut abilities.manage_webhooks,
            &mut abilities.manage_integrations,
            // Voice moderation (P4).
            &mut abilities.mute_members,
            &mut abilities.deafen_members,
            &mut abilities.move_members,
            // Server administration, rest (P7).
            &mut abilities.edit_identity,
            &mut abilities.edit_safety,
            &mut abilities.pause_invites,
            &mut abilities.reorder_channels,
            &mut abilities.role_icons,
            &mut abilities.role_gradients,
            // Rules and onboarding (2026-10-18).
            &mut abilities.manage_onboarding,
            // Icon, banner and community (2026-10-18).
            &mut abilities.animated_icon,
            &mut abilities.edit_banner,
            &mut abilities.animated_banner,
            &mut abilities.edit_community,
            &mut abilities.toggle_community,
            // AutoMod (automod).
            &mut abilities.manage_automod,
        ] {
            if permit.allowed() {
                *permit = denied.clone();
            }
        }
    }

    /// Register a profile read before forwarding its command. One card is
    /// open at a time, so a newer read replaces the one in flight.
    pub fn begin_profile(
        &mut self,
        user: UserId,
        server: Option<ServerId>,
        request: RequestId,
    ) -> Permit {
        if !self
            .provider
            .as_ref()
            .is_some_and(|p| p.capabilities.profiles)
        {
            return Permit::Denied(t("Este proveedor no permite cargar perfiles").into());
        }
        if self.me.is_none() || self.from_cache || self.connection != ConnectionState::Connected {
            return Permit::Denied(t("Esperá a que la conexión esté disponible").into());
        }
        if request == 0
            || self
                .profile_read
                .is_some_and(|(known, ..)| known >= request)
        {
            return Permit::Denied(t("Esta solicitud de perfil ya venció").into());
        }
        self.profile_read = Some((request, user, server));
        Permit::Allowed
    }

    /// Whether a profile read for `user` is in flight.
    pub fn profile_loading(&self, user: UserId) -> bool {
        self.profile_read.is_some_and(|(_, known, _)| known == user)
    }

    /// Whether the read in flight is of `user`'s profile in exactly this
    /// scope: another scope asked for replaces it.
    pub fn profile_loading_in(&self, user: UserId, server: Option<ServerId>) -> bool {
        self.profile_read
            .is_some_and(|(_, known, at)| known == user && at == server)
    }

    /// What was loaded of `user`'s profile in exactly this scope.
    pub fn profile(&self, user: UserId, server: Option<ServerId>) -> Option<&UserProfile> {
        self.profiles
            .iter()
            .rev()
            .find(|profile| profile.user == user && profile.server == server)
    }

    /// A loaded profile counts only as the answer to the read in flight.
    pub(crate) fn loaded_profile(&mut self, request: RequestId, profile: UserProfile) -> Applied {
        let Some((expected, user, server)) = self.profile_read else {
            return Applied::Nothing;
        };
        if expected != request || !profile.answers(user, server) {
            return Applied::Nothing;
        }
        self.profile_read = None;
        self.profiles
            .retain(|known| !(known.user == user && known.server == server));
        if self.profiles.len() >= PROFILES_KEPT {
            let old = self.profiles.remove(0);
            self.profile_at.remove(&(old.user, old.server));
        }
        self.profiles.push(profile);
        self.profile_at.insert((user, server), self.wall());
        Applied::Profile(user)
    }

    /// Whether `user`'s profile in this scope should be asked for again
    /// as its card opens: none was loaded, or it is older than
    /// a minute. What was kept still shows meanwhile (2026-10-18:
    /// a profile kept was never asked for again in the session). A clock
    /// set back past when it came counts it old, not fresh for hours.
    pub fn profile_stale(&self, user: UserId, server: Option<ServerId>) -> bool {
        self.profile_at.get(&(user, server)).is_none_or(|at| {
            let age = self.wall().0 - at.0;
            !(0..PROFILE_FRESH.as_millis() as i64).contains(&age)
        })
    }

    /// Whether a failure belongs to the read in flight; it ends it if so.
    pub(crate) fn profile_failed(&mut self, request: RequestId) -> bool {
        let current = self
            .profile_read
            .is_some_and(|(known, ..)| known == request);
        if current {
            self.profile_read = None;
        }
        current
    }

    /// `server`'s member list as its provider orders it, when one arrived.
    pub fn member_list(&self, server: ServerId) -> Option<&MemberList> {
        self.member_list
            .as_ref()
            .filter(|list| list.server == server)
    }

    /// Only a list within bounds of a known server replaces the one kept.
    pub(crate) fn member_list_changed(&mut self, list: MemberList) -> Applied {
        if !list.valid() || self.server(list.server).is_none() {
            return Applied::Nothing;
        }
        let server = list.server;
        self.member_list = Some(list);
        Applied::Members(server)
    }
}
