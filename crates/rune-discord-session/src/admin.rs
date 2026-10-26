//! Server administration on the personal session: which commands may leave
//! (checked against what the session knows before anything is sent), what
//! Discord's answers change in the session, and the permission bits RUNE
//! writes. The requests live in `http::admin`; each route is one Serein
//! uses, one Discord documents or one the official client's bundle shows
//! (see docs/DISCORD.md). Fixtures only: none was tried against Discord.

mod audit;
mod decode;
pub(crate) mod favorites;
// Server administration, rest (P7).
pub(crate) mod rest;
#[cfg(test)]
mod tests;

pub(crate) use audit::{action_types, merge as audit_merge, page as audit_page};
pub(crate) use decode::{
    bans, integration, invite, member_page, parse_cursor, valid_code, webhook, MAX_ROWS,
    MEMBERS_PAGE,
};
pub(crate) use favorites::Favorites;

use rune_i18n::es;
use rune_model::{
    permissions, valid_emoji_name, valid_sticker_name, valid_webhook_name, AuditActionKind, Ban,
    Channel, ChannelId, ChannelKind, ExpressionId, Member, OverwriteTarget, Permissions, Rgb, Role,
    RoleId, Server, ServerId, UserId,
};
use rune_provider::{Command, Event, Snapshot, VoiceModeration};
use serde_json::{json, Value};

use crate::{bootstrap, messages::wall_now, wire, LoginError};

/// The longest timeout Discord allows: 28 days.
pub(crate) const TIMEOUT_MAX_MS: i64 = 28 * 24 * 60 * 60 * 1000;
/// The longest an invitation lasts (seven days) and its most uses.
pub(crate) const INVITE_MAX_AGE: u32 = 7 * 24 * 60 * 60;
pub(crate) const INVITE_MAX_USES: u32 = 100;
/// The most characters of an audit log reason, as Discord takes it.
pub(crate) const REASON_MAX: usize = 512;
/// The most history a ban deletes: seven days, in seconds.
pub(crate) const MAX_DELETE_SECONDS: u32 = 7 * 24 * 60 * 60;

const NOT_ALLOWED: &str = es("No tienes permiso para esta acción en el servidor.");
const ABOVE: &str = es("Su rol está a la altura del tuyo o por encima.");
const UNCHANGED: &str = es("No hay cambios para guardar.");

/// Whether `command` is one of the administration this module checks.
pub(crate) fn is_admin(command: &Command) -> bool {
    matches!(
        command,
        Command::CreateServer { .. }
            | Command::EditServer { .. }
            | Command::DeleteServer { .. }
            | Command::LeaveServer { .. }
            | Command::CreateChannel { .. }
            | Command::EditChannel { .. }
            | Command::DeleteChannel { .. }
            | Command::SetOverwrite { .. }
            | Command::RemoveOverwrite { .. }
            | Command::CreateRole { .. }
            | Command::EditRole { .. }
            | Command::DeleteRole { .. }
            | Command::SetMemberRoles { .. }
            | Command::SetNick { .. }
            | Command::Kick { .. }
            | Command::Ban { .. }
            | Command::Unban { .. }
            | Command::LoadBans { .. }
            | Command::LoadInvites { .. }
            | Command::RevokeInvite { .. }
            | Command::RenameExpression { .. }
            | Command::DeleteExpression { .. }
            | Command::LoadAudit { .. }
            | Command::TimeoutMember { .. }
            | Command::CreateInvite { .. }
            | Command::CreateExpression { .. }
            | Command::LoadMembers { .. }
            | Command::PruneMembers { .. }
            | Command::LoadWebhooks { .. }
            | Command::CreateWebhook { .. }
            | Command::EditWebhook { .. }
            | Command::DeleteWebhook { .. }
            | Command::LoadIntegrations { .. }
            | Command::DeleteIntegration { .. }
            | Command::LoadGifFavorites
            | Command::SetGifFavorite { .. }
            // Voice moderation (P4).
            | Command::ModerateVoice { .. }
            // Server administration, rest (P7).
            | Command::LoadServerProfile { .. }
            | Command::ReorderChannels { .. }
            | Command::LoadRoleCounts { .. }
    )
}

/// Whether `command` only reads: its failure is never ambiguous.
pub(crate) fn is_read(command: &Command) -> bool {
    matches!(
        command,
        Command::LoadBans { .. }
            | Command::LoadInvites { .. }
            | Command::LoadAudit { .. }
            | Command::LoadMembers { .. }
            | Command::LoadWebhooks { .. }
            | Command::LoadIntegrations { .. }
            | Command::LoadGifFavorites
            | Command::PruneMembers { remove: false, .. }
            // Server administration, rest (P7).
            | Command::LoadServerProfile { .. }
            | Command::LoadRoleCounts { .. }
    )
}

/// The server `command` is about: the one it names, or its channel's when
/// this session knows that channel.
fn server_of(snapshot: &Snapshot, command: &Command) -> Option<ServerId> {
    match command {
        Command::EditChannel { channel, .. }
        | Command::DeleteChannel { channel }
        | Command::SetOverwrite { channel, .. }
        | Command::RemoveOverwrite { channel, .. } => snapshot
            .channels
            .iter()
            .find(|known| known.id == *channel)
            .and_then(|known| known.server),
        Command::EditServer { server, .. }
        | Command::DeleteServer { server }
        | Command::LeaveServer { server }
        | Command::CreateChannel { server, .. }
        | Command::CreateRole { server, .. }
        | Command::EditRole { server, .. }
        | Command::DeleteRole { server, .. }
        | Command::SetMemberRoles { server, .. }
        | Command::SetNick { server, .. }
        | Command::Kick { server, .. }
        | Command::Ban { server, .. }
        | Command::Unban { server, .. }
        | Command::LoadBans { server, .. }
        | Command::LoadInvites { server, .. }
        | Command::RevokeInvite { server, .. }
        | Command::RenameExpression { server, .. }
        | Command::DeleteExpression { server, .. }
        | Command::LoadAudit { server, .. }
        | Command::TimeoutMember { server, .. }
        | Command::ModerateVoice { server, .. }
        | Command::CreateInvite { server, .. }
        | Command::CreateExpression { server, .. }
        | Command::LoadMembers { server, .. }
        | Command::PruneMembers { server, .. }
        | Command::LoadWebhooks { server, .. }
        | Command::CreateWebhook { server, .. }
        | Command::EditWebhook { server, .. }
        | Command::DeleteWebhook { server, .. }
        | Command::LoadIntegrations { server, .. }
        | Command::DeleteIntegration { server, .. }
        // Server administration, rest (P7).
        | Command::LoadServerProfile { server, .. }
        | Command::ReorderChannels { server, .. }
        | Command::LoadRoleCounts { server, .. } => Some(*server),
        _ => None,
    }
}

/// The account in one server: what it may do there and how high it ranks.
struct Standing<'a> {
    snapshot: &'a Snapshot,
    server: &'a Server,
    me: UserId,
    owner: bool,
    permissions: Permissions,
    /// Its highest role's position; above every role for the owner.
    rank: i64,
}

impl Standing<'_> {
    fn needs(&self, needed: Permissions) -> Result<(), LoginError> {
        if self.permissions.contains(needed) {
            Ok(())
        } else {
            Err(LoginError::new(NOT_ALLOWED))
        }
    }

    /// A channel of this server, not a thread, with what the account may do in it.
    fn channel(&self, id: ChannelId) -> Result<&Channel, LoginError> {
        self.snapshot
            .channels
            .iter()
            .find(|known| known.id == id && known.server == Some(self.server.id))
            .filter(|known| !known.is_thread())
            .ok_or(LoginError::new(es("Ese canal ya no está en el servidor.")))
    }

    fn role(&self, id: RoleId) -> Result<&Role, LoginError> {
        self.server
            .roles
            .iter()
            .find(|role| role.id == id)
            .ok_or(LoginError::new(es("Ese rol ya no existe.")))
    }

    /// A role the account may change, give or take: below its own highest.
    fn role_below(&self, id: RoleId) -> Result<&Role, LoginError> {
        let role = self.role(id)?;
        if self.owner || i64::from(role.position) < self.rank {
            Ok(role)
        } else {
            Err(LoginError::new(es(
                "Ese rol está a la altura del tuyo o por encima.",
            )))
        }
    }

    /// Someone the account may act on: not itself, not who owns the server,
    /// nobody ranked as high. Roles this session does not know rank lowest:
    /// Discord has the last word.
    fn member_below(&self, user: UserId) -> Result<(), LoginError> {
        if user.0 == 0 {
            return Err(wire::invalid());
        }
        if user == self.me {
            return Err(LoginError::new(es("No puedes hacerte esto a ti.")));
        }
        if self.server.owner == Some(user) {
            return Err(LoginError::new(es(
                "Nadie puede hacerle eso a quien es dueño del servidor.",
            )));
        }
        let theirs =
            member(self.snapshot, self.server.id, user).map_or(0, |found| rank(self.server, found));
        if self.owner || theirs < self.rank {
            Ok(())
        } else {
            Err(LoginError::new(ABOVE))
        }
    }

    /// Nobody grants what they do not have themselves.
    fn grants_only_own(&self, mine: Permissions, granted: Permissions) -> Result<(), LoginError> {
        if mine.contains(granted) {
            Ok(())
        } else {
            Err(LoginError::new(es("No puedes dar permisos que no tienes.")))
        }
    }
}

fn member(snapshot: &Snapshot, server: ServerId, user: UserId) -> Option<&Member> {
    snapshot
        .members
        .iter()
        .find(|known| known.server == server && known.user == user)
}

/// A member's highest role position; `@everyone` is 0.
fn rank(server: &Server, member: &Member) -> i64 {
    server
        .roles
        .iter()
        .filter(|role| member.roles.contains(&role.id))
        .map(|role| i64::from(role.position))
        .max()
        .unwrap_or(0)
        .max(0)
}

/// Whether `command` may go to Discord, checked against what this session
/// knows; Discord stays the last word.
pub(crate) fn allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    match command {
        Command::CreateServer { name, .. } => return server_name(name),
        Command::LoadGifFavorites => return Ok(()),
        Command::SetGifFavorite { gif, .. } => return favorites::valid(gif),
        _ => {}
    }
    let me = snapshot.me.as_ref().ok_or_else(wire::invalid)?.id;
    let server = server_of(snapshot, command)
        .and_then(|id| snapshot.servers.iter().find(|known| known.id == id))
        .ok_or(LoginError::new(es(
            "Ese servidor o canal ya no está disponible.",
        )))?;
    let owner = server.owner == Some(me);
    if let Command::LeaveServer { .. } = command {
        return if owner {
            Err(LoginError::new(es(
                "Eres dueño del servidor: elimínalo en vez de salir.",
            )))
        } else {
            Ok(())
        };
    }
    let mine = member(snapshot, server.id, me);
    if mine.is_none() && !owner {
        return Err(LoginError::new(es(
            "Tu membresía en este servidor todavía no cargó.",
        )));
    }
    if mine.is_some_and(|mine| mine.restrictions.at(wall_now()).is_some()) {
        return Err(LoginError::new(es(
            "Tu cuenta está restringida en este servidor: puedes leer, pero no administrar.",
        )));
    }
    let standing = Standing {
        snapshot,
        server,
        me,
        owner,
        permissions: match mine {
            _ if owner => Permissions::ALL,
            Some(mine) => permissions::in_server(server, mine),
            None => Permissions::NONE,
        },
        rank: if owner {
            i64::MAX
        } else {
            mine.map_or(0, |mine| rank(server, mine))
        },
    };
    let everyone = server.everyone_id();
    match command {
        Command::EditServer { edit, .. } => {
            standing.needs(Permissions::MANAGE_SERVER)?;
            if *edit == Default::default() {
                return Err(LoginError::new(UNCHANGED));
            }
            if let Some(name) = &edit.name {
                server_name(name)?;
            }
            if let Some(Some(description)) = &edit.description {
                paragraph(
                    description,
                    300,
                    es("La descripción puede tener hasta 300 caracteres."),
                )?;
            }
            if edit.accent.is_some_and(|accent| accent.0 > 0xFF_FFFF) {
                return Err(wire::invalid());
            }
            // Server administration, rest (P7).
            rest::server_edit(&standing, edit)
        }
        Command::DeleteServer { .. } if !owner => Err(LoginError::new(es(
            "Solo quien es dueño puede eliminar el servidor.",
        ))),
        Command::DeleteServer { .. } => Ok(()),
        Command::CreateChannel {
            kind,
            name,
            parent,
            private,
            ..
        } => {
            if !matches!(
                kind,
                ChannelKind::Text
                    | ChannelKind::Voice
                    | ChannelKind::Category
                    | ChannelKind::Announcement
                    | ChannelKind::Stage
                    | ChannelKind::Forum
            ) {
                return Err(LoginError::new(es(
                    "Ese tipo de canal no se crea en un servidor.",
                )));
            }
            channel_name(name)?;
            match parent {
                Some(_) if *kind == ChannelKind::Category => {
                    Err(LoginError::new(es("Una categoría no va dentro de otra.")))
                }
                Some(parent) => {
                    let parent = standing.channel(*parent)?;
                    if parent.kind != ChannelKind::Category {
                        return Err(LoginError::new(es(
                            "Un canal solo va dentro de una categoría.",
                        )));
                    }
                    holds(parent, Permissions::MANAGE_CHANNELS)
                }
                None => standing.needs(Permissions::MANAGE_CHANNELS),
            }?;
            if *private {
                standing.needs(Permissions::MANAGE_ROLES)?;
            }
            Ok(())
        }
        Command::EditChannel { channel, edit } => {
            let found = standing.channel(*channel)?;
            holds(found, Permissions::MANAGE_CHANNELS)?;
            if *edit == Default::default() {
                return Err(LoginError::new(UNCHANGED));
            }
            if let Some(name) = &edit.name {
                channel_name(name)?;
            }
            if let Some(Some(topic)) = &edit.topic {
                let max = if found.kind == ChannelKind::Forum {
                    4096
                } else {
                    1024
                };
                paragraph(topic, max, es("El tema es demasiado largo."))?;
            }
            if let Some(Some(parent)) = edit.parent {
                let parent = standing.channel(parent)?;
                if found.kind == ChannelKind::Category || parent.kind != ChannelKind::Category {
                    return Err(LoginError::new(es(
                        "Un canal solo va dentro de una categoría.",
                    )));
                }
            }
            if edit.slowmode_secs.is_some_and(|secs| secs > 21_600) {
                return Err(LoginError::new(es("El modo lento llega hasta 6 horas.")));
            }
            if edit.position.is_some_and(|position| position < 0) {
                return Err(wire::invalid());
            }
            // Server administration, rest (P7).
            rest::forum_edit(standing.server, found, edit)
        }
        Command::DeleteChannel { channel } => {
            holds(standing.channel(*channel)?, Permissions::MANAGE_CHANNELS)
        }
        Command::SetOverwrite { channel, overwrite } => {
            let found = standing.channel(*channel)?;
            holds(found, Permissions::MANAGE_ROLES)?;
            overwrite_target(&standing, overwrite.target)?;
            if overwrite.allow.intersects(overwrite.deny) {
                return Err(LoginError::new(es(
                    "Un permiso no puede estar permitido y denegado a la vez.",
                )));
            }
            let before = found
                .overwrites
                .iter()
                .find(|known| known.target == overwrite.target);
            let (allow, deny) = before.map_or((Permissions::NONE, Permissions::NONE), |known| {
                (known.allow, known.deny)
            });
            let changed = Permissions((allow.0 ^ overwrite.allow.0) | (deny.0 ^ overwrite.deny.0));
            standing.grants_only_own(found.permissions, changed)
        }
        Command::RemoveOverwrite { channel, target } => {
            let found = standing.channel(*channel)?;
            holds(found, Permissions::MANAGE_ROLES)?;
            overwrite_target(&standing, *target)?;
            if !found.overwrites.iter().any(|known| known.target == *target) {
                return Err(LoginError::new(es("Ese permiso ya no está en el canal.")));
            }
            Ok(())
        }
        Command::CreateRole { name, .. } => {
            standing.needs(Permissions::MANAGE_ROLES)?;
            role_name(name)
        }
        Command::EditRole { role, edit, .. } => {
            standing.needs(Permissions::MANAGE_ROLES)?;
            let found = if *role == everyone {
                standing.role(*role)?
            } else {
                standing.role_below(*role)?
            };
            if *edit == Default::default() {
                return Err(LoginError::new(UNCHANGED));
            }
            let looks = edit.name.is_some()
                || edit.color.is_some()
                || edit.hoisted.is_some()
                || edit.mentionable.is_some()
                || edit.position.is_some()
                || edit.icon.is_some()
                || edit.gradient.is_some();
            if *role == everyone && looks {
                return Err(LoginError::new(es(
                    "De @everyone solo cambian sus permisos.",
                )));
            }
            if let Some(name) = &edit.name {
                role_name(name)?;
            }
            if edit
                .color
                .flatten()
                .is_some_and(|color| color.0 > 0xFF_FFFF)
            {
                return Err(wire::invalid());
            }
            if let Some(permissions) = edit.permissions {
                standing.grants_only_own(
                    standing.permissions,
                    permissions.without(found.permissions),
                )?;
            }
            // Server administration, rest (P7).
            rest::role_style(server, edit)?;
            match edit.position {
                Some(position) if position < 1 => Err(wire::invalid()),
                Some(position) if !owner && i64::from(position) >= standing.rank => {
                    Err(LoginError::new(es(
                        "No puedes subir un rol a la altura del tuyo o más arriba.",
                    )))
                }
                _ => Ok(()),
            }
        }
        Command::DeleteRole { role, .. } => {
            standing.needs(Permissions::MANAGE_ROLES)?;
            if *role == everyone {
                return Err(LoginError::new(es("@everyone no se puede eliminar.")));
            }
            standing.role_below(*role).map(|_| ())
        }
        Command::SetMemberRoles { user, roles, .. } => {
            standing.needs(Permissions::MANAGE_ROLES)?;
            let Some(found) = member(snapshot, server.id, *user) else {
                return Err(LoginError::new(es(
                    "Carga a esa persona en la lista de miembros antes de cambiar sus roles.",
                )));
            };
            if server.owner == Some(*user) && !owner {
                return Err(LoginError::new(es(
                    "Nadie puede hacerle eso a quien es dueño del servidor.",
                )));
            }
            let mut seen = std::collections::HashSet::new();
            if roles
                .iter()
                .any(|role| *role == everyone || !seen.insert(*role))
            {
                return Err(wire::invalid());
            }
            let changed = roles
                .iter()
                .filter(|role| !found.roles.contains(role))
                .chain(found.roles.iter().filter(|role| !roles.contains(role)));
            for role in changed {
                standing.role_below(*role)?;
            }
            Ok(())
        }
        Command::SetNick { user, nick, .. } => {
            if let Some(nick) = nick {
                let nick = nick.trim();
                if nick.is_empty()
                    || nick.chars().count() > 32
                    || nick.chars().any(char::is_control)
                {
                    return Err(LoginError::new(es(
                        "El apodo puede tener hasta 32 caracteres, sin saltos de línea.",
                    )));
                }
            }
            if *user == me {
                if standing
                    .permissions
                    .intersects(Permissions::CHANGE_NICK | Permissions::MANAGE_NICKS)
                {
                    Ok(())
                } else {
                    Err(LoginError::new(NOT_ALLOWED))
                }
            } else {
                standing.needs(Permissions::MANAGE_NICKS)?;
                standing.member_below(*user)
            }
        }
        Command::Kick { user, reason, .. } => {
            standing.needs(Permissions::KICK)?;
            standing.member_below(*user)?;
            valid_reason(reason.as_deref())
        }
        // Voice moderation (P4): someone in voice in this server, below the
        // account, as the rest of the administration; a move goes to
        // another voice channel of it they may join.
        Command::ModerateVoice { user, change, .. } => {
            standing.needs(match change {
                VoiceModeration::Mute(_) => Permissions::MUTE_MEMBERS,
                VoiceModeration::Deafen(_) => Permissions::DEAFEN_MEMBERS,
                VoiceModeration::Move(_) | VoiceModeration::Disconnect => Permissions::MOVE_MEMBERS,
            })?;
            standing.member_below(*user)?;
            let here = snapshot
                .voice
                .iter()
                .find(|state| {
                    state.user == *user
                        && snapshot.channels.iter().any(|known| {
                            known.id == state.channel && known.server == Some(server.id)
                        })
                })
                .ok_or(LoginError::new(es(
                    "Esa persona ya no está en un canal de voz del servidor.",
                )))?;
            if let VoiceModeration::Move(to) = change {
                let room = standing.channel(*to)?;
                if room.id == here.channel
                    || room.kind != ChannelKind::Voice
                    || !room
                        .permissions
                        .contains(Permissions::VIEW | Permissions::CONNECT)
                {
                    return Err(LoginError::new(es("No se puede mover a ese canal.")));
                }
            }
            Ok(())
        }
        Command::Ban {
            user,
            reason,
            delete_message_seconds,
            ..
        } => {
            standing.needs(Permissions::BAN)?;
            standing.member_below(*user)?;
            // Discord keeps the history of up to seven days to delete.
            if *delete_message_seconds > MAX_DELETE_SECONDS {
                return Err(wire::invalid());
            }
            valid_reason(reason.as_deref())
        }
        Command::Unban { user, .. } => {
            standing.needs(Permissions::BAN)?;
            nonzero(user.0)
        }
        Command::LoadBans { after, limit, .. } => {
            standing.needs(Permissions::BAN)?;
            if *limit == 0 || after.is_some_and(|after| after.0 == 0) {
                return Err(wire::invalid());
            }
            Ok(())
        }
        Command::LoadInvites { .. } | Command::LoadIntegrations { .. } => {
            standing.needs(Permissions::MANAGE_SERVER)
        }
        Command::RevokeInvite { code, .. } => {
            standing.needs(Permissions::MANAGE_SERVER)?;
            if valid_code(code) {
                Ok(())
            } else {
                Err(wire::invalid())
            }
        }
        Command::RenameExpression {
            expression, name, ..
        } => {
            standing.needs(Permissions::MANAGE_EMOJI)?;
            expression_known(server, *expression)?;
            let name = name.trim();
            match expression {
                ExpressionId::Emoji(_) if !valid_emoji_name(name) => Err(LoginError::new(
                    es("El nombre del emoji necesita de 2 a 32 letras sin acentos, números o guiones bajos."),
                )),
                ExpressionId::Sticker(_) if !valid_sticker_name(name) => Err(LoginError::new(
                    es("El nombre del sticker necesita de 2 a 30 caracteres."),
                )),
                _ => Ok(()),
            }
        }
        Command::DeleteExpression { expression, .. } => {
            standing.needs(Permissions::MANAGE_EMOJI)?;
            expression_known(server, *expression)
        }
        Command::CreateExpression { expression, .. } => {
            standing.needs(Permissions::MANAGE_EMOJI)?;
            expression
                .problem()
                .map_or(Ok(()), |problem| Err(LoginError::new(problem)))
        }
        Command::LoadAudit { query, limit, .. } => {
            standing.needs(Permissions::VIEW_AUDIT_LOG)?;
            if *limit == 0
                || query.action == Some(AuditActionKind::Other)
                || query.actor.is_some_and(|actor| actor.0 == 0)
            {
                return Err(LoginError::new(es("Ese filtro no está disponible.")));
            }
            Ok(())
        }
        Command::TimeoutMember {
            user,
            until,
            reason,
            ..
        } => {
            standing.needs(Permissions::TIMEOUT_MEMBERS)?;
            standing.member_below(*user)?;
            // Discord never times out who administers the server.
            if member(snapshot, server.id, *user).is_some_and(|found| {
                permissions::in_server(server, found).contains(Permissions::ADMINISTRATOR)
            }) {
                return Err(LoginError::new(es(
                    "No se puede aislar a quien administra el servidor.",
                )));
            }
            let now = wall_now().0;
            if until.is_some_and(|until| until.0 <= now || until.0 > now + TIMEOUT_MAX_MS) {
                return Err(LoginError::new(es("El aislamiento dura hasta 28 días.")));
            }
            valid_reason(reason.as_deref())
        }
        Command::CreateInvite {
            channel,
            max_age_secs,
            max_uses,
            ..
        } => {
            let found = standing.channel(*channel)?;
            if found.kind == ChannelKind::Category {
                return Err(LoginError::new(es(
                    "Se invita a un canal, no a una categoría.",
                )));
            }
            holds(found, Permissions::CREATE_INVITE)?;
            if *max_age_secs > INVITE_MAX_AGE || *max_uses > INVITE_MAX_USES {
                return Err(LoginError::new(es(
                    "Una invitación dura hasta 7 días y sirve hasta 100 veces.",
                )));
            }
            Ok(())
        }
        Command::LoadMembers { after, .. } => {
            if !standing.permissions.intersects(
                Permissions::KICK
                    | Permissions::BAN
                    | Permissions::TIMEOUT_MEMBERS
                    | Permissions::MANAGE_ROLES
                    | Permissions::MANAGE_NICKS
                    | Permissions::MANAGE_SERVER,
            ) {
                return Err(LoginError::new(NOT_ALLOWED));
            }
            match after {
                Some(cursor) if parse_cursor(cursor).is_none() => {
                    Err(LoginError::new(es("El cursor de la página no es válido.")))
                }
                _ => Ok(()),
            }
        }
        Command::PruneMembers { days, .. } => {
            standing.needs(Permissions::KICK | Permissions::MANAGE_SERVER)?;
            if (1..=30).contains(days) {
                Ok(())
            } else {
                Err(LoginError::new(es(
                    "Se poda por 1 a 30 días sin actividad.",
                )))
            }
        }
        Command::LoadWebhooks { .. } => standing.needs(Permissions::MANAGE_WEBHOOKS),
        Command::CreateWebhook { channel, name, .. } => {
            standing.needs(Permissions::MANAGE_WEBHOOKS)?;
            webhook_channel(standing.channel(*channel)?)?;
            webhook_name(name)
        }
        Command::EditWebhook {
            webhook,
            name,
            channel,
            ..
        } => {
            standing.needs(Permissions::MANAGE_WEBHOOKS)?;
            nonzero(webhook.0)?;
            if name.is_none() && channel.is_none() {
                return Err(LoginError::new(UNCHANGED));
            }
            if let Some(channel) = channel {
                webhook_channel(standing.channel(*channel)?)?;
            }
            name.as_deref().map_or(Ok(()), webhook_name)
        }
        Command::DeleteWebhook { webhook, .. } => {
            standing.needs(Permissions::MANAGE_WEBHOOKS)?;
            nonzero(webhook.0)
        }
        Command::DeleteIntegration { integration, .. } => {
            standing.needs(Permissions::MANAGE_SERVER)?;
            nonzero(integration.0)
        }
        // Server administration, rest (P7).
        Command::LoadServerProfile { .. } => Ok(()),
        Command::LoadRoleCounts { .. } => standing.needs(Permissions::MANAGE_ROLES),
        Command::ReorderChannels { moves, .. } => rest::moves(&standing, moves),
        _ => Err(LoginError::new(es(
            "Esta acción todavía no está disponible.",
        ))),
    }
}

/// What a request in flight must still hold: its server, when it names one,
/// is still the account's. Everything else is Discord's to answer: the
/// Gateway often reports a change before the answer to the write that made it.
pub(crate) fn in_flight(snapshot: &Snapshot, command: &Command) -> Option<LoginError> {
    if matches!(
        command,
        Command::DeleteServer { .. } | Command::LeaveServer { .. }
    ) {
        return None;
    }
    server_of(snapshot, command)
        .filter(|server| !snapshot.servers.iter().any(|known| known.id == *server))
        .map(|_| LoginError::new(es("Ya no estás en ese servidor.")))
}

/// What a request needs of the session, read when it leaves.
pub(crate) struct Context {
    pub me: Option<UserId>,
    /// The server it is about, when the session knows it.
    pub server: Option<ServerId>,
    /// The member it names, as this session knows them.
    pub member: Option<Member>,
    // Server administration, rest (P7).
    /// The role it changes, as this session knows it.
    pub role: Option<Role>,
}

pub(crate) fn context(snapshot: &Snapshot, command: &Command) -> Context {
    let server = server_of(snapshot, command);
    let user = match command {
        Command::SetMemberRoles { user, .. } | Command::SetNick { user, .. } => Some(*user),
        _ => None,
    };
    let role = match command {
        Command::EditRole { server, role, .. } => snapshot
            .servers
            .iter()
            .find(|known| known.id == *server)
            .and_then(|known| known.roles.iter().find(|found| found.id == *role))
            .cloned(),
        _ => None,
    };
    Context {
        me: snapshot.me.as_ref().map(|me| me.id),
        server,
        member: server
            .zip(user)
            .and_then(|(server, user)| member(snapshot, server, user))
            .cloned(),
        role,
    }
}

/// What an answer changes in the session besides the events it reports,
/// applied in the provider loop as the Gateway's own dispatch would be.
#[derive(Debug)]
pub(crate) enum Echo {
    /// As if the Gateway had sent `kind` with this data.
    Dispatch(&'static str, Value),
    /// The same, for members a page already reports: kept, not reported twice.
    Quiet(&'static str, Value),
    /// An emoji or sticker as the answer names it; `None`: it is gone.
    Expression {
        server: ServerId,
        expression: ExpressionId,
        answer: Option<Value>,
    },
}

/// Keep what an answer confirmed, with the same validation, permission
/// recompute and channel diffs as a Gateway dispatch, and report it before
/// the answer's own events (so a server or channel precedes its `Created`).
/// Echoes that do not apply leave the session as it was: the Gateway's
/// dispatch still carries them.
pub(crate) fn confirmed(
    snapshot: &mut Snapshot,
    echoes: Vec<Echo>,
    events: Vec<Event>,
) -> Vec<Event> {
    let mut out = Vec::new();
    if !echoes.is_empty() {
        let mut trial = snapshot.clone();
        let applied = echoes.into_iter().try_fold(Vec::new(), |mut done, echo| {
            done.extend(apply(&mut trial, echo)?);
            Ok::<_, LoginError>(done)
        });
        if let Ok(done) = applied {
            *snapshot = trial;
            out = done;
        }
    }
    for mut event in events {
        if let Event::AuditLoaded {
            server, entries, ..
        } = &mut event
        {
            audit::names(snapshot, *server, entries);
        }
        out.push(event);
    }
    out
}

fn apply(snapshot: &mut Snapshot, echo: Echo) -> Result<Vec<Event>, LoginError> {
    match echo {
        Echo::Dispatch(kind, data) if made_already(snapshot, kind, &data) => Ok(Vec::new()),
        Echo::Dispatch(kind, data) => bootstrap::update(snapshot, kind, &data),
        Echo::Quiet(kind, data) => Ok(bootstrap::update(snapshot, kind, &data)?
            .into_iter()
            .filter(|event| !matches!(event, Event::MemberChanged(_) | Event::UserChanged(_)))
            .collect()),
        Echo::Expression {
            server,
            expression,
            answer,
        } => {
            let Some(found) = snapshot.servers.iter_mut().find(|known| known.id == server) else {
                return Ok(Vec::new());
            };
            match (expression, answer) {
                (ExpressionId::Emoji(id), Some(answer)) => {
                    let emoji = crate::emoji::catalog(&json!([answer]), server)?
                        .pop()
                        .filter(|emoji| emoji.id == id)
                        .ok_or_else(wire::invalid)?;
                    match found.emojis.iter_mut().find(|known| known.id == id) {
                        Some(known) => *known = emoji,
                        None => found.emojis.push(emoji),
                    }
                }
                (ExpressionId::Emoji(id), None) => found.emojis.retain(|known| known.id != id),
                (ExpressionId::Sticker(id), Some(answer)) => {
                    let sticker = crate::stickers::decode(&json!([answer]), true)?
                        .pop()
                        .filter(|sticker| sticker.id == id)
                        .ok_or_else(wire::invalid)?;
                    match found.stickers.iter_mut().find(|known| known.id == id) {
                        Some(known) => *known = sticker,
                        None => found.stickers.push(sticker),
                    }
                }
                (ExpressionId::Sticker(id), None) => found.stickers.retain(|known| known.id != id),
            }
            Ok(vec![Event::ServerChanged(found.clone())])
        }
    }
}

/// A creation the Gateway reported first: its copy, often fuller, stays.
fn made_already(snapshot: &Snapshot, kind: &str, data: &Value) -> bool {
    let id = |value: &Value| wire::id(value).ok();
    match kind {
        "GUILD_CREATE" => {
            id(&data["id"]).is_some_and(|id| snapshot.servers.iter().any(|known| known.id.0 == id))
        }
        "CHANNEL_CREATE" => {
            id(&data["id"]).is_some_and(|id| snapshot.channels.iter().any(|known| known.id.0 == id))
        }
        "GUILD_ROLE_CREATE" => id(&data["role"]["id"]).is_some_and(|id| {
            snapshot
                .servers
                .iter()
                .any(|known| known.roles.iter().any(|role| role.id.0 == id))
        }),
        _ => false,
    }
}

/// Bans the Gateway reports (documented `GUILD_BAN_ADD` / `GUILD_BAN_REMOVE`);
/// `None` for any other dispatch. A malformed one, or one of a server this
/// session does not know, is dropped: it grants nothing.
pub(crate) fn dispatch(snapshot: &Snapshot, kind: &str, data: &Value) -> Option<Vec<Event>> {
    let added = match kind {
        "GUILD_BAN_ADD" => true,
        "GUILD_BAN_REMOVE" => false,
        _ => return None,
    };
    let server = wire::id(&data["guild_id"])
        .ok()
        .map(ServerId)
        .filter(|id| snapshot.servers.iter().any(|known| known.id == *id));
    let (Some(server), Ok(user)) = (server, wire::id(&data["user"]["id"]).map(UserId)) else {
        return Some(Vec::new());
    };
    let mut events: Vec<Event> = wire::user(&data["user"])
        .ok()
        .map(Event::UserChanged)
        .into_iter()
        .collect();
    events.push(if added {
        Event::BanAdded {
            server,
            ban: Ban { user, reason: None },
        }
    } else {
        Event::BanRemoved { server, user }
    });
    Some(events)
}

/// A server's color as Discord's profile carries it: `"#rrggbb"` or a number.
pub(crate) fn accent(value: &Value) -> Option<Rgb> {
    let rgb = match value {
        Value::String(text) => {
            let hex = text.strip_prefix('#')?;
            if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            u32::from_str_radix(hex, 16).ok()?
        }
        Value::Number(number) => u32::try_from(number.as_u64()?).ok()?,
        _ => return None,
    };
    (rgb <= 0xFF_FFFF).then_some(Rgb(rgb))
}

/// Discord's bits for `permissions`, the inverse of what bootstrap reads.
pub(crate) fn discord_bits(permissions: Permissions) -> u128 {
    bootstrap::PERMISSION_BITS
        .iter()
        .filter(|(_, permission)| permissions.contains(*permission))
        .fold(0, |bits, (bit, _)| bits | 1 << bit)
}

/// `raw` with the bits RUNE models replaced by `permissions`; the bits it
/// does not model stay as Discord had them.
pub(crate) fn merged_bits(raw: u128, permissions: Permissions) -> u128 {
    (raw & !discord_bits(Permissions::ALL)) | discord_bits(permissions)
}

/// Discord's permission value: a decimal string (or a number).
pub(crate) fn raw_bits(value: &Value) -> Option<u128> {
    match value {
        Value::String(text)
            if !text.is_empty() && text.len() <= 39 && text.bytes().all(|b| b.is_ascii_digit()) =>
        {
            text.parse().ok()
        }
        Value::Number(number) => number.as_u64().map(u128::from),
        _ => None,
    }
}

fn holds(channel: &Channel, needed: Permissions) -> Result<(), LoginError> {
    if channel.permissions.contains(needed) {
        Ok(())
    } else {
        Err(LoginError::new(es(
            "No tienes permiso para esta acción en el canal.",
        )))
    }
}

/// Whom an overwrite is for: a role the account ranks above (`@everyone`
/// always), or someone.
fn overwrite_target(standing: &Standing, target: OverwriteTarget) -> Result<(), LoginError> {
    match target {
        OverwriteTarget::Role(role) if role == standing.server.everyone_id() => Ok(()),
        OverwriteTarget::Role(role) => standing.role_below(role).map(|_| ()),
        OverwriteTarget::Member(user) => nonzero(user.0),
    }
}

fn expression_known(server: &Server, expression: ExpressionId) -> Result<(), LoginError> {
    let known = match expression {
        ExpressionId::Emoji(id) => server.emojis.iter().any(|emoji| emoji.id == id),
        ExpressionId::Sticker(id) => server.stickers.iter().any(|sticker| sticker.id == id),
    };
    if known {
        Ok(())
    } else {
        Err(LoginError::new(es("Esa expresión ya no existe.")))
    }
}

fn webhook_channel(channel: &Channel) -> Result<(), LoginError> {
    if matches!(
        channel.kind,
        ChannelKind::Text | ChannelKind::Announcement | ChannelKind::Forum
    ) {
        Ok(())
    } else {
        Err(LoginError::new(es(
            "Un webhook publica en un canal de texto, de anuncios o un foro.",
        )))
    }
}

fn webhook_name(name: &str) -> Result<(), LoginError> {
    if valid_webhook_name(name.trim()) {
        Ok(())
    } else {
        Err(LoginError::new(es(
            "El nombre necesita de 1 a 80 caracteres, sin «discord» ni «clyde».",
        )))
    }
}

fn nonzero(id: u64) -> Result<(), LoginError> {
    if id == 0 {
        Err(wire::invalid())
    } else {
        Ok(())
    }
}

/// One line of `min..=max` characters once trimmed.
fn line(text: &str, min: usize, max: usize, reason: &'static str) -> Result<(), LoginError> {
    let text = text.trim();
    if (min..=max).contains(&text.chars().count()) && !text.chars().any(char::is_control) {
        Ok(())
    } else {
        Err(LoginError::new(reason))
    }
}

fn paragraph(text: &str, max: usize, reason: &'static str) -> Result<(), LoginError> {
    if text.chars().count() <= max
        && !text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        Ok(())
    } else {
        Err(LoginError::new(reason))
    }
}

fn server_name(name: &str) -> Result<(), LoginError> {
    line(
        name,
        2,
        100,
        es("El nombre del servidor necesita de 2 a 100 caracteres."),
    )
}

fn channel_name(name: &str) -> Result<(), LoginError> {
    line(
        name,
        1,
        100,
        es("El nombre del canal necesita de 1 a 100 caracteres."),
    )
}

fn role_name(name: &str) -> Result<(), LoginError> {
    line(
        name,
        1,
        100,
        es("El nombre del rol necesita de 1 a 100 caracteres."),
    )
}

/// An audit log reason: nothing, or one line of up to [`REASON_MAX`].
fn valid_reason(reason: Option<&str>) -> Result<(), LoginError> {
    match reason.map(str::trim).filter(|reason| !reason.is_empty()) {
        Some(reason) => line(
            reason,
            1,
            REASON_MAX,
            es("El motivo puede tener hasta 512 caracteres, sin saltos de línea."),
        ),
        None => Ok(()),
    }
}
