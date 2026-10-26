//! Apps' commands and interactions (P3), for this session only: the
//! commands a conversation offers, the one interaction in flight and what
//! became of it, the modal an app opened, the choices it suggests, and the
//! replies only the account sees. The interface asks
//! [`Store::interaction_abilities`] whether it may use an app at all, and
//! [`Store::usable_commands`] which commands it may run where.

use std::time::{Duration, Instant};

use rune_i18n::{t, tr};
use rune_model::components::Component;
use rune_model::interactions::{AppCommand, Interaction, Modal, OptionChoice};
use rune_model::{ChannelId, ChannelKind, Message, MessageId, Permissions, ServerId};
use rune_provider::{AuthenticationState, Command, ConnectionState, Failure, RequestId};

use crate::{Applied, Permit, Store};

/// How long an interaction waits for its app before RUNE stops waiting.
pub const INTERACTION_WAIT: Duration = Duration::from_secs(30);
/// How long a modal sent with files waits, uploads included: as the
/// official client waits (docs/DISCORD.md).
pub const UPLOAD_WAIT: Duration = Duration::from_secs(2 * 60);
/// How long a list of commands is kept before it is asked for again.
pub const COMMANDS_KEPT: Duration = Duration::from_secs(5 * 60);
/// Replies only the account sees, kept at once at most.
const EPHEMERAL_KEPT: usize = 16;

/// What RUNE keeps of apps during the session.
#[derive(Debug, Default)]
pub(crate) struct InteractionState {
    pending: Option<Pending>,
    problem: Option<Problem>,
    modal: Option<(ChannelId, Modal)>,
    catalog: Catalog,
    choices: Option<(RequestId, Vec<OptionChoice>)>,
    /// The replies only the account sees, the oldest first.
    ephemeral: Vec<(ChannelId, MessageId)>,
}

/// The interaction in flight.
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub request: RequestId,
    pub channel: ChannelId,
    /// The message and the part used, for a button or a menu.
    pub part: Option<(MessageId, String)>,
    /// A modal sent, which closes once its app answers.
    pub modal: bool,
    pub since: Instant,
    /// The app got it.
    pub received: bool,
    /// How long it is waited for from `since`: [`INTERACTION_WAIT`], or
    /// [`UPLOAD_WAIT`] with files.
    pub wait: Duration,
}

/// Why the last interaction did not go through, and where.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub channel: ChannelId,
    pub part: Option<(MessageId, String)>,
    pub reason: String,
}

/// Where a list of commands holds: a server's channels, or one direct
/// conversation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scope {
    Server(ServerId),
    Direct(ChannelId),
}

#[derive(Debug, Default)]
struct Catalog {
    scope: Option<Scope>,
    request: RequestId,
    loading: bool,
    loaded: Option<Instant>,
    commands: Vec<AppCommand>,
    error: Option<String>,
}

/// What the account may do with apps in one conversation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteractionAbilities {
    /// Press apps' buttons, pick in their menus, fill their modals.
    pub parts: Permit,
    /// List and run apps' commands.
    pub commands: Permit,
    /// Attach files to their modals.
    pub files: Permit,
}

/// How the list of a conversation's commands is.
#[derive(Clone, Debug, PartialEq)]
pub enum CommandsState<'a> {
    /// Not asked for yet.
    Unknown,
    Loading,
    Failed(&'a str),
    Loaded,
}

impl Store {
    /// Whether the account may use apps in `channel`, and why not.
    pub fn interaction_abilities(&self, channel: ChannelId) -> InteractionAbilities {
        let capabilities = self.capabilities();
        let Some(found) = self.channel(channel) else {
            let gone = Permit::Denied(t("La conversación no está disponible").into());
            return InteractionAbilities {
                parts: gone.clone(),
                commands: gone.clone(),
                files: gone,
            };
        };
        let connected = self.me().is_some()
            && !self.from_cache
            && self.connection == ConnectionState::Connected
            && (!capabilities.authentication
                || matches!(self.authentication, AuthenticationState::SignedIn { .. }));
        let restricted = found
            .server
            .and_then(|server| Some((self.server(server)?, self.restriction(server)?)))
            .map(|(server, restriction)| crate::profiles::restricted(restriction, &server.name));
        let check = |declared: bool, what: &str| {
            if !declared {
                Permit::Denied(tr!("Este proveedor no permite {what}", what))
            } else if !connected {
                Permit::Denied(t("Espera a que la conexión esté disponible").into())
            } else if let Some(restricted) = restricted.clone() {
                restricted
            } else if !found.permissions.contains(Permissions::VIEW) {
                Permit::Denied(t("No puedes ver esta conversación").into())
            } else {
                Permit::Allowed
            }
        };
        let parts = check(capabilities.interactions, t("usar aplicaciones"));
        let mut commands = check(
            capabilities.app_commands,
            t("usar comandos de aplicaciones"),
        );
        if commands.allowed() {
            let write = self.abilities(channel).map(|abilities| abilities.write);
            commands = match write {
                Some(Permit::Denied(reason)) => Permit::Denied(reason),
                None => Permit::Denied(t("La conversación no está disponible").into()),
                Some(Permit::Allowed) if found.server.is_some() => {
                    if found.permissions.contains(Permissions::USE_APP_COMMANDS) {
                        Permit::Allowed
                    } else {
                        Permit::Denied(
                            t("No tienes permiso para usar comandos de aplicaciones aquí").into(),
                        )
                    }
                }
                // Outside servers, as Serein: only a conversation with a bot.
                Some(Permit::Allowed) => {
                    let bot = found.kind == ChannelKind::Direct
                        && found
                            .recipients
                            .iter()
                            .any(|user| self.user(*user).is_some_and(|user| user.bot));
                    if bot {
                        Permit::Allowed
                    } else {
                        Permit::Denied(t("Esta conversación no tiene aplicaciones").into())
                    }
                }
            };
        }
        // A modal's files are staged in the conversation: they need to see
        // it and attach there, not to write in it, as the provider asks (a
        // read-only channel's form may take files, as in Serein).
        let files = match check(capabilities.attachments, t("adjuntar archivos")) {
            _ if !parts.allowed() => parts.clone(),
            Permit::Allowed if !found.permissions.contains(Permissions::ATTACH) => {
                Permit::Denied(t("No tienes permiso para adjuntar archivos aquí").into())
            }
            permit => permit,
        };
        InteractionAbilities {
            parts,
            commands,
            files,
        }
    }

    /// Where the commands of `channel` are listed: its server, or itself.
    fn command_scope(&self, channel: ChannelId) -> Option<Scope> {
        let found = self.channel(channel)?;
        Some(match found.server {
            Some(server) => Scope::Server(server),
            None => Scope::Direct(channel),
        })
    }

    /// Ask for the commands of `channel` unless they are known, being read
    /// or kept for less than [`COMMANDS_KEPT`]; `refresh` asks again anyway.
    pub fn load_commands(
        &mut self,
        channel: ChannelId,
        request: RequestId,
        now: Instant,
        refresh: bool,
    ) -> Option<Command> {
        if !self.interaction_abilities(channel).commands.allowed() {
            return None;
        }
        let scope = self.command_scope(channel)?;
        let catalog = &self.interactions.catalog;
        let fresh = catalog.scope == Some(scope)
            && (catalog.loading
                || catalog
                    .loaded
                    .is_some_and(|loaded| now.duration_since(loaded) < COMMANDS_KEPT));
        // A refresh waits only for a read in flight.
        if fresh && (catalog.loading || !refresh) {
            return None;
        }
        self.interactions.catalog = Catalog {
            scope: Some(scope),
            request,
            loading: true,
            ..Catalog::default()
        };
        Some(Command::LoadAppCommands { request, channel })
    }

    /// How the list of `channel`'s commands is.
    pub fn commands_state(&self, channel: ChannelId) -> CommandsState<'_> {
        let catalog = &self.interactions.catalog;
        if self.command_scope(channel).is_none() || catalog.scope != self.command_scope(channel) {
            return CommandsState::Unknown;
        }
        match (&catalog.error, catalog.loading) {
            (_, true) => CommandsState::Loading,
            (Some(error), _) => CommandsState::Failed(error),
            (None, false) => CommandsState::Loaded,
        }
    }

    /// The commands the account may run in `channel`: where they belong,
    /// with what they need, as the server's overrides say.
    pub fn usable_commands(&self, channel: ChannelId) -> Vec<&AppCommand> {
        if !self.interaction_abilities(channel).commands.allowed()
            || self.commands_state(channel) != CommandsState::Loaded
        {
            return Vec::new();
        }
        self.interactions
            .catalog
            .commands
            .iter()
            .filter(|command| self.may_run(channel, command))
            .collect()
    }

    /// Whether `command` runs in `channel` for the account.
    fn may_run(&self, channel: ChannelId, command: &AppCommand) -> bool {
        let Some(found) = self.channel(channel) else {
            return false;
        };
        let Some(server) = found.server else {
            return command.in_direct && command.server.is_none();
        };
        if !command.in_servers
            || command.server.is_some_and(|only| only != server)
            || command.nsfw && !self.adult(found)
        {
            return false;
        }
        if found.permissions.contains(Permissions::ADMINISTRATOR) {
            return true;
        }
        // A thread follows its channel's overrides.
        let place = if found.is_thread() {
            found.parent.unwrap_or(channel)
        } else {
            channel
        };
        let base = command
            .needs
            .is_none_or(|needs| found.permissions.contains(needs));
        let access = &command.access;
        let location = access
            .channels
            .iter()
            .find(|(id, _)| *id == place)
            .map(|(_, allowed)| *allowed)
            .or(access.all_channels)
            .unwrap_or(true);
        access.granted.unwrap_or(base) && location
    }

    /// Whether `channel`, or the channel of a thread, is for adults.
    fn adult(&self, channel: &rune_model::Channel) -> bool {
        channel.nsfw
            || channel
                .parent
                .filter(|_| channel.is_thread())
                .and_then(|parent| self.channel(parent))
                .is_some_and(|parent| parent.nsfw)
    }

    /// Tell an app what the account did in `channel`, as `request`; why not
    /// when it cannot go. One at a time: a second waits for the first.
    pub fn begin_interaction(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        interaction: Interaction,
        now: Instant,
    ) -> Result<Command, String> {
        let abilities = self.interaction_abilities(channel);
        let permit = match &interaction {
            Interaction::Command(_) => abilities.commands,
            _ => abilities.parts,
        };
        if let Some(reason) = permit.reason() {
            return Err(reason.to_owned());
        }
        let files = !interaction.files().is_empty();
        if let Some(reason) = abilities.files.reason().filter(|_| files) {
            return Err(reason.to_owned());
        }
        if self.interaction_pending(now).is_some() {
            return Err(t("Espera la respuesta de la aplicación anterior").into());
        }
        let part = match &interaction {
            Interaction::Part {
                message,
                custom_id,
                values,
            } => {
                let found = self
                    .timeline(channel)
                    .and_then(|timeline| timeline.get(*message))
                    .filter(|found| !found.delivery.is_local())
                    .ok_or(t("El mensaje ya no está cargado"))?;
                if found.components.app.is_none() || found.forwarded.is_some() {
                    return Err(t("Este mensaje no pertenece a una aplicación").into());
                }
                let usable =
                    match rune_model::components::control(&found.components.parts, custom_id) {
                        Some(Component::Button(button)) => {
                            !button.disabled && button.url.is_none() && values.is_empty()
                        }
                        Some(Component::Select(menu)) => !menu.disabled && menu.accepts(values),
                        _ => false,
                    };
                if !usable {
                    return Err(t("Ese control ya no se puede usar").into());
                }
                Some((*message, custom_id.clone()))
            }
            Interaction::Modal { id, .. } => {
                if self
                    .interactions
                    .modal
                    .as_ref()
                    .is_none_or(|(open, modal)| *open != channel || modal.id != *id)
                {
                    return Err(t("El formulario ya no está abierto").into());
                }
                None
            }
            Interaction::Command(used) => {
                self.usable_commands(channel)
                    .into_iter()
                    .find(|command| command.id == used.command && command.app == used.app)
                    .ok_or(t("Ese comando ya no está disponible aquí"))?
                    .check(used)?;
                None
            }
        };
        let modal = matches!(interaction, Interaction::Modal { .. });
        self.interactions.pending = Some(Pending {
            request,
            channel,
            part,
            modal,
            since: now,
            received: false,
            wait: if files { UPLOAD_WAIT } else { INTERACTION_WAIT },
        });
        self.interactions.problem = None;
        self.revision += 1;
        Ok(Command::Interact {
            request,
            channel,
            interaction,
            files: Vec::new(),
        })
    }

    /// The interaction in flight, while its app may still answer.
    pub fn interaction_pending(&self, now: Instant) -> Option<&Pending> {
        self.interactions
            .pending
            .as_ref()
            .filter(|pending| now.saturating_duration_since(pending.since) < pending.wait)
    }

    /// Why the last interaction did not go through: a refusal, or no
    /// answer in [`INTERACTION_WAIT`].
    pub fn interaction_problem(&self, now: Instant) -> Option<Problem> {
        if let Some(pending) = self
            .interactions
            .pending
            .as_ref()
            .filter(|_| self.interaction_pending(now).is_none())
        {
            return Some(Problem {
                channel: pending.channel,
                part: pending.part.clone(),
                reason:
                    t("La aplicación no respondió. Revisa la conversación antes de volver a intentar.")
                        .into(),
            });
        }
        self.interactions.problem.clone()
    }

    /// When the interaction in flight stops being waited for.
    pub fn interaction_expires(&self, now: Instant) -> Option<Duration> {
        self.interaction_pending(now)
            .map(|pending| (pending.since + pending.wait).saturating_duration_since(now))
    }

    /// The modal an app opened, and where.
    pub fn modal(&self) -> Option<(ChannelId, &Modal)> {
        self.interactions
            .modal
            .as_ref()
            .map(|(channel, modal)| (*channel, modal))
    }

    /// Close the modal without sending it.
    pub fn close_modal(&mut self) {
        self.interactions.modal = None;
        self.revision += 1;
    }

    /// What the app suggested for the request `request`, once it did.
    pub fn suggested(&self, request: RequestId) -> Option<&[OptionChoice]> {
        self.interactions
            .choices
            .as_ref()
            .filter(|(asked, _)| *asked == request)
            .map(|(_, choices)| choices.as_slice())
    }

    /// Take a reply only the account sees out of `channel`.
    pub fn dismiss_ephemeral(&mut self, channel: ChannelId, message: MessageId) -> bool {
        let ephemeral = self
            .timeline(channel)
            .and_then(|timeline| timeline.get(message))
            .is_some_and(|found| found.components.ephemeral);
        if !ephemeral {
            return false;
        }
        if let Some(timeline) = self.timelines.get_mut(&channel) {
            timeline.remove(message);
        }
        self.interactions
            .ephemeral
            .retain(|kept| *kept != (channel, message));
        self.revision += 1;
        true
    }

    /// A reply only the account sees: shown in its conversation, without
    /// counting as unread, and at most [`EPHEMERAL_KEPT`] at once.
    pub(crate) fn ephemeral_created(&mut self, message: Message) -> Applied {
        let channel = message.channel;
        let key = (channel, message.id);
        let kept = &mut self.interactions.ephemeral;
        kept.retain(|old| *old != key);
        kept.push(key);
        let dropped = (kept.len() > EPHEMERAL_KEPT).then(|| kept.remove(0));
        if let Some((old_channel, old)) = dropped {
            if let Some(timeline) = self.timelines.get_mut(&old_channel) {
                timeline.remove(old);
            }
        }
        let timeline = self.timelines.entry(channel).or_default();
        if !timeline.latest {
            return Applied::Messages { channel, added: 0 };
        }
        let added = timeline.upsert(message) as usize;
        Applied::Messages { channel, added }
    }

    /// The commands asked for with `request` arrived.
    pub(crate) fn commands_loaded(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        commands: Vec<AppCommand>,
        now: Instant,
    ) -> Applied {
        let scope = self.command_scope(channel);
        let catalog = &mut self.interactions.catalog;
        if catalog.request != request || !catalog.loading || catalog.scope != scope {
            return Applied::Nothing;
        }
        catalog.loading = false;
        catalog.loaded = Some(now);
        catalog.error = None;
        catalog.commands = commands;
        Applied::Channel(channel)
    }

    /// The app got, or answered, the interaction `request`.
    pub(crate) fn interaction_answered(&mut self, request: RequestId, done: bool) -> Applied {
        let Some(pending) = self
            .interactions
            .pending
            .as_mut()
            .filter(|pending| pending.request == request)
        else {
            return Applied::Nothing;
        };
        let channel = pending.channel;
        if done {
            // A modal sent is closed once its app answers it.
            if pending.modal {
                self.interactions.modal = None;
            }
            self.interactions.pending = None;
        } else {
            pending.received = true;
        }
        Applied::Channel(channel)
    }

    /// The app opened `modal` in answer to `request`.
    pub(crate) fn modal_opened(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        modal: Modal,
    ) -> Applied {
        if self
            .interactions
            .pending
            .as_ref()
            .is_none_or(|pending| pending.request != request || pending.channel != channel)
        {
            return Applied::Nothing;
        }
        self.interactions.pending = None;
        self.interactions.problem = None;
        self.interactions.modal = Some((channel, modal));
        Applied::Channel(channel)
    }

    pub(crate) fn choices_suggested(
        &mut self,
        request: RequestId,
        mut choices: Vec<OptionChoice>,
    ) -> Applied {
        choices.truncate(rune_model::interactions::MAX_CHOICES);
        self.interactions.choices = Some((request, choices));
        Applied::Nothing
    }

    /// Whether a failure is of P3, and what it changes here; false when it
    /// is not one of this session's.
    pub(crate) fn interaction_failed(&mut self, failure: &Failure, reason: &str) -> bool {
        match failure {
            Failure::AppCommands { request, .. } => {
                let catalog = &mut self.interactions.catalog;
                if catalog.request != *request || !catalog.loading {
                    return false;
                }
                catalog.loading = false;
                catalog.error = Some(reason.to_owned());
                true
            }
            Failure::Interaction { request, channel } => {
                let Some(pending) = self
                    .interactions
                    .pending
                    .take_if(|pending| pending.request == *request)
                else {
                    return false;
                };
                self.interactions.problem = Some(Problem {
                    channel: *channel,
                    part: pending.part,
                    reason: reason.to_owned(),
                });
                true
            }
            _ => false,
        }
    }

    /// The commands of `server` changed at the service: a list of them, or
    /// of any conversation (`None`), is read again when next needed.
    pub(crate) fn commands_changed(&mut self, server: Option<ServerId>) -> Applied {
        let catalog = &mut self.interactions.catalog;
        let stale = match (catalog.scope, server) {
            (Some(Scope::Server(kept)), Some(server)) => kept == server,
            (Some(_), None) => true,
            _ => false,
        };
        if stale && !catalog.loading {
            catalog.loaded = None;
        }
        Applied::Nothing
    }

    /// The interaction in flight waits for the person's verification: its
    /// app is waited for once that may be done.
    pub(crate) fn interaction_challenged(&mut self, failure: &Failure, now: Instant) {
        if let (Failure::Interaction { request, .. }, Some(pending)) =
            (failure, self.interactions.pending.as_mut())
        {
            if pending.request == *request {
                pending.since = now + rune_provider::CHALLENGE_LIFETIME;
            }
        }
    }

    /// A new session: nothing of the last one's apps is waited for.
    pub(crate) fn interactions_ready(&mut self) {
        let ephemeral = std::mem::take(&mut self.interactions.ephemeral);
        self.interactions = InteractionState {
            ephemeral,
            ..InteractionState::default()
        };
    }
}
