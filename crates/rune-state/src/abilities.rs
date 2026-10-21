//! What the account may do in a conversation, and why not when it may not.
//! The answer joins what the provider can do and what the conversation lets
//! the account do, in one place, so every part of the interface says the
//! same thing.

use rune_i18n::{es, t, tr};
use rune_model::{Channel, ChannelKind, Permissions, Server};
use rune_provider::{
    AuthenticationState, Capabilities, ConnectionState, InvitationDestination, VoiceSupport,
};

/// Whether something is allowed, and the reason when it is not.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Permit {
    Allowed,
    Denied(String),
}

impl Permit {
    pub fn allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    /// Why not, when it is not allowed.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Allowed => None,
            Self::Denied(reason) => Some(reason),
        }
    }
}

impl crate::Store {
    /// Creating a server is an account action, independent of any existing server.
    pub fn create_server(&self) -> Permit {
        let Some(provider) = &self.provider else {
            return Permit::Denied(t("Esta conexión no permite crear servidores").into());
        };
        if !provider.capabilities.manage_servers {
            Permit::Denied(t("Esta conexión no permite crear servidores").into())
        } else if self.me().is_none() {
            Permit::Denied(t("Conectá una cuenta para crear un servidor").into())
        } else if self.from_cache || self.connection != ConnectionState::Connected {
            Permit::Denied(t("Esperá a que la conexión esté disponible").into())
        } else if provider.capabilities.authentication
            && !matches!(self.authentication, AuthenticationState::SignedIn { .. })
        {
            Permit::Denied(t("Iniciá sesión para crear un servidor").into())
        } else {
            Permit::Allowed
        }
    }

    /// Public metadata is shown in the context of the currently selected account.
    /// It grants no membership and needs no channel permission.
    pub fn invitation_preview(&self) -> Permit {
        if !self
            .provider
            .as_ref()
            .is_some_and(|p| p.capabilities.invitation_preview)
        {
            Permit::Denied(t("Esta conexión no permite consultar invitaciones").into())
        } else if self.me().is_none() {
            Permit::Denied(t("Conectá una cuenta para consultar esta invitación").into())
        } else {
            Permit::Allowed
        }
    }

    /// Joining is an account action, not a channel permission or a consequence
    /// of public metadata. The adapter still validates target and account bindings.
    pub fn accept_invitation(&self, destination: InvitationDestination) -> Permit {
        let Some(provider) = &self.provider else {
            return Permit::Denied(t("Esta conexión no permite aceptar invitaciones").into());
        };
        if !provider.capabilities.invitation_accept {
            Permit::Denied(t("Esta conexión no permite aceptar invitaciones").into())
        } else if !provider.capabilities.accepts(destination) {
            Permit::Denied(
                match destination {
                    InvitationDestination::Guest { .. } => {
                        t("Esta conexión no permite entrar como invitado")
                    }
                    InvitationDestination::Group(_) => {
                        t("Esta conexión no permite aceptar invitaciones a grupos")
                    }
                    _ => t("Esta conexión no permite aceptar invitaciones de amistad"),
                }
                .into(),
            )
        } else if self.me().is_none() {
            Permit::Denied(t("Conectá una cuenta para aceptar la invitación").into())
        } else if self.from_cache || self.connection != ConnectionState::Connected {
            Permit::Denied(t("Esperá a que la conexión esté disponible").into())
        } else if provider.capabilities.authentication
            && !matches!(self.authentication, AuthenticationState::SignedIn { .. })
        {
            Permit::Denied(t("Iniciá sesión para aceptar la invitación").into())
        } else if !destination.valid() {
            Permit::Denied(t("La invitación no identifica un destino válido").into())
        } else if matches!(destination, InvitationDestination::Friend(user) if self.me() == Some(user))
        {
            Permit::Denied(t("Es una invitación tuya").into())
        } else if self.invitation_joined(destination) {
            Permit::Denied(
                match destination {
                    InvitationDestination::Group(_) => t("Ya estás en este grupo"),
                    InvitationDestination::Friend(_) => t("Ya son amigos"),
                    _ => t("Ya estás en este servidor"),
                }
                .into(),
            )
        } else {
            Permit::Allowed
        }
    }

    /// Whether the account already has what an invitation to `destination`
    /// gives, as the account's own events told: the server, the group or the
    /// friendship.
    pub fn invitation_joined(&self, destination: InvitationDestination) -> bool {
        match destination {
            InvitationDestination::Server(server) | InvitationDestination::Guest { server, .. } => {
                self.server(server).is_some()
            }
            InvitationDestination::Group(channel) => self.channel(channel).is_some(),
            InvitationDestination::Friend(user) => self
                .relationship(user)
                .is_some_and(|known| known.kind == rune_model::RelationshipKind::Friend),
        }
    }
}

/// What the account may do in one conversation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Abilities {
    /// Read retained local history independently of a network feature.
    pub read_history: Permit,
    /// Fetch an unloaded message's surrounding history from the provider.
    pub message_context: Permit,
    pub write: Permit,
    /// Notify others that the account is writing in this conversation.
    pub typing: Permit,
    pub react: Permit,
    /// Vote on a poll that is still open, or take the vote back.
    pub vote: Permit,
    pub edit_own: Permit,
    pub delete_own: Permit,
    /// Delete what others wrote: moderation.
    pub delete_others: Permit,
    pub attach: Permit,
    pub stickers: Permit,
    pub join_voice: Permit,
    /// Be heard in the voice channel; without it a call only listens.
    pub speak: Permit,
    /// Talk by voice activity; without it a call needs push-to-talk.
    pub voice_activity: Permit,
    /// Send a camera to the others in the voice channel.
    pub send_video: Permit,
    /// Share the account's screen with the others in the voice channel.
    pub share_screen: Permit,
    /// Watch someone's shared screen in the account's call there.
    pub watch_stream: Permit,
    /// See the cameras of the others in the account's call there.
    pub watch_camera: Permit,
    /// Rename, describe, move or delete the channel.
    pub manage: Permit,
    /// Change who may do what in the channel.
    pub manage_permissions: Permit,
    /// Pin and unpin its messages.
    pub pin: Permit,
    /// Read its pinned messages, independently of permission to change them.
    pub view_pins: Permit,
    // Message actions and moderation (P4).
    /// Forward its messages elsewhere; where they go asks for `write` there.
    pub forward: Permit,
    /// Start a poll in it.
    pub create_poll: Permit,
    /// See who reacted to its messages with each emoji.
    pub reaction_list: Permit,
    /// See who voted for each answer of its polls.
    pub poll_voters: Permit,
    // Server administration, rest (P7).
    /// Change a forum's tags, default reaction, layout and order.
    pub forum_settings: Permit,
    // Private calls.
    /// Ring the people of a direct or group conversation to its call, or
    /// stop ringing them; joining it is `join_voice`.
    pub ring: Permit,
    // Emoji and states (2026-10-18).
    /// Use another server's emoji here (its plan may still keep them:
    /// `Store::emoji_perk`). Outside servers nothing asks it.
    pub external_emoji: Permit,
    /// Send another server's stickers here, as `external_emoji`.
    pub external_stickers: Permit,
}

/// What the account may do across one server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerAbilities {
    /// Name, description, look.
    pub manage_server: Permit,
    /// Create, edit and delete channels and categories.
    pub manage_channels: Permit,
    /// Create, edit and delete roles, and give them.
    pub manage_roles: Permit,
    pub kick: Permit,
    pub ban: Permit,
    /// Change other people's names here.
    pub manage_nicks: Permit,
    /// Change the account's own name here.
    pub change_nick: Permit,
    /// Delete the server: only who owns it.
    pub delete: Permit,
    /// Leave the server when supported: everyone but who owns it.
    pub leave: Permit,
    /// Read the log of changes made to the server.
    pub view_audit_log: Permit,
    /// List and revoke the server's invitations.
    pub manage_invites: Permit,
    /// Rename and delete the server's custom emojis and stickers.
    pub manage_expressions: Permit,
    // Administration (19).
    /// Time members out and lift it.
    pub timeout: Permit,
    /// Make invitations; each channel also asks for its own permission.
    pub create_invites: Permit,
    /// Add custom emojis and stickers.
    pub upload_expressions: Permit,
    /// Read the members a page at a time, for the administration.
    pub member_pages: Permit,
    /// Count and remove inactive members without a role.
    pub prune: Permit,
    /// List, make, rename, move and delete the server's webhooks.
    pub manage_webhooks: Permit,
    /// List and remove the server's integrations.
    pub manage_integrations: Permit,
    // Message actions and moderation (P4).
    /// Keep someone's microphone off in voice, or let it be.
    pub mute_members: Permit,
    /// Keep someone's sound off in voice, or let it be.
    pub deafen_members: Permit,
    /// Move someone to another voice channel, or take them out of voice.
    pub move_members: Permit,
    // Server administration, rest (P7).
    /// Change its picture and the traits of its profile.
    pub edit_identity: Permit,
    /// Change its system channel and messages, default notifications, idle
    /// channel, verification and media scan.
    pub edit_safety: Permit,
    /// Pause new invitations, or resume them.
    pub pause_invites: Permit,
    /// Reorder its channels and move them between categories at once.
    pub reorder_channels: Permit,
    /// Give its roles a picture or an emoji, as the server allows.
    pub role_icons: Permit,
    /// Give its roles a gradient of colors, as the server allows.
    pub role_gradients: Permit,
    /// Read how many members have each role.
    pub role_counts: Permit,
    /// Sort the member pages, keep them to recent joins and see how each
    /// member joined.
    pub member_queries: Permit,
    // Rules and onboarding (2026-10-18).
    /// Change its onboarding: only a community has one.
    pub manage_onboarding: Permit,
    // Icon, banner and community (2026-10-18).
    /// Keep a moving picture moving as its icon, as the server allows.
    pub animated_icon: Permit,
    /// Change its banner, and keep a moving one moving, as it allows.
    pub edit_banner: Permit,
    pub animated_banner: Permit,
    /// Choose a community's rules and updates channels.
    pub edit_community: Permit,
    /// Make it a community, or stop it being one: only administration.
    pub toggle_community: Permit,
    // AutoMod (automod).
    /// List, make, change, turn on and off and delete its AutoMod rules.
    pub manage_automod: Permit,
}

/// The abilities across `server` for an account with `permissions` there,
/// with a provider that can do `capabilities`.
pub fn server_abilities(
    server: &Server,
    permissions: Permissions,
    owner: bool,
    capabilities: Capabilities,
) -> ServerAbilities {
    let both = |provider: bool, what: &str, permission: Permissions| {
        if !provider {
            Permit::Denied(tr!("Este proveedor no permite {what}", what))
        } else if !permissions.contains(permission) {
            Permit::Denied(tr!(
                "No tenés permiso para {what} en {server}",
                what,
                server = server.name
            ))
        } else {
            Permit::Allowed
        }
    };
    let member_pages = if !capabilities.member_pages {
        Permit::Denied(t("Este proveedor no permite cargar más miembros").into())
    } else if !permissions.intersects(
        Permissions::KICK
            | Permissions::BAN
            | Permissions::TIMEOUT_MEMBERS
            | Permissions::MANAGE_ROLES
            | Permissions::MANAGE_NICKS
            | Permissions::MANAGE_SERVER,
    ) {
        Permit::Denied(tr!(
            "No tenés permiso para administrar a los miembros de {server}",
            server = server.name
        ))
    } else {
        Permit::Allowed
    };
    // Icon, banner and community (2026-10-18): a moving banner asks both.
    let banner = |moving: bool| match both(
        capabilities.server_banner,
        t("cambiar el banner del servidor"),
        Permissions::MANAGE_SERVER,
    ) {
        Permit::Allowed if !server.options.banner_allowed => {
            Permit::Denied(t("Este servidor todavía no tiene el banner desbloqueado").into())
        }
        Permit::Allowed if moving && !server.options.animated_banner => Permit::Denied(
            t("Este servidor todavía no tiene el banner animado desbloqueado").into(),
        ),
        permit => permit,
    };
    ServerAbilities {
        manage_server: both(
            capabilities.manage_servers,
            t("cambiar el servidor"),
            Permissions::MANAGE_SERVER,
        ),
        manage_channels: both(
            capabilities.manage_channels,
            t("gestionar canales"),
            Permissions::MANAGE_CHANNELS,
        ),
        manage_roles: both(
            capabilities.manage_servers,
            t("gestionar roles"),
            Permissions::MANAGE_ROLES,
        ),
        kick: both(
            capabilities.moderation,
            t("expulsar miembros"),
            Permissions::KICK,
        ),
        ban: both(
            capabilities.moderation,
            t("vetar miembros"),
            Permissions::BAN,
        ),
        manage_nicks: both(
            capabilities.moderation,
            t("cambiar apodos"),
            Permissions::MANAGE_NICKS,
        ),
        change_nick: both(
            capabilities.moderation,
            t("cambiar tu apodo"),
            Permissions::CHANGE_NICK,
        ),
        delete: if !capabilities.manage_servers {
            Permit::Denied(t("Este proveedor no permite borrar servidores").into())
        } else if owner {
            Permit::Allowed
        } else {
            Permit::Denied(t("Solo quien es dueño puede borrar el servidor").into())
        },
        view_audit_log: both(
            capabilities.audit_log,
            t("ver el registro de auditoría"),
            Permissions::VIEW_AUDIT_LOG,
        ),
        manage_invites: both(
            capabilities.manage_invites,
            t("gestionar invitaciones"),
            Permissions::MANAGE_SERVER,
        ),
        manage_expressions: both(
            capabilities.manage_expressions,
            t("gestionar emojis y stickers"),
            Permissions::MANAGE_EMOJI,
        ),
        timeout: both(
            capabilities.timeouts,
            t("aislar miembros"),
            Permissions::TIMEOUT_MEMBERS,
        ),
        create_invites: both(
            capabilities.create_invites,
            t("crear invitaciones"),
            Permissions::CREATE_INVITE,
        ),
        upload_expressions: both(
            capabilities.upload_expressions,
            t("agregar emojis y stickers"),
            Permissions::MANAGE_EMOJI,
        ),
        member_pages: member_pages.clone(),
        prune: both(
            capabilities.prune,
            t("podar miembros"),
            Permissions::KICK | Permissions::MANAGE_SERVER,
        ),
        manage_webhooks: both(
            capabilities.webhooks,
            t("gestionar webhooks"),
            Permissions::MANAGE_WEBHOOKS,
        ),
        manage_integrations: both(
            capabilities.integrations,
            t("gestionar integraciones"),
            Permissions::MANAGE_SERVER,
        ),
        // Leaving needs no management of servers (P4).
        leave: if !capabilities.leave_servers {
            Permit::Denied(t("Este proveedor no permite abandonar servidores").into())
        } else if owner {
            Permit::Denied(t("Sos dueño: no podés abandonar tu propio servidor").into())
        } else {
            Permit::Allowed
        },
        mute_members: both(
            capabilities.voice_moderation,
            t("silenciar en el servidor"),
            Permissions::MUTE_MEMBERS,
        ),
        deafen_members: both(
            capabilities.voice_moderation,
            t("ensordecer en el servidor"),
            Permissions::DEAFEN_MEMBERS,
        ),
        move_members: both(
            capabilities.voice_moderation,
            t("mover miembros de voz"),
            Permissions::MOVE_MEMBERS,
        ),
        // Server administration, rest (P7).
        edit_identity: both(
            capabilities.server_identity,
            t("cambiar la imagen y los rasgos del servidor"),
            Permissions::MANAGE_SERVER,
        ),
        edit_safety: both(
            capabilities.server_safety,
            t("cambiar la seguridad del servidor"),
            Permissions::MANAGE_SERVER,
        ),
        pause_invites: both(
            capabilities.pause_invites,
            t("pausar las invitaciones"),
            Permissions::MANAGE_SERVER,
        ),
        reorder_channels: both(
            capabilities.reorder_channels,
            t("ordenar los canales"),
            Permissions::MANAGE_CHANNELS,
        ),
        role_icons: match both(
            capabilities.role_styles,
            t("darles íconos a los roles"),
            Permissions::MANAGE_ROLES,
        ) {
            Permit::Allowed if !server.options.role_icons => Permit::Denied(
                t("Este servidor todavía no tiene los íconos de rol desbloqueados").into(),
            ),
            permit => permit,
        },
        role_gradients: match both(
            capabilities.role_styles,
            t("darles degradés a los roles"),
            Permissions::MANAGE_ROLES,
        ) {
            Permit::Allowed if !server.options.role_gradients => Permit::Denied(
                t("Este servidor todavía no tiene los degradés de rol desbloqueados").into(),
            ),
            permit => permit,
        },
        role_counts: both(
            capabilities.role_counts,
            t("contar los miembros de cada rol"),
            Permissions::MANAGE_ROLES,
        ),
        member_queries: if !capabilities.member_queries {
            Permit::Denied(t("Este proveedor no permite ordenar ni filtrar a los miembros").into())
        } else {
            member_pages
        },
        // Rules and onboarding (2026-10-18): it gives roles, so it asks both.
        manage_onboarding: match both(
            capabilities.manage_onboarding,
            t("cambiar la incorporación"),
            Permissions::MANAGE_SERVER | Permissions::MANAGE_ROLES,
        ) {
            Permit::Allowed if !server.options.community => Permit::Denied(
                t("Solo una comunidad tiene incorporación: este servidor no lo es").into(),
            ),
            permit => permit,
        },
        // Icon, banner and community (2026-10-18).
        animated_icon: match both(
            capabilities.server_identity,
            t("cambiar la imagen y los rasgos del servidor"),
            Permissions::MANAGE_SERVER,
        ) {
            Permit::Allowed if !server.options.animated_icon => Permit::Denied(
                t("Este servidor todavía no tiene el ícono animado desbloqueado").into(),
            ),
            permit => permit,
        },
        edit_banner: banner(false),
        animated_banner: banner(true),
        edit_community: both(
            capabilities.community,
            t("cambiar los canales de la comunidad"),
            Permissions::MANAGE_SERVER,
        ),
        toggle_community: both(
            capabilities.community,
            t("activar o desactivar la comunidad"),
            Permissions::ADMINISTRATOR,
        ),
        // AutoMod (automod).
        manage_automod: both(
            capabilities.automod,
            t("administrar AutoMod"),
            Permissions::MANAGE_SERVER,
        ),
    }
}

/// The abilities in `channel` with a provider that can do `capabilities`.
pub fn abilities(channel: &Channel, capabilities: Capabilities) -> Abilities {
    let name = if channel.name.is_empty() {
        t("esta conversación").to_owned()
    } else {
        format!("#{}", channel.name)
    };
    let both = |provider: bool, what: &str, permission: Permissions| {
        if !provider {
            Permit::Denied(tr!("Este proveedor no permite {what}", what))
        } else if !channel.permissions.contains(permission) {
            Permit::Denied(tr!("No tenés permiso para {what} en {name}", what, name))
        } else {
            Permit::Allowed
        }
    };
    // What only a server's channel asks for.
    let in_server = |permission: Permissions| {
        if channel.server.is_some() {
            permission
        } else {
            Permissions::NONE
        }
    };
    // Private calls: a direct or group conversation is its call's room, and
    // asks no permission of its own to talk, stream or film there.
    let private = capabilities.private_calls
        && matches!(channel.kind, ChannelKind::Direct | ChannelKind::Group);
    let talk = |provider: bool, what: &str, permission: Permissions| {
        both(
            provider,
            what,
            if private {
                Permissions::NONE
            } else {
                permission
            },
        )
    };
    // A stage is talked in like a voice room, and has the same chat.
    let voice = private
        || (channel.kind.is_voice() && (channel.kind != ChannelKind::Stage || capabilities.stages));
    let messages =
        channel.kind.has_messages() || (channel.kind.is_voice() && capabilities.voice_text);
    let thread = channel.thread.as_ref().filter(|_| channel.is_thread());
    // In a thread, Discord-style permissions use their own bit to write.
    let send = if thread.is_some() {
        Permissions::SEND_IN_THREADS
    } else {
        Permissions::SEND
    };
    // A reply to an archived thread reopens it, as the service does; only a
    // lock keeps it closed to all but moderation. `reopens` is false for
    // what does not reopen it, such as saying the account is typing.
    let both_send = |provider: bool, what: &str, also: Permissions, reopens: bool| {
        if thread.is_some() && !capabilities.thread_replies {
            return Permit::Denied(t("Este proveedor no permite responder en hilos").into());
        }
        let permit = both(provider, what, send | also);
        // Why a thread takes no replies, whatever the permissions say.
        let closed = thread.and_then(|thread| {
            if thread.locked && !channel.permissions.contains(Permissions::MANAGE_THREADS) {
                Some(t(
                    "Este hilo está bloqueado: solo la moderación puede responder",
                ))
            } else if thread.archived && !reopens {
                Some(t(
                    "Este hilo está archivado: respondé para abrirlo otra vez",
                ))
            } else {
                None
            }
        });
        match closed {
            Some(reason) if permit.allowed() => Permit::Denied(reason.into()),
            _ => permit,
        }
    };
    Abilities {
        read_history: both(
            messages,
            t("leer el historial"),
            Permissions::VIEW | Permissions::READ_HISTORY,
        ),
        message_context: both(
            capabilities.message_context && messages,
            t("cargar el contexto de mensajes"),
            Permissions::VIEW | Permissions::READ_HISTORY,
        ),
        write: both_send(
            capabilities.send_messages && messages,
            t("escribir"),
            Permissions::NONE,
            true,
        ),
        typing: both_send(
            capabilities.typing && messages,
            t("avisar que estás escribiendo"),
            Permissions::VIEW,
            false,
        ),
        react: match thread.filter(|thread| thread.archived) {
            Some(_) => {
                Permit::Denied(t("Este hilo está archivado: ya no admite reacciones").into())
            }
            None => both(capabilities.reactions, t("reaccionar"), Permissions::REACT),
        },
        vote: match thread.filter(|thread| thread.archived) {
            Some(_) => Permit::Denied(t("Este hilo está archivado: ya no admite votos").into()),
            None => both(
                capabilities.poll_votes && messages,
                t("votar en encuestas"),
                Permissions::VIEW | Permissions::READ_HISTORY,
            ),
        },
        edit_own: both(
            capabilities.edit_messages,
            t("editar mensajes"),
            Permissions::VIEW,
        ),
        delete_own: both(
            capabilities.delete_messages,
            t("borrar mensajes"),
            Permissions::VIEW,
        ),
        delete_others: both(
            capabilities.delete_messages,
            t("borrar mensajes de otras personas"),
            Permissions::MANAGE_MESSAGES,
        ),
        attach: both_send(
            capabilities.attachments && capabilities.send_messages && messages,
            t("adjuntar archivos"),
            Permissions::VIEW | Permissions::ATTACH,
            // shortcut: files do not reopen an archived thread; the text does.
            false,
        ),
        stickers: both_send(
            capabilities.stickers && messages && channel.server.is_some(),
            t("enviar stickers"),
            Permissions::NONE,
            true,
        ),
        join_voice: if channel.kind == ChannelKind::Stage && !capabilities.stages {
            Permit::Denied(t(NO_STAGES).into())
        } else {
            talk(
                voice && capabilities.voice != VoiceSupport::None,
                t("conectarse a la voz"),
                Permissions::CONNECT,
            )
        },
        speak: talk(
            voice && capabilities.voice != VoiceSupport::None,
            t("hablar"),
            Permissions::SPEAK,
        ),
        voice_activity: talk(
            voice && capabilities.voice != VoiceSupport::None,
            t("hablar por detección de voz"),
            Permissions::USE_VOICE_ACTIVITY,
        ),
        manage: both(
            capabilities.manage_channels && channel.server.is_some(),
            t("gestionar el canal"),
            Permissions::MANAGE_CHANNELS,
        ),
        manage_permissions: both(
            capabilities.manage_channels && channel.server.is_some(),
            t("cambiar los permisos"),
            Permissions::MANAGE_ROLES,
        ),
        // In a direct conversation anyone of it pins; in a server, whoever
        // may pin and read the history, as the official client asks
        // (docs/DISCORD.md): managing messages alone no longer offers it.
        pin: both(
            capabilities.pins && messages,
            t("fijar mensajes"),
            if channel.server.is_none() {
                Permissions::VIEW
            } else {
                Permissions::VIEW | Permissions::PIN_MESSAGES | Permissions::READ_HISTORY
            },
        ),
        // Without history access an endpoint may return an empty list. Do not
        // present that as proof that the conversation has no pinned messages.
        view_pins: both(
            capabilities.pins && messages,
            t("leer los mensajes fijados del historial"),
            Permissions::VIEW | Permissions::READ_HISTORY,
        ),
        // The store also asks for the account's call to be in the channel.
        watch_stream: talk(
            voice && capabilities.watch_streams,
            t("ver transmisiones"),
            Permissions::VIEW | Permissions::CONNECT,
        ),
        watch_camera: talk(
            voice && capabilities.watch_cameras,
            t("ver cámaras"),
            Permissions::VIEW | Permissions::CONNECT,
        ),
        // The local preview needs nothing from the provider; sending does.
        send_video: if voice && !capabilities.video {
            Permit::Denied(t(NO_VIDEO_SENDING).into())
        } else {
            talk(
                voice && capabilities.video,
                t("transmitir video"),
                Permissions::CONNECT | Permissions::STREAM,
            )
        },
        share_screen: if voice && !capabilities.share_screen {
            Permit::Denied(t(NO_VIDEO_SENDING).into())
        } else {
            talk(
                voice && capabilities.share_screen,
                t("compartir la pantalla"),
                Permissions::CONNECT | Permissions::STREAM,
            )
        },
        forward: both(
            capabilities.forward_messages && messages,
            t("reenviar mensajes"),
            Permissions::VIEW | Permissions::READ_HISTORY,
        ),
        // Outside servers anyone of the conversation starts polls.
        create_poll: both_send(
            capabilities.create_polls && messages,
            t("crear encuestas"),
            if channel.server.is_some() {
                Permissions::SEND_POLLS
            } else {
                Permissions::NONE
            },
            true,
        ),
        reaction_list: both(
            capabilities.reaction_lists && messages,
            t("ver quién reaccionó"),
            Permissions::VIEW | Permissions::READ_HISTORY,
        ),
        poll_voters: both(
            capabilities.poll_voters && messages,
            t("ver quién votó"),
            Permissions::VIEW | Permissions::READ_HISTORY,
        ),
        // Server administration, rest (P7).
        forum_settings: both(
            capabilities.forum_settings && channel.kind == ChannelKind::Forum,
            t("cambiar los ajustes del foro"),
            Permissions::MANAGE_CHANNELS,
        ),
        // Private calls.
        ring: both(
            private && capabilities.voice != VoiceSupport::None,
            t("hacer llamadas"),
            Permissions::NONE,
        ),
        // Emoji and states (2026-10-18).
        external_emoji: both(
            true,
            t("usar emojis de otros servidores"),
            in_server(Permissions::EXTERNAL_EMOJI),
        ),
        external_stickers: both(
            true,
            t("usar stickers de otros servidores"),
            in_server(Permissions::EXTERNAL_STICKERS),
        ),
    }
}

/// Why a stage channel cannot be joined with a provider without stages.
pub const NO_STAGES: &str = es("Este proveedor no admite canales de escenario");

/// Why a shared screen or camera stays on this machine.
pub const NO_VIDEO_SENDING: &str = es("La transmisión llega con las llamadas");

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{ChannelId, ServerId};

    fn channel(kind: ChannelKind, permissions: Permissions) -> Channel {
        Channel {
            id: ChannelId(1),
            server: Some(ServerId(1)),
            kind,
            name: "anuncios".into(),
            topic: None,
            topic_pictures: Vec::new(),
            parent: None,
            position: 0,
            recipients: Vec::new(),
            owner: None,
            permissions,
            overwrites: Vec::new(),
            slowmode_secs: 0,
            nsfw: false,
            icon: None,
            thread: None,
            tags: Vec::new(),
            forum: Default::default(),
        }
    }

    fn everything() -> Capabilities {
        Capabilities {
            message_context: true,
            send_messages: true,
            edit_messages: true,
            delete_messages: true,
            reactions: true,
            voice: VoiceSupport::Simulated,
            ..Capabilities::default()
        }
    }

    #[test]
    fn invitation_preview_requires_provider_support_and_current_account() {
        use rune_provider::Provider;
        let provider = rune_provider::mock::MockProvider::new(Default::default());
        let mut store = crate::Store::new();
        store.provider = Some(provider.info());
        store
            .provider
            .as_mut()
            .unwrap()
            .capabilities
            .invitation_preview = true;
        assert!(!store.invitation_preview().allowed());
        store.apply(
            rune_provider::Event::Ready(Box::new(provider.snapshot())),
            std::time::Instant::now(),
        );
        assert!(store.invitation_preview().allowed());
        store
            .provider
            .as_mut()
            .unwrap()
            .capabilities
            .invitation_preview = false;
        assert!(!store.invitation_preview().allowed());
        store
            .provider
            .as_mut()
            .unwrap()
            .capabilities
            .invitation_preview = true;
        store.apply(
            rune_provider::Event::Authentication(rune_provider::AuthenticationState::SignedOut),
            std::time::Instant::now(),
        );
        assert!(!store.invitation_preview().allowed());
    }

    #[test]
    fn invitation_acceptance_requires_live_account_and_a_separate_capability() {
        use rune_provider::{Event, Provider};
        let provider = rune_provider::mock::MockProvider::new(Default::default());
        let target = InvitationDestination::Server(ServerId(999_999));
        let mut store = crate::Store::new();
        assert!(!store.accept_invitation(target).allowed());
        store.provider = Some(provider.info());
        store
            .provider
            .as_mut()
            .unwrap()
            .capabilities
            .invitation_accept = true;
        assert!(!store.accept_invitation(target).allowed(), "no account");
        store.apply(
            Event::Ready(Box::new(provider.snapshot())),
            std::time::Instant::now(),
        );
        assert!(store.accept_invitation(target).allowed());
        let member = InvitationDestination::Server(store.servers()[0].id);
        assert!(!store.accept_invitation(member).allowed(), "already joined");
        assert!(
            !store
                .accept_invitation(InvitationDestination::Server(ServerId(0)))
                .allowed(),
            "invalid server"
        );

        store
            .provider
            .as_mut()
            .unwrap()
            .capabilities
            .invitation_accept = false;
        store
            .provider
            .as_mut()
            .unwrap()
            .capabilities
            .invitation_preview = true;
        assert!(
            !store.accept_invitation(target).allowed(),
            "preview is not acceptance"
        );
        store
            .provider
            .as_mut()
            .unwrap()
            .capabilities
            .invitation_accept = true;
        store.from_cache = true;
        assert!(
            !store.accept_invitation(target).allowed(),
            "cached account data"
        );
        store.from_cache = false;
        for connection in [
            ConnectionState::Connecting,
            ConnectionState::Reconnecting {
                attempt: 1,
                retry_in_ms: 100,
            },
            ConnectionState::Disconnected {
                reason: "offline".into(),
            },
        ] {
            store.connection = connection;
            assert!(!store.accept_invitation(target).allowed());
        }
        store.connection = ConnectionState::Connected;
        assert!(store.accept_invitation(target).allowed());

        store.provider.as_mut().unwrap().capabilities.authentication = true;
        for authentication in [
            AuthenticationState::Unavailable,
            AuthenticationState::SignedOut,
            AuthenticationState::AwaitingExternal,
            AuthenticationState::Exchanging,
            AuthenticationState::Failed {
                reason: "expired".into(),
            },
        ] {
            store.authentication = authentication;
            assert!(!store.accept_invitation(target).allowed());
        }
        store.authentication = AuthenticationState::SignedIn { scopes: vec![] };
        assert!(store.accept_invitation(target).allowed());
        store.apply(
            Event::Authentication(AuthenticationState::SignedOut),
            std::time::Instant::now(),
        );
        assert!(!store.accept_invitation(target).allowed());
    }

    #[test]
    fn each_kind_of_invitation_asks_its_capability_and_what_the_account_has() {
        use rune_model::{ChannelId, UserId};
        use rune_provider::{Event, Provider};
        let provider = rune_provider::mock::MockProvider::new(Default::default());
        let mut store = crate::Store::new();
        store.provider = Some(provider.info());
        store.apply(
            Event::Ready(Box::new(provider.snapshot())),
            std::time::Instant::now(),
        );
        let me = store.me().unwrap();
        let group = store
            .directs()
            .iter()
            .find(|channel| channel.kind == ChannelKind::Group)
            .map(|channel| channel.id)
            .unwrap();
        let friend = store
            .relationships()
            .unwrap()
            .iter()
            .find(|known| known.kind == rune_model::RelationshipKind::Friend)
            .map(|known| known.user)
            .unwrap();
        let server = store.servers()[0].id;
        let new_group = InvitationDestination::Group(ChannelId(777));
        let new_friend = InvitationDestination::Friend(UserId(778));
        let guest = InvitationDestination::Guest {
            server: ServerId(779),
            channel: ChannelId(780),
        };
        for destination in [new_group, new_friend, guest] {
            assert!(
                store.accept_invitation(destination).allowed(),
                "{destination:?}"
            );
        }
        for (destination, reason) in [
            (
                InvitationDestination::Group(group),
                "Ya estás en este grupo",
            ),
            (InvitationDestination::Friend(friend), "Ya son amigos"),
            (InvitationDestination::Friend(me), "Es una invitación tuya"),
            (
                InvitationDestination::Guest {
                    server,
                    channel: ChannelId(780),
                },
                "Ya estás en este servidor",
            ),
            (
                InvitationDestination::Guest {
                    server: ServerId(779),
                    channel: ChannelId(0),
                },
                "La invitación no identifica un destino válido",
            ),
        ] {
            assert_eq!(
                store.accept_invitation(destination).reason(),
                Some(reason),
                "{destination:?}"
            );
        }
        let capabilities = &mut store.provider.as_mut().unwrap().capabilities;
        capabilities.group_invitations = false;
        capabilities.friend_invitations = false;
        capabilities.guest_invitations = false;
        for (destination, reason) in [
            (
                new_group,
                "Esta conexión no permite aceptar invitaciones a grupos",
            ),
            (
                new_friend,
                "Esta conexión no permite aceptar invitaciones de amistad",
            ),
            (guest, "Esta conexión no permite entrar como invitado"),
        ] {
            assert_eq!(store.accept_invitation(destination).reason(), Some(reason));
        }
        assert!(store
            .accept_invitation(InvitationDestination::Server(ServerId(999)))
            .allowed());
    }

    #[test]
    fn typing_needs_provider_support_and_current_access_to_a_message_channel() {
        let supported = Capabilities {
            typing: true,
            ..Capabilities::default()
        };
        let text = channel(ChannelKind::Text, Permissions::MEMBER);
        assert_eq!(
            abilities(&text, Capabilities::default()).typing.reason(),
            Some("Este proveedor no permite avisar que estás escribiendo")
        );
        for kind in [ChannelKind::Text, ChannelKind::Direct, ChannelKind::Group] {
            let mut conversation = text.clone();
            conversation.kind = kind;
            assert!(abilities(&conversation, supported).typing.allowed());
        }
        for kind in [ChannelKind::Voice, ChannelKind::Category] {
            assert!(!abilities(&channel(kind, Permissions::ALL), supported)
                .typing
                .allowed());
        }
        for permissions in [Permissions::NONE, Permissions::VIEW, Permissions::SEND] {
            assert_eq!(
                abilities(&channel(ChannelKind::Text, permissions), supported)
                    .typing
                    .reason(),
                Some("No tenés permiso para avisar que estás escribiendo en #anuncios")
            );
        }
    }

    #[test]
    fn reading_pins_needs_history_access_but_does_not_grant_moderation() {
        let supported = Capabilities {
            pins: true,
            ..Capabilities::default()
        };
        let member = channel(
            ChannelKind::Text,
            Permissions::VIEW | Permissions::READ_HISTORY,
        );
        let permits = abilities(&member, supported);
        assert!(permits.view_pins.allowed());
        assert!(!permits.pin.allowed());
        assert!(!abilities(&member, Capabilities::default())
            .view_pins
            .allowed());
        // This product gate is stricter than an endpoint returning no items:
        // an inaccessible history must not be presented as a verified empty list.
        for permissions in [
            Permissions::VIEW,
            Permissions::READ_HISTORY,
            Permissions::NONE,
        ] {
            assert!(
                abilities(&channel(ChannelKind::Text, permissions), supported)
                    .view_pins
                    .reason()
                    .is_some()
            );
        }
        for kind in [ChannelKind::Voice, ChannelKind::Category] {
            assert!(!abilities(&channel(kind, Permissions::ALL), supported)
                .view_pins
                .allowed());
        }
        let hidden_moderator = channel(ChannelKind::Text, Permissions::MANAGE_MESSAGES);
        assert!(!abilities(&hidden_moderator, supported).pin.allowed());
        // As in the official client: the pin bit and the history, not
        // managing messages.
        let pinner = Permissions::VIEW | Permissions::PIN_MESSAGES | Permissions::READ_HISTORY;
        assert!(abilities(&channel(ChannelKind::Text, pinner), supported)
            .pin
            .allowed());
        assert!(
            !abilities(&channel(ChannelKind::Text, pinner), Capabilities::default())
                .pin
                .allowed()
        );
        for lacking in [
            Permissions::VIEW | Permissions::READ_HISTORY | Permissions::MANAGE_MESSAGES,
            Permissions::VIEW | Permissions::PIN_MESSAGES,
            Permissions::PIN_MESSAGES | Permissions::READ_HISTORY,
        ] {
            let permits = abilities(&channel(ChannelKind::Text, lacking), supported);
            assert!(!permits.pin.allowed(), "{lacking:?}");
        }
    }

    #[test]
    fn the_provider_is_named_before_the_permission() {
        let read_only = channel(
            ChannelKind::Text,
            Permissions(Permissions::VIEW.0 | Permissions::REACT.0),
        );
        let abilities = abilities(&read_only, everything());
        assert_eq!(
            abilities.write.reason(),
            Some("No tenés permiso para escribir en #anuncios")
        );
        assert!(abilities.react.allowed());
        assert_eq!(
            abilities.attach.reason(),
            Some("Este proveedor no permite adjuntar archivos")
        );
        assert!(!abilities.delete_others.allowed());
    }

    #[test]
    fn voice_channels_are_joined_not_written_in() {
        let voice = abilities(
            &channel(ChannelKind::Voice, Permissions::MEMBER),
            everything(),
        );
        assert!(voice.join_voice.allowed() && !voice.write.allowed());
        let text = abilities(
            &channel(ChannelKind::Text, Permissions::MEMBER),
            Capabilities::default(),
        );
        assert!(!text.join_voice.allowed());
        // A provider whose voice channels have a chat writes there like anywhere.
        let chat = Capabilities {
            voice_text: true,
            ..everything()
        };
        let voice = abilities(&channel(ChannelKind::Voice, Permissions::EVERYONE), chat);
        assert!(voice.read_history.allowed() && voice.write.allowed());
        let silent = abilities(
            &channel(ChannelKind::Voice, Permissions::VIEW | Permissions::CONNECT),
            chat,
        );
        assert!(!silent.write.allowed(), "the permission still counts");
    }

    #[test]
    fn speaking_and_voice_activity_need_their_permissions_and_stages_wait() {
        let member = abilities(
            &channel(ChannelKind::Voice, Permissions::EVERYONE),
            everything(),
        );
        assert!(member.speak.allowed() && member.voice_activity.allowed());
        let listener = abilities(
            &channel(ChannelKind::Voice, Permissions::VIEW | Permissions::CONNECT),
            everything(),
        );
        assert!(listener.join_voice.allowed());
        assert_eq!(
            listener.speak.reason(),
            Some("No tenés permiso para hablar en #anuncios")
        );
        assert!(!listener.voice_activity.allowed());
        let stage = abilities(
            &channel(ChannelKind::Stage, Permissions::EVERYONE),
            everything(),
        );
        assert_eq!(stage.join_voice.reason(), Some(NO_STAGES));
        // With a provider that has them, a stage is joined like a room.
        let with_stages = Capabilities {
            stages: true,
            ..everything()
        };
        let stage = abilities(
            &channel(ChannelKind::Stage, Permissions::EVERYONE),
            with_stages,
        );
        assert!(stage.join_voice.allowed() && stage.speak.allowed());
        assert!(
            !stage.write.allowed(),
            "a stage's chat is a voice room's: it needs `voice_text`"
        );
        let chat = abilities(
            &channel(ChannelKind::Stage, Permissions::EVERYONE),
            Capabilities {
                voice_text: true,
                ..with_stages
            },
        );
        assert!(chat.read_history.allowed() && chat.write.allowed());
    }

    #[test]
    fn video_stays_local_until_the_provider_carries_it() {
        let member = channel(ChannelKind::Voice, Permissions::EVERYONE);
        let local = abilities(&member, everything());
        assert_eq!(local.send_video.reason(), Some(NO_VIDEO_SENDING));
        let carried = Capabilities {
            video: true,
            ..everything()
        };
        assert!(abilities(&member, carried).send_video.allowed());
        let silent = channel(ChannelKind::Voice, Permissions::VIEW | Permissions::CONNECT);
        assert!(!abilities(&silent, carried).send_video.allowed());
        // The screen is split from the camera: a provider may carry only it.
        assert_eq!(local.share_screen.reason(), Some(NO_VIDEO_SENDING));
        let screen = Capabilities {
            share_screen: true,
            ..everything()
        };
        assert!(abilities(&member, screen).share_screen.allowed());
        assert!(!abilities(&member, screen).send_video.allowed());
        assert!(!abilities(&silent, screen).share_screen.allowed());
    }

    #[test]
    fn leaving_a_server_requires_provider_support() {
        let abilities = server_abilities(
            &Server {
                stickers: Vec::new(),
                emojis: Vec::new(),
                icon: None,
                id: rune_model::ServerId(1),
                name: "Taller".into(),
                accent: rune_model::Rgb(0),
                roles: Vec::new(),
                owner: None,
                description: None,
                options: Default::default(),
            },
            Permissions::ALL,
            false,
            Capabilities::default(),
        );
        assert!(!abilities.leave.allowed());
    }

    // Server administration, rest (P7).
    #[test]
    fn role_styles_ask_the_server_too_and_safety_needs_managing_it() {
        let mut server = Server {
            stickers: Vec::new(),
            emojis: Vec::new(),
            icon: None,
            id: rune_model::ServerId(1),
            name: "Taller".into(),
            accent: rune_model::Rgb(0),
            roles: Vec::new(),
            owner: None,
            description: None,
            options: Default::default(),
        };
        let all = Capabilities {
            role_styles: true,
            server_safety: true,
            ..Capabilities::default()
        };
        let locked = server_abilities(&server, Permissions::ALL, false, all);
        assert!(locked
            .role_icons
            .reason()
            .is_some_and(|reason| reason.contains("desbloqueados")));
        assert!(locked.edit_safety.allowed());
        server.options.role_icons = true;
        server.options.role_gradients = true;
        let open = server_abilities(&server, Permissions::ALL, false, all);
        assert!(open.role_icons.allowed() && open.role_gradients.allowed());
        let member = server_abilities(&server, Permissions::MEMBER, false, all);
        assert!(!member.edit_safety.allowed() && !member.role_icons.allowed());
        let none = server_abilities(&server, Permissions::ALL, false, Capabilities::default());
        assert!(none
            .reorder_channels
            .reason()
            .is_some_and(|reason| reason.contains("proveedor")));
    }

    // Icon, banner and community (2026-10-19).
    #[test]
    fn a_banner_and_moving_pictures_ask_the_boosts_and_a_community_administration() {
        let mut server = Server {
            stickers: Vec::new(),
            emojis: Vec::new(),
            icon: None,
            id: rune_model::ServerId(1),
            name: "Taller".into(),
            accent: rune_model::Rgb(0),
            roles: Vec::new(),
            owner: None,
            description: None,
            options: Default::default(),
        };
        let all = Capabilities {
            server_identity: true,
            server_banner: true,
            community: true,
            ..Capabilities::default()
        };
        let locked = server_abilities(&server, Permissions::ALL, false, all);
        assert!(!locked.edit_banner.allowed() && !locked.animated_icon.allowed());
        server.options.banner_allowed = true;
        server.options.animated_icon = true;
        let open = server_abilities(&server, Permissions::ALL, false, all);
        assert!(open.edit_banner.allowed() && open.animated_icon.allowed());
        assert!(open
            .animated_banner
            .reason()
            .is_some_and(|reason| reason.contains("animado")));
        assert!(open.toggle_community.allowed() && open.edit_community.allowed());
        let manager = server_abilities(&server, Permissions::MANAGE_SERVER, false, all);
        assert!(manager.edit_community.allowed() && !manager.toggle_community.allowed());
        let none = server_abilities(&server, Permissions::ALL, false, Capabilities::default());
        assert!(!none.edit_banner.allowed() && !none.toggle_community.allowed());
    }
}
