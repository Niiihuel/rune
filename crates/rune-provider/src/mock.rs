//! A simulated service: servers, channels, people and years of messages made
//! up from a seed, so the same seed always gives the same world. Nothing
//! leaves the machine. It answers like a service would, after a latency:
//! history in pages, sent messages confirmed, replies from the people of the
//! channel, people typing, coming and going, and talking in voice channels,
//! all simulated. [`MockControl`] drops the connection on purpose, to see the
//! client reconnect.

mod admin;
mod challenges;
mod manage;
mod profiles;
mod read_state;
mod search;
mod showcase;
mod social;
mod stage;
mod threads;
mod uploads;
// The account's synced settings (P1).
mod account;
// Commands and interactions (P3).
mod interactions;
// Message actions and moderation (P4).
mod message_actions;
// Desktop, language and local voice (P6).
mod desktop;
// Presence, identity and expressions (P5).
mod identity;
// Server administration, rest (P7).
mod server_admin;
// Private calls.
mod calls;
// Invitations of every kind.
mod invitations;
// A refused acceptance, for captures (invitaciones-motivo).
pub use invitations::refused_invitation;
// Rail and notifications.
mod rail;
// Voice protocol (voz-protocolo).
mod voice_protocol;
// Rules and onboarding (2026-10-18).
mod onboarding;
// Scheduled events (eventos-programados).
mod events;
// AutoMod (automod).
mod automod;

use read_state::ReadCursor;
use uploads::SentMessage;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rune_i18n::{es, t, tr};
use rune_model::{
    Attachment, AttachmentId, CallQuality, Channel, ChannelId, ChannelKind, Delivery, Member,
    Message, MessageId, MessageKind, Overwrite, OverwriteTarget, Permissions, Poll, PollAnswer,
    Presence, Reaction, Rgb, Role, RoleId, Server, ServerId, Status, StreamPhase, Timestamp, User,
    UserId, VoiceState,
};
use tokio::sync::{mpsc, Notify};
use tokio::task::AbortHandle;

use crate::{
    BoxFuture, Capabilities, Command, Event, EventSink, Failure, Latest, Provider, ProviderError,
    ProviderInfo, ProviderKind, RequestId, Snapshot, StorageIdentity, VoiceSupport,
};

/// How the simulation behaves.
#[derive(Clone)]
pub struct MockConfig {
    /// The world: the same seed makes the same people, channels and history.
    pub seed: u64,
    /// When the made-up history ends.
    pub history_end: Timestamp,
    /// The time of a message sent now.
    pub clock: fn() -> Timestamp,
    /// How long an answer takes, as over a network.
    pub latency: Duration,
    /// People write, type and change status on their own, every few seconds.
    pub ambient: bool,
    /// Someone answers a message sent here, after typing for a moment.
    pub replies: bool,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            seed: 0x52_55_4E_45,
            history_end: wall_clock(),
            clock: wall_clock,
            latency: Duration::from_millis(140),
            ambient: true,
            replies: true,
        }
    }
}

/// The time on this machine's clock.
pub fn wall_clock() -> Timestamp {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Timestamp(elapsed.as_millis() as i64)
}

/// Lets the host disturb the simulation.
#[derive(Default)]
pub struct MockControl {
    dropped: Notify,
    fail_next_send: AtomicBool,
    ignore_next_send: AtomicBool,
    // Private calls.
    ring_me: Notify,
    answer_elsewhere: Notify,
}

impl MockControl {
    /// End the current session as a lost connection would.
    pub fn drop_connection(&self) {
        self.dropped.notify_one();
    }

    /// Refuse the next message sent, as a service would reject it.
    pub fn fail_next_send(&self) {
        self.fail_next_send.store(true, Ordering::Relaxed);
    }

    /// Take the next message sent and never answer: its fate stays unknown.
    pub fn ignore_next_send(&self) {
        self.ignore_next_send.store(true, Ordering::Relaxed);
    }

    // Private calls.
    /// Have the person of the first direct conversation call the account.
    pub fn ring_me(&self) {
        self.ring_me.notify_one();
    }

    /// Answer the call ringing the account on «another of its devices».
    pub fn answer_elsewhere(&self) {
        self.answer_elsewhere.notify_one();
    }
}

/// The simulated service.
pub struct MockProvider {
    config: MockConfig,
    world: Arc<Mutex<World>>,
    control: Arc<MockControl>,
    // Profile editing (edicion-de-perfil).
    /// The world's saved pictures, read without its lock.
    hosted: Hosted,
}

impl MockProvider {
    pub fn new(config: MockConfig) -> Self {
        let world = World::build(&config);
        Self {
            config,
            hosted: world.hosted.clone(),
            world: Arc::new(Mutex::new(world)),
            control: Arc::default(),
        }
    }

    /// The handle that disturbs this simulation.
    pub fn control(&self) -> Arc<MockControl> {
        self.control.clone()
    }

    /// The account the simulation logs in as.
    pub fn me(&self) -> UserId {
        ME
    }

    /// What a session starts with, at once: for tests and measurements that
    /// drive the interface without a runtime.
    pub fn snapshot(&self) -> Snapshot {
        self.world.lock().unwrap().snapshot()
    }

    /// A page of history at once, as `LoadHistory` would answer it.
    pub fn history(
        &self,
        channel: ChannelId,
        before: Option<MessageId>,
        limit: usize,
    ) -> (Vec<Message>, bool) {
        self.world.lock().unwrap().history(channel, before, limit)
    }

    /// What a session reports for a management command (servers, channels,
    /// roles, members, bans, audit log, profile, status), at once: for tests that
    /// drive the interface without a runtime.
    pub fn manage(&self, command: Command) -> Vec<Event> {
        let now = (self.config.clock)();
        self.world.lock().unwrap().manage(command, now)
    }

    // Message actions and moderation (P4).
    /// What a session reports for a message action or a voice moderation,
    /// at once, as [`MockProvider::manage`] does for management.
    pub fn message_action(&self, command: Command) -> Vec<Event> {
        let now = (self.config.clock)();
        self.world.lock().unwrap().message_action(command, now)
    }
}

/// The account of the person using RUNE in the simulation.
pub const ME: UserId = UserId(1);
const MESSAGES_PER_CHANNEL: u64 = 10_000_000;
/// The messages of each voice room's chat.
const VOICE_CHAT: u64 = 8;
/// The most messages a channel keeps pinned, as services cap them.
const PINS_MAX: usize = 50;
/// Messages of one sitting: the same few people, minutes apart.
const BURST: u64 = 6;

impl Provider for MockProvider {
    fn upload_limits(&self) -> Option<crate::UploadLimits> {
        Some(uploads::LIMITS)
    }

    fn legacy_storage_namespace(&self) -> Option<&'static str> {
        Some("Simulación local")
    }

    fn storage_identity(&self) -> Option<StorageIdentity> {
        Some(StorageIdentity {
            provider: "rune.mock.v1",
            account: ME,
        })
    }

    // Invitations of every kind.
    fn invitation_target(&self, url: &str) -> Option<crate::InvitationTarget> {
        invitations::target(url)
    }

    fn inspect_invitation(
        &self,
        target: crate::InvitationTarget,
    ) -> BoxFuture<'_, Result<crate::InvitationPreview, crate::InvitationError>> {
        let preview = self.world.lock().unwrap().invitation(&target);
        Box::pin(async move { preview })
    }

    // Profile editing (edicion-de-perfil).
    fn hosted_image(&self, url: &str) -> Option<bytes::Bytes> {
        self.hosted.read().ok()?.get(url).cloned()
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: es("Simulación local").into(),
            kind: ProviderKind::Simulated,
            capabilities: Capabilities {
                // Its own links of every kind; see `mock::invitations`.
                invitation_preview: true,
                invitation_accept: true,
                // Of its servers and packs, as the simulated plan allows; see `mock::identity`.
                stickers: true,
                authentication: false,
                saved_accounts: false,
                account_servers: true,
                send_messages: true,
                edit_messages: true,
                delete_messages: true,
                reactions: true,
                poll_votes: true,
                history: true,
                message_context: true,
                read_state: true,
                typing: true,
                direct_messages: true,
                member_list: true,
                presence: true,
                voice: VoiceSupport::Simulated,
                // The camera «goes» to the simulated call, and says it is simulated.
                video: true,
                // The screen «goes live» in the simulation, and says it is simulated.
                share_screen: true,
                // A simulated stream: the host draws made-up frames for it.
                watch_streams: true,
                // Simulated cameras too, drawn the same way.
                watch_cameras: true,
                // Its voice rooms have a few made-up messages of their own.
                voice_text: true,
                attachments: true,
                // A nonce sent again returns the message it made: see `World::post_once`.
                idempotent_send: true,
                manage_servers: true,
                manage_channels: true,
                moderation: true,
                edit_profile: true,
                custom_status: true,
                audit_log: true,
                manage_invites: true,
                manage_expressions: true,
                pins: true,
                relationships: true,
                friend_requests: true,
                block_users: true,
                open_direct: true,
                manage_groups: true,
                thread_replies: true,
                profiles: true,
                // Its streams answer with no picture; the column draws a
                // made-up one and says it is simulated.
                stream_previews: true,
                // Streams (transmisiones): taken and dropped; see `UploadStreamPreview`.
                upload_stream_previews: true,
                create_groups: true,
                group_members: true,
                close_direct: true,
                friend_nicknames: true,
                friend_presence: true,
                // Threads and posts are made, followed and changed here, and
                // archived ones come a page at a time; see `mock::threads`.
                create_threads: true,
                thread_pages: true,
                follow_threads: true,
                manage_threads: true,
                // A simulated channel has no address anyone else could open.
                channel_links: None,
                // Its stage seats the account in the audience; see `mock::stage`.
                stages: true,
                // Pictures saved stay in memory and are read back through
                // `Provider::hosted_image`: see `mock::profiles`.
                profile_images: true,
                server_profiles: true,
                group_owner: true,
                // Administration: made-up webhooks and integrations, codes
                // without a link anyone could open, pictures named and not
                // hosted, favorites kept on this device; see `mock::admin`.
                timeouts: true,
                create_invites: true,
                invite_links: None,
                upload_expressions: true,
                member_pages: true,
                prune: true,
                webhooks: true,
                integrations: true,
                synced_gif_favorites: false,
                // Kept with the simulated account; see `mock::account`.
                synced_notifications: true,
                server_folders: true,
                messaging_privacy: true,
                notes: true,
                // Reads, search and remote members (P2): the simulation keeps
                // the cursors and reminders, searches every made-up message
                // and looks up a crowd of people; see `mock::search`.
                server_read: true,
                remote_unread: true,
                message_search: true,
                member_search: true,
                // RuneBot's commands, parts and modal; see `mock::interactions`.
                interactions: true,
                app_commands: true,
                // Message actions and moderation (P4): forwards, polls,
                // who reacted and voted (made up from the counts), links to
                // its own conversations (on the reserved `.invalid` domain,
                // so they open only in RUNE), silent messages, replies
                // without a mention, voice moderation and leaving; see
                // `mock::message_actions`.
                forward_messages: true,
                create_polls: true,
                reaction_lists: true,
                poll_voters: true,
                load_channels: true,
                silent_messages: true,
                reply_mentions: true,
                voice_moderation: true,
                leave_servers: true,
                trusted_links: &[],
                // Desktop, language and local voice (P6): the local activity
                // shows in the account's presence, a few made-up games, and
                // made-up call codes the interface calls simulated; see
                // `mock::desktop`.
                local_activity: true,
                detectable_games: true,
                voice_privacy_code: true,
                // Made-up packs and GIFs, named as simulated; see `mock::identity`.
                sticker_packs: true,
                gif_search: true,
                // Server administration, rest (P7): pictures named and not
                // hosted, members who joined when and how the simulation
                // says; see `mock::server_admin`.
                server_identity: true,
                server_safety: true,
                pause_invites: true,
                reorder_channels: true,
                forum_settings: true,
                role_styles: true,
                role_counts: true,
                member_queries: true,
                // Private calls: whom the account rings answers, declines
                // or lets it ring by who they are, and «Pruebas de la
                // simulación» has someone call it; see `mock::calls`.
                private_calls: true,
                // Invitations of every kind: see `mock::invitations`.
                group_invitations: true,
                friend_invitations: true,
                guest_invitations: true,
                // A simulated loss waits in the core, without a session to
                // hurry: «Reintentar» starts it again instead.
                reconnect_now: false,
                // A direct call rings a friend, starts silent with a
                // request on either side and is refused otherwise; see
                // `mock::calls`.
                call_check: true,
                // Rules and onboarding: the club of the invitation «club»
                // asks both, and the owned server's can be edited; see
                // `mock::onboarding`.
                server_rules: true,
                onboarding: true,
                manage_onboarding: true,
                // Scheduled events: see `mock::events`.
                scheduled_events: true,
                manage_scheduled_events: true,
                // A followed thread is silenced here; see `mock::threads`.
                mute_threads: true,
                // The simulated account shows its activity until told not
                // to, and says whether the local one is seen; see
                // `mock::account` and `mock::desktop`.
                activity_sharing: true,
                // A banner named and not hosted, and a server made a
                // community when it has what one needs; see
                // `mock::server_admin`.
                server_banner: true,
                community: true,
                // AutoMod: the owned server's rules are edited, and one of
                // «Taller de Rust» blocks the account; see `mock::automod`.
                automod: true,
            },
            notice: es("Datos simulados en este equipo: no hay conexión con Discord ni con ningún servicio.").into(),
        }
    }

    fn connect<'a>(
        &'a self,
        commands: &'a mut mpsc::Receiver<Command>,
        events: &'a EventSink,
    ) -> BoxFuture<'a, Result<(), ProviderError>> {
        Box::pin(Session::new(self, events.clone()).run(commands))
    }
}

/// One connection of the simulation.
struct Session {
    config: MockConfig,
    world: Arc<Mutex<World>>,
    control: Arc<MockControl>,
    events: EventSink,
    requests: HashMap<RequestId, AbortHandle>,
    tasks: Vec<AbortHandle>,
    /// The command waiting on a simulated verification, and the last one asked.
    challenge: Option<(RequestId, Command)>,
    challenges: RequestId,
}

impl Drop for Session {
    fn drop(&mut self) {
        for task in self.requests.values().chain(&self.tasks) {
            task.abort();
        }
    }
}

impl Session {
    fn new(provider: &MockProvider, events: EventSink) -> Self {
        Self {
            config: provider.config.clone(),
            world: provider.world.clone(),
            control: provider.control.clone(),
            events,
            requests: HashMap::new(),
            tasks: Vec::new(),
            challenge: None,
            challenges: 0,
        }
    }

    async fn run(mut self, commands: &mut mpsc::Receiver<Command>) -> Result<(), ProviderError> {
        let closed = |_| ProviderError::Fatal("la interfaz se cerró".into());
        tokio::time::sleep(self.config.latency).await;
        let snapshot = self.world.lock().unwrap().snapshot();
        self.events
            .send(Event::Ready(Box::new(snapshot)))
            .await
            .map_err(closed)?;
        // What the account keeps with the service comes right after.
        let account = self.world.lock().unwrap().account_events();
        for event in account {
            self.events.send(event).await.map_err(closed)?;
        }
        self.events
            .send(Event::Perks(identity::perks()))
            .await
            .map_err(closed)?;
        // Rail and notifications: an app in voice.
        let activities = self.world.lock().unwrap().voice_activities();
        for event in activities {
            self.events.send(event).await.map_err(closed)?;
        }
        // Voice protocol (voz-protocolo): since when its voice rooms are on.
        let now = (self.config.clock)();
        let started = self.world.lock().unwrap().voice_start_times(now);
        for event in started {
            self.events.send(event).await.map_err(closed)?;
        }
        // A call that was on when the connection dropped connects again.
        let rejoined = self.world.lock().unwrap().my_call();
        if let Some(channel) = rejoined {
            let (quality, phase) = {
                let mut world = self.world.lock().unwrap();
                let phase = world.call_phase(channel);
                world.call_shown(channel, &phase);
                (Some(world.quality()), phase)
            };
            self.events
                .send(Event::CallPhase {
                    channel,
                    phase,
                    quality,
                })
                .await
                .map_err(closed)?;
            self.events
                .send(Event::CallPrivacyCode {
                    channel,
                    code: Some(desktop::privacy_code(channel)),
                })
                .await
                .map_err(closed)?;
        }
        let mut ambient = tokio::time::interval(Duration::from_millis(500));
        ambient.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut next_ambient = self.world.lock().unwrap().rng.range(5_000, 12_000) as u64 / 500;
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(command) => self.handle(command),
                    None => break Ok(()),
                },
                () = self.control.dropped.notified() => {
                    break Err(ProviderError::Transient("conexión simulada perdida".into()));
                }
                // Private calls.
                () = self.control.ring_me.notified() => {
                    self.call_later(Duration::ZERO, |world| world.ring_me());
                }
                () = self.control.answer_elsewhere.notified() => {
                    self.later(Duration::ZERO, |world| world.answer_elsewhere());
                }
                _ = ambient.tick() => {
                    self.speak();
                    if self.config.ambient {
                        next_ambient = next_ambient.saturating_sub(1);
                        if next_ambient == 0 {
                            next_ambient = self.world.lock().unwrap().rng.range(5_000, 12_000) as u64 / 500;
                            self.ambient();
                            self.tally();
                        }
                    }
                }
            }
        }
    }

    /// Run `work` after `delay`, reporting through this session's sink, and
    /// stop it when the session ends.
    fn later(
        &mut self,
        delay: Duration,
        work: impl FnOnce(&mut World) -> Vec<Event> + Send + 'static,
    ) {
        let (world, events) = (self.world.clone(), self.events.clone());
        let clock = self.config.clock;
        let handle = tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let produced = {
                let mut world = world.lock().unwrap();
                let mut produced = work(&mut world);
                // Voice protocol (voz-protocolo): someone came or left.
                produced.extend(world.follow_call());
                produced.extend(world.follow_rooms(clock()));
                produced
            };
            for event in produced {
                if events.send(event).await.is_err() {
                    return;
                }
            }
        });
        self.tasks.retain(|task| !task.is_finished());
        self.tasks.push(handle.abort_handle());
    }

    /// Run `work` on a call after `delay`, as [`Session::later`] does, then
    /// each step it leaves when it comes, as the simulated people would.
    fn call_later(
        &mut self,
        delay: Duration,
        work: impl FnOnce(&mut World) -> (Vec<Event>, Vec<calls::Later>) + Send + 'static,
    ) {
        let (world, events) = (self.world.clone(), self.events.clone());
        let clock = self.config.clock;
        let handle = tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let (produced, mut steps) = work(&mut world.lock().unwrap());
            for event in produced {
                if events.send(event).await.is_err() {
                    return;
                }
            }
            steps.sort_by_key(|step| step.after);
            let start = tokio::time::Instant::now();
            for step in steps {
                tokio::time::sleep_until(start + step.after).await;
                let produced = {
                    let mut world = world.lock().unwrap();
                    let mut produced = world.call_step(&step);
                    // Voice protocol (voz-protocolo): someone came or left.
                    produced.extend(world.follow_call());
                    produced.extend(world.follow_rooms(clock()));
                    produced
                };
                for event in produced {
                    if events.send(event).await.is_err() {
                        return;
                    }
                }
            }
        });
        self.tasks.retain(|task| !task.is_finished());
        self.tasks.push(handle.abort_handle());
    }

    fn handle(&mut self, command: Command) {
        let Some(command) = self.verify(command) else {
            return;
        };
        let latency = self.config.latency;
        let now = (self.config.clock)();
        match command {
            Command::BeginSignIn | Command::CancelSignIn | Command::SignOut => {
                self.events.send_lossy(Event::Failed {
                    failure: Failure::Other,
                    reason: t("La simulación local no usa autorización de cuentas.").into(),
                    ambiguous: false,
                });
            }
            Command::LoadHistory {
                request,
                channel,
                before,
                limit,
            } => {
                let (world, events) = (self.world.clone(), self.events.clone());
                let handle = tokio::spawn(async move {
                    tokio::time::sleep(latency).await;
                    let (messages, reached_start) =
                        world.lock().unwrap().history(channel, before, limit);
                    let _ = events
                        .send(Event::HistoryLoaded {
                            request,
                            channel,
                            before,
                            messages,
                            reached_start,
                        })
                        .await;
                });
                self.requests.retain(|_, task| !task.is_finished());
                self.requests.insert(request, handle.abort_handle());
            }
            Command::LoadContext {
                request,
                channel,
                anchor,
                limit,
            } => {
                let (world, events) = (self.world.clone(), self.events.clone());
                let handle = tokio::spawn(async move {
                    tokio::time::sleep(latency).await;
                    let messages = world.lock().unwrap().context(channel, anchor, limit);
                    let event = match messages {
                        Some(messages) => Event::ContextLoaded {
                            request,
                            channel,
                            anchor,
                            messages,
                        },
                        None => Event::Failed {
                            failure: Failure::Context { request, channel },
                            reason: t("No encontramos ese mensaje en este contexto.").into(),
                            ambiguous: false,
                        },
                    };
                    let _ = events.send(event).await;
                });
                self.requests.retain(|_, task| !task.is_finished());
                if let Some(previous) = self.requests.insert(request, handle.abort_handle()) {
                    previous.abort();
                }
            }
            Command::Cancel(request) => {
                if let Some(task) = self.requests.remove(&request) {
                    task.abort();
                }
            }
            Command::SendMessage {
                channel,
                content,
                reply_to,
                nonce,
                ..
            } => {
                // A silent message notifies nobody; here nobody is notified anyway.
                let content = crate::without_silent(&content)
                    .map(str::to_owned)
                    .unwrap_or(content);
                if self.control.ignore_next_send.swap(false, Ordering::Relaxed) {
                    return;
                }
                let failure = Failure::Send { channel, nonce };
                if self.control.fail_next_send.swap(false, Ordering::Relaxed) {
                    self.later(latency, move |_| {
                        vec![Event::Failed {
                            failure,
                            reason: t("rechazado (fallo simulado)").into(),
                            ambiguous: false,
                        }]
                    });
                    return;
                }
                let replies = self.config.replies;
                let fresh = !self.world.lock().unwrap().sent.contains_key(&nonce);
                self.later(latency, move |world| {
                    if world.restricted_in(channel, now).is_some() {
                        return vec![Event::Failed {
                            failure,
                            reason: t("estás aislado en este servidor (simulado)").into(),
                            ambiguous: false,
                        }];
                    }
                    // AutoMod (automod): a rule of the server may block it.
                    if let Some(notice) = world.automod_blocks(channel, ME, &content) {
                        return vec![Event::AutoModBlocked { failure, notice }];
                    }
                    let reopened = match world.thread_reply(channel) {
                        Ok(reopened) => reopened,
                        Err(reason) => {
                            return vec![Event::Failed {
                                failure,
                                reason,
                                ambiguous: false,
                            }]
                        }
                    };
                    match world.post_once(nonce, channel, content, reply_to, now) {
                        Some(message) => reopened
                            .into_iter()
                            .chain([Event::MessageCreated {
                                message,
                                nonce: Some(nonce),
                            }])
                            .collect(),
                        None => vec![Event::Failed {
                            failure,
                            reason: t("el canal no existe").into(),
                            ambiguous: false,
                        }],
                    }
                });
                if replies && fresh {
                    self.reply(channel, now);
                }
            }
            Command::SendFiles {
                scope,
                content,
                reply_to,
                files,
                fence,
                mention_reply,
            } => {
                self.send_files(scope, content, reply_to, files, fence, mention_reply);
            }
            Command::EditMessage {
                channel,
                message,
                content,
            } => self.later(latency, move |world| {
                match world.edit(channel, message, |message| {
                    message.content = content;
                    message.edited = Some(now);
                }) {
                    Ok(message) => vec![Event::MessageUpdated(message)],
                    Err(reason) => vec![Event::Failed {
                        failure: Failure::Edit { channel, message },
                        reason,
                        ambiguous: false,
                    }],
                }
            }),
            Command::DeleteMessage { channel, message } => {
                self.later(latency, move |world| match world.delete(channel, message) {
                    Ok(()) => vec![Event::MessageDeleted { channel, message }],
                    Err(reason) => vec![Event::Failed {
                        failure: Failure::Delete { channel, message },
                        reason,
                        ambiguous: false,
                    }],
                })
            }
            Command::ToggleReaction {
                channel,
                message,
                emoji,
                add,
            } => self.later(latency / 2, move |world| {
                match world.react(channel, message, &emoji, add) {
                    Ok(message) => vec![Event::MessageUpdated(message)],
                    Err(reason) => vec![Event::Failed {
                        failure: Failure::React {
                            channel,
                            message,
                            emoji,
                        },
                        reason,
                        ambiguous: false,
                    }],
                }
            }),
            Command::VotePoll {
                channel,
                message,
                answers,
            } => self.later(latency / 2, move |world| {
                match world.vote(channel, message, &answers, now) {
                    Ok(message) => vec![Event::MessageUpdated(message)],
                    Err(reason) => vec![Event::Failed {
                        failure: Failure::Vote { channel, message },
                        reason,
                        ambiguous: false,
                    }],
                }
            }),
            Command::StartTyping { .. } => {}
            Command::MarkRead {
                request,
                account,
                channel,
                message,
            } => {
                let (world, events) = (self.world.clone(), self.events.clone());
                let handle = tokio::spawn(async move {
                    tokio::time::sleep(latency).await;
                    let event = world
                        .lock()
                        .unwrap()
                        .confirm_read(request, account, channel, message);
                    let _ = events.send(event).await;
                });
                self.requests.retain(|_, task| !task.is_finished());
                if let Some(previous) = self.requests.insert(request, handle.abort_handle()) {
                    previous.abort();
                }
            }
            Command::JoinVoice {
                channel,
                muted,
                deafened,
            } => self.later(latency, move |world| {
                let mut events = world.join_voice(channel, muted, deafened);
                // Private calls: a conversation's call starts, or is answered.
                events.extend(world.call_joined(channel));
                events
            }),
            // Private calls: what each person does comes later.
            // After the call it rings in, as a stream's connection is.
            Command::RingCall { channel, users } => {
                self.call_later(latency * 2, move |world| world.ring(channel, users))
            }
            Command::StopRinging { channel, users } => {
                self.call_later(latency, move |world| world.stop_ringing(channel, users))
            }
            // Calls and the voice panel (llamadas-y-panel-de-voz).
            Command::CheckCall { channel } => {
                self.later(latency, move |world| vec![world.check_call(channel)])
            }
            Command::LeaveVoice => self.later(latency, |world| world.leave_voice()),
            // A raised hand is answered by the made-up moderation a moment later.
            command @ Command::RaiseHand { raised: true, .. } => {
                let Command::RaiseHand { channel, .. } = command else {
                    unreachable!()
                };
                self.later(latency, move |world| world.manage(command, now));
                self.later(latency + Duration::from_millis(2_500), move |world| {
                    world.simulated_invitation(channel, Timestamp(now.0 + 2_500))
                });
            }
            // Its own connection, after the call's.
            Command::WatchStream { channel, owner } => {
                self.later(latency * 2, move |world| world.watch_stream(channel, owner))
            }
            // Nothing to release: the frames were never real.
            Command::StopWatching { .. } => {}
            // Its own connection too; nothing leaves this machine.
            Command::StartStream { channel, .. } => {
                self.later(latency * 2, move |world| world.share(channel, true))
            }
            Command::StopStream { channel } => {
                self.later(Duration::ZERO, move |world| world.share(channel, false))
            }
            Command::PauseStream { channel, paused } => self.later(Duration::ZERO, move |world| {
                world.pause_share(channel, paused)
            }),
            // No picture leaves a simulated stream: only that it streams.
            Command::LoadStreamPreview { channel, owner, .. } => {
                self.later(latency, move |world| {
                    let streaming = world.voice.iter().any(|state| {
                        state.user == owner && state.channel == channel && state.streaming
                    });
                    streaming
                        .then_some(Event::StreamPreview {
                            channel,
                            owner,
                            image: None,
                            retry_after: None,
                        })
                        .into_iter()
                        .collect()
                })
            }
            // Only the voice state changes: nothing leaves this machine. As
            // late as a join, so a camera turned on while joining finds the call.
            Command::SetCamera { channel, on } => {
                self.later(latency, move |world| world.camera(channel, on))
            }
            Command::SetVoiceSelf { muted, deafened } => self.later(Duration::ZERO, move |world| {
                world
                    .voice_self(muted, deafened)
                    .into_iter()
                    .map(Event::VoiceChanged)
                    .collect()
            }),
            command @ Command::SetStatus(_) => {
                self.later(Duration::ZERO, move |world| world.manage(command, now))
            }
            // The simulation groups its members itself, from what it knows.
            Command::WatchMemberList { .. }
            | Command::UnwatchMemberList
            | Command::WatchPeople { .. } => {}
            // Voice protocol (voz-protocolo): its rooms' start times go with
            // each session, and change as people come and go.
            Command::WatchServer(_) => {}
            // Streams (transmisiones): nobody else sees a simulated stream,
            // so its picture goes nowhere; that it was sent is enough.
            Command::UploadStreamPreview { .. } => {}
            // Reads, search and remote members (P2).
            Command::MarkUnread {
                account,
                channel,
                message,
                mentions,
            } => self.later(latency / 2, move |world| {
                vec![world.mark_unread(account, channel, message, mentions)]
            }),
            Command::MarkServerRead { account, server } => self.later(latency / 2, move |world| {
                world.mark_server_read(account, server)
            }),
            Command::SearchMessages {
                request,
                search,
                offset,
            } => {
                let (world, events) = (self.world.clone(), self.events.clone());
                let handle = tokio::spawn(async move {
                    tokio::time::sleep(latency).await;
                    let found = world
                        .lock()
                        .unwrap()
                        .search_messages(request, &search, offset);
                    for event in found {
                        if events.send(event).await.is_err() {
                            return;
                        }
                    }
                });
                self.requests.retain(|_, task| !task.is_finished());
                if let Some(previous) = self.requests.insert(request, handle.abort_handle()) {
                    previous.abort();
                }
            }
            Command::SearchMembers { server, query } => self.later(latency / 2, move |world| {
                world.search_members(server, &query)
            }),
            // Message actions and moderation (P4): see `message_actions`.
            command @ (Command::ForwardMessage { .. }
            | Command::CreatePoll { .. }
            | Command::EndPoll { .. }
            | Command::LoadReactions { .. }
            | Command::LoadPollVoters { .. }
            | Command::LoadChannel { .. }
            | Command::ModerateVoice { .. }) => {
                self.later(latency, move |world| world.message_action(command, now))
            }
            // Servers, channels, roles, members, the profile and the custom status: see `manage`.
            command @ (Command::LoadAppCommands { .. }
            | Command::Interact { .. }
            | Command::SuggestChoices { .. }) => self.interact(command),
            // Desktop, language and local voice (P6): see `desktop`.
            Command::SetLocalActivity(activity) => {
                self.later(Duration::ZERO, move |world| world.local_activity(activity))
            }
            Command::LoadDetectableGames => {
                self.later(latency, |_| vec![Event::DetectableGames(desktop::games())])
            }
            Command::VoiceVerificationCode { channel, user } => {
                self.later(
                    latency,
                    move |world| vec![world.verification(channel, user)],
                )
            }
            // Presence, identity and expressions (P5).
            command @ (Command::LoadStickerPacks
            | Command::SearchGifs { .. }
            | Command::SuggestGifs { .. }
            | Command::SendSticker { .. }) => self.identity(command),
            // A running simulation is connected: nothing waits to hurry
            // (the core waits after a simulated loss, without a session).
            Command::ReconnectNow => {}
            command => self.later(latency, move |world| world.manage(command, now)),
        }
    }

    /// Someone of the channel answers what was just sent: they type, then write.
    fn reply(&mut self, channel: ChannelId, now: Timestamp) {
        let (responder, wait) = {
            let mut world = self.world.lock().unwrap();
            let others: Vec<UserId> = world
                .participants(channel)
                .into_iter()
                .filter(|user| *user != ME)
                .collect();
            if others.is_empty() || (!world.is_direct(channel) && world.rng.chance(0.35)) {
                return;
            }
            let responder = others[world.rng.below(others.len() as u64) as usize];
            (responder, world.rng.range(900, 1_800) as u64)
        };
        self.later(Duration::from_millis(wait), move |_| {
            vec![Event::Typing {
                channel,
                user: responder,
            }]
        });
        self.later(Duration::from_millis(wait + 1_600), move |world| {
            let line = REPLIES[world.rng.below(REPLIES.len() as u64) as usize].to_owned();
            let at = now.plus_minutes(0);
            // AutoMod (automod): a rule of the server may hold it, and
            // tell the account what it did.
            let (posted, told) = world
                .automod_acts(channel, responder, &line)
                .unwrap_or((true, Vec::new()));
            if !posted {
                return told;
            }
            world
                .post(
                    channel,
                    responder,
                    line,
                    None,
                    Timestamp(at.0 + wait as i64 + 1_600),
                )
                .map(|message| {
                    std::iter::once(Event::MessageCreated {
                        message,
                        nonce: None,
                    })
                    .chain(told)
                    .collect()
                })
                .unwrap_or_default()
        });
    }

    /// Life elsewhere: someone types and writes in a channel, or changes status.
    fn ambient(&mut self) {
        let now = (self.config.clock)();
        let mut world = self.world.lock().unwrap();
        // Someone whose conversation was closed writes again: it comes back
        // first, as the service lists it again. Only drawn once there is one.
        if !world.closed.is_empty() && world.rng.chance(0.1) {
            let closed = world.closed.len() as u64;
            let pick = world.rng.below(closed);
            if let Some((channel, author, listed)) = world.reopen_closed(pick) {
                drop(world);
                self.events.send_lossy(listed);
                self.later(Duration::from_millis(1_500), move |world| {
                    world
                        .post(
                            channel,
                            author,
                            "¡Hola de nuevo!".into(),
                            None,
                            Timestamp(now.0 + 1_500),
                        )
                        .map(|message| {
                            vec![Event::MessageCreated {
                                message,
                                nonce: None,
                            }]
                        })
                        .unwrap_or_default()
                });
                return;
            }
        }
        if world.rng.chance(0.3) {
            let count = world.users.len() as u64;
            let index = world.rng.below(count) as usize;
            let user = world.users[index].id;
            if user != ME {
                let status = [
                    Status::Online,
                    Status::Idle,
                    Status::DoNotDisturb,
                    Status::Offline,
                ][world.rng.below(4) as usize];
                let presence = Presence {
                    status,
                    ..world.presences.get(&user).cloned().unwrap_or_default()
                };
                world.presences.insert(user, presence.clone());
                self.events.send_latest(
                    Latest::Presence(user),
                    Event::PresenceChanged { user, presence },
                );
            }
            return;
        }
        let channels: Vec<ChannelId> = world
            .channels
            .iter()
            .filter(|channel| channel.kind == ChannelKind::Text && channel.name != "anuncios")
            .map(|channel| channel.id)
            .collect();
        let channel = channels[world.rng.below(channels.len() as u64) as usize];
        let others: Vec<UserId> = world
            .participants(channel)
            .into_iter()
            .filter(|user| *user != ME)
            .collect();
        let author = others[world.rng.below(others.len() as u64) as usize];
        let mention = world.rng.chance(0.15);
        drop(world);
        self.events.send_lossy(Event::Typing {
            channel,
            user: author,
        });
        self.later(Duration::from_millis(1_500), move |world| {
            let mut line = PHRASES[world.rng.below(PHRASES.len() as u64) as usize].to_owned();
            if mention {
                line = format!("@invitado {line}");
            }
            world
                .post(channel, author, line, None, Timestamp(now.0 + 1_500))
                .map(|message| {
                    vec![Event::MessageCreated {
                        message,
                        nonce: None,
                    }]
                })
                .unwrap_or_default()
        });
    }

    /// Now and then someone else reacts or votes, as a live change.
    fn tally(&mut self) {
        let now = (self.config.clock)();
        if let Some(event) = self.world.lock().unwrap().tally(now) {
            self.events.send_lossy(event);
        }
    }

    /// Who speaks in the voice channel the account is in changes now and then.
    fn speak(&mut self) {
        let mut world = self.world.lock().unwrap();
        let Some(channel) = world
            .voice
            .iter()
            .find(|state| state.user == ME)
            .map(|state| state.channel)
        else {
            return;
        };
        let speakers: Vec<usize> = world
            .voice
            .iter()
            .enumerate()
            .filter(|(_, state)| state.channel == channel && state.user != ME && !state.muted)
            .map(|(index, _)| index)
            .collect();
        if world.rng.chance(0.1) {
            let quality = Some(world.quality());
            let phase = world.call_phase(channel);
            world.call_shown(channel, &phase);
            self.events.send_lossy(Event::CallPhase {
                channel,
                phase,
                quality,
            });
        }
        if speakers.is_empty() || !world.rng.chance(0.5) {
            return;
        }
        let index = speakers[world.rng.below(speakers.len() as u64) as usize];
        world.voice[index].speaking = !world.voice[index].speaking;
        self.events.send_latest(
            Latest::Speaking(world.voice[index].user),
            Event::Speaking {
                channel,
                user: world.voice[index].user,
                speaking: world.voice[index].speaking,
            },
        );
    }
}

/// SplitMix64: small, fast, and the same everywhere.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            0
        } else {
            self.next() % bound
        }
    }

    fn range(&mut self, low: i64, high: i64) -> i64 {
        low + self.below((high - low).max(1) as u64) as i64
    }

    fn chance(&mut self, probability: f64) -> bool {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 <= probability
    }
}

/// A seed for one thing of the world, mixed from its parts.
fn mix(parts: &[u64]) -> Rng {
    let mut rng = Rng(0);
    for part in parts {
        rng.0 ^= *part;
        rng.next();
    }
    rng
}

/// What a channel holds: a made-up history, what was written since, and the
/// changes made to either.
struct ChannelSim {
    generated: u64,
    /// Minutes between two sittings of the channel.
    sitting_gap: i64,
    extra: Vec<Message>,
    /// A changed generated message, or `None` once deleted.
    overlay: HashMap<MessageId, Option<Message>>,
}

struct World {
    seed: u64,
    history_end: Timestamp,
    rng: Rng,
    users: Vec<User>,
    presences: HashMap<UserId, Presence>,
    servers: Vec<Server>,
    channels: Vec<Channel>,
    members: Vec<Member>,
    voice: Vec<VoiceState>,
    sims: HashMap<ChannelId, ChannelSim>,
    read: HashMap<ChannelId, ReadCursor>,
    /// The message each nonce made, so a nonce sent again makes no second one.
    sent: HashMap<u64, SentMessage>,
    bans: HashMap<ServerId, Vec<rune_model::Ban>>,
    audit: HashMap<ServerId, Vec<rune_model::AuditEntry>>,
    invites: HashMap<ServerId, Vec<rune_model::Invite>>,
    webhooks: HashMap<ServerId, Vec<rune_model::Webhook>>,
    integrations: HashMap<ServerId, Vec<rune_model::Integration>>,
    relationships: Vec<rune_model::Relationship>,
    /// Direct conversations the account took off its list, history kept:
    /// they come back when opened again or written in.
    closed: Vec<Channel>,
    /// The next id for something made during the session.
    next_id: u64,
    /// When the command being carried out arrived.
    now: Timestamp,
    /// Others' simulated reactions and votes draw from their own stream, so
    /// the rest of the simulation stays as it was.
    live: Rng,
    /// The thread each message started, so it starts no second one.
    thread_starts: HashMap<MessageId, ChannelId>,
    /// What the account set for itself in each server: «Sobre mí» and banner.
    server_profiles: HashMap<ServerId, (Option<String>, rune_model::ProfileBanner)>,
    /// Notifications, folders, privacy and notes the account keeps (P1).
    account: account::AccountSim,
    // Reads, search and remote members (P2).
    /// Channels whose search index this run already «prepared».
    indexed: std::collections::HashSet<ChannelId>,
    /// The simulated app's modal open (P3).
    app: interactions::AppSim,
    // Private calls.
    calls: calls::CallsSim,
    // Profile editing (edicion-de-perfil).
    /// The pictures the account saved, by the address they were given:
    /// apart from the world, so reading one never waits on the simulation.
    hosted: Hosted,
    // Voice protocol (voz-protocolo).
    voice_protocol: voice_protocol::VoiceProtocolSim,
    // Rules and onboarding (2026-10-18).
    onboarding: onboarding::OnboardingSim,
    // Scheduled events (eventos-programados).
    events: events::EventsSim,
    // AutoMod (automod).
    automod: automod::AutoModSim,
}

/// The pictures the simulated account saved, by their address.
type Hosted = Arc<std::sync::RwLock<HashMap<String, bytes::Bytes>>>;

/// A channel of a made-up server: its name, its kind, and how many messages
/// it has had (zero for a voice channel).
type ChannelPlan = (&'static str, ChannelKind, u64);

/// A made-up server: its name, its accent, and its categories of channels.
type ServerPlan = (
    &'static str,
    u32,
    &'static [(&'static str, &'static [ChannelPlan])],
);

/// The made-up servers.
const SERVERS: [ServerPlan; 4] = [
    (
        "Taller de Rust",
        0xD9_7A_3C,
        &[
            (
                "Información",
                &[
                    ("bienvenida", ChannelKind::Text, 14),
                    ("anuncios", ChannelKind::Text, 18),
                ],
            ),
            (
                "General",
                &[
                    ("general", ChannelKind::Text, 900),
                    ("ayuda", ChannelKind::Text, 420),
                    ("mostrá-tu-proyecto", ChannelKind::Text, 160),
                ],
            ),
            (
                "Voz",
                &[
                    ("Sala común", ChannelKind::Voice, 0),
                    ("Pair programming", ChannelKind::Voice, 0),
                ],
            ),
        ],
    ),
    (
        "Laboratorio limn",
        0x3C_A6_8F,
        &[
            (
                "Desarrollo",
                &[
                    ("limn-core", ChannelKind::Text, 380),
                    ("componentes", ChannelKind::Text, 260),
                    ("rendimiento", ChannelKind::Text, 5_000),
                ],
            ),
            ("Voz", &[("Reunión semanal", ChannelKind::Voice, 0)]),
        ],
    ),
    (
        "Café Nocturno",
        0x8A_6C_D9,
        &[
            (
                "Charlas",
                &[
                    ("charla", ChannelKind::Text, 640),
                    ("música", ChannelKind::Text, 210),
                    ("off-topic", ChannelKind::Text, 330),
                ],
            ),
            ("Voz", &[("Sala chill", ChannelKind::Voice, 0)]),
        ],
    ),
    (
        "Club de Fotografía",
        0x4F_8D_D6,
        &[(
            "Fotos",
            &[
                ("galería", ChannelKind::Text, 240),
                ("técnica", ChannelKind::Text, 150),
            ],
        )],
    ),
];

/// People of the simulation: handle, name, accent, and a status line.
const PEOPLE: [(&str, &str, u32, Option<&str>); 26] = [
    (
        "ada",
        "Ada",
        0xE0_6C_75,
        Some("probando el nuevo scheduler"),
    ),
    ("bruno", "Bruno", 0x61_AF_EF, None),
    ("cami", "Camila", 0x98_C3_79, Some("de vacaciones 🌴")),
    ("dario", "Darío", 0xD1_9A_66, None),
    ("elena", "Elena", 0xC6_78_DD, Some("revisando PRs")),
    ("facu", "Facundo", 0x56_B6_C2, None),
    ("gala", "Gala", 0xE5_C0_7B, None),
    ("hugo", "Hugo", 0xBE_50_46, Some("🦀")),
    ("ines", "Inés", 0x7E_C6_99, None),
    ("julian", "Julián", 0x52_8B_FF, None),
    ("karen", "Karen", 0xF2_8F_AD, None),
    ("lucas", "Lucas", 0x8F_BC_BB, Some("escuchando vinilos")),
    ("mora", "Mora", 0xB4_8E_AD, None),
    ("nico", "Nico", 0xEB_CB_8B, None),
    ("olivia", "Olivia", 0x81_A1_C1, None),
    ("pablo", "Pablo", 0xA3_BE_8C, None),
    ("quimey", "Quimey", 0xD0_87_70, Some("en el laboratorio")),
    ("rocio", "Rocío", 0x88_C0_D0, None),
    ("santi", "Santiago", 0xBF_61_6A, None),
    ("tania", "Tania", 0x5E_81_AC, None),
    ("uriel", "Uriel", 0xA9_DC_76, None),
    (
        "valen",
        "Valentina",
        0xFF_D8_66,
        Some("fotografiando la ciudad"),
    ),
    ("wanda", "Wanda", 0xAB_9D_F2, None),
    ("ximena", "Ximena", 0x78_DC_E8, None),
    ("yago", "Yago", 0xFC_98_67, None),
    ("zoe", "Zoe", 0xFF_61_88, None),
];

/// Lines of made-up conversation.
const PHRASES: [&str; 40] = [
    "¿Alguien probó la nueva versión? A mí me compila bastante más rápido.",
    "Buen día a todos ☕",
    "Me pasó lo mismo, al final era un problema de lifetimes.",
    "Dejo esto por acá: https://doc.rust-lang.org/book/",
    "**Ojo:** si usás `tokio::select!` dentro de un loop, cancelás el futuro que no gana.",
    "¿Cuál es la diferencia entre `Arc<Mutex<T>>` y un canal? Siempre me confundo.",
    "Jajaja tremendo",
    "Lo subí a una rama, si alguien quiere revisarlo avisen.",
    "Para mí el problema está en el layout, no en el render.",
    "```rust\nfn main() {\n    let lista: Vec<u32> = (1..=5).collect();\n    println!(\"{lista:?}\");\n}\n```",
    "Tres cosas que aprendí hoy:\n- `cargo tree -d` muestra dependencias duplicadas\n- `cargo bloat` es una maravilla\n- dormir ayuda",
    "> Primero que funcione, después que sea rápido.\nTotalmente de acuerdo.",
    "Mañana a las 18 hacemos la reunión, ¿les queda bien?",
    "Gracias!! Eso era exactamente lo que necesitaba 🙌",
    "Probé con `cargo clippy -- -W clippy::pedantic` y me tiró 200 avisos 😅",
    "¿Alguien tiene experiencia con PipeWire desde Rust?",
    "Ya está mergeado 🎉",
    "No sé si es un bug o si lo estoy usando mal.",
    "El benchmark nuevo da 3x mejor en el peor caso.",
    "Les comparto una foto del setup de hoy.",
    "Che, ¿cómo hacen para medir el uso de memoria por frame?",
    "Lo más lindo de Rust es cuando compila y anda a la primera.",
    "Hay que agregar un test para ese caso.",
    "¿Usaron `rusqlite` con el feature `bundled`? Evita depender de la versión del sistema.",
    "Una pregunta medio tonta: ¿por qué `String` y `&str`?",
    "No es tonta para nada, es *la* pregunta de Rust.",
    "Recién terminé de leer el capítulo de async, me explotó la cabeza.",
    "Vamos que el viernes se libera 🚀",
    "¿Qué editor usan? Yo con Helix estoy muy cómodo.",
    "Wayland por fin anda bien en mi notebook.",
    "Esto merece un issue aparte.",
    "Agregué la documentación en `docs/ARCHITECTURE.md`.",
    "Buenísimo, lo pruebo a la noche.",
    "Hice un perfil con `perf` y el 40% se va en clonar strings 🙃",
    "El scroll ahora no salta al cargar mensajes viejos 👌",
    "¿Alguien sabe si hay que reiniciar después de actualizar?",
    "Café número tres del día.",
    "Paso a saludar, hace mucho que no entraba 👋",
    "Esa librería tiene licencia MIT, así que no hay problema.",
    "Mirá este artículo sobre listas virtualizadas: https://github.com/Niiihuel/limn",
];

/// What people answer to a message sent here.
const REPLIES: [&str; 10] = [
    "¡Buena pregunta! Yo lo resolví así: primero aislé el caso en un test.",
    "Coincido 👍",
    "Jaja sí, me pasó la semana pasada.",
    "¿Podés pegar el error completo?",
    "Lo miro en un rato y te digo.",
    "¡Bienvenido/a! Cualquier cosa preguntá por acá.",
    "Excelente, gracias por avisar 🙌",
    "Eso está documentado en el README, fijate la sección de instalación.",
    "Yo probaría con `cargo clean` primero, por las dudas.",
    "Me encanta 😄",
];

const EMOJIS: [&str; 8] = ["👍", "🎉", "❤️", "😂", "🚀", "👀", "🦀", "🙌"];
const PICTURES: [(&str, u32, u32); 5] = [
    ("atardecer.jpg", 1600, 1067),
    ("calle-lluvia.jpg", 1200, 1600),
    ("montaña.png", 1920, 1080),
    ("macro-hoja.jpg", 1080, 1080),
    ("captura.png", 1280, 800),
];
/// Link previews of made-up sites: (site, title, text, stripe).
const CARDS: [(&str, &str, &str, u32); 3] = [
    (
        "Wiki de ejemplo",
        "Programación con Rust",
        "Un lenguaje de sistemas con **seguridad de memoria** sin recolector.",
        0xDEA584,
    ),
    (
        "Video simulado",
        "Charla: interfaces nativas sin navegador",
        "Cómo dibujar una aplicación de escritorio completa con la GPU.",
        0xFF0000,
    ),
    (
        "Noticias de prueba",
        "Se publicó una nueva versión",
        "Notas de la versión, cambios y correcciones.",
        0x5865F2,
    ),
];
const FILES: [(&str, u64, &str); 3] = [
    ("notas-reunion.pdf", 482_133, "application/pdf"),
    ("benchmark.csv", 18_402, "text/csv"),
    ("proyecto.zip", 3_204_871, "application/zip"),
];
/// The loudness of the simulated voice message's tone (assets/voice-sample.ogg):
/// under two syllables a second, louder and softer in turn.
const SIMULATED_VOICE: [u8; 48] = [
    89, 173, 163, 16, 0, 0, 0, 126, 245, 222, 43, 0, 0, 0, 107, 210, 185, 46, 0, 0, 0, 54, 108, 91,
    26, 0, 0, 0, 19, 48, 48, 19, 0, 0, 0, 26, 91, 108, 54, 0, 0, 0, 46, 185, 210, 107, 0, 0,
];

impl World {
    fn build(config: &MockConfig) -> Self {
        let mut rng = mix(&[config.seed, 1]);
        let mut users = vec![User {
            identity: Default::default(),
            avatar: None,
            banner: Default::default(),
            id: ME,
            name: "invitado".into(),
            display_name: Some("Invitado".into()),
            accent: Rgb(0xE0_A4_3C),
            bot: false,
            pronouns: None,
            about: None,
            default_avatar: None,
        }];
        let mut presences = HashMap::from([(
            ME,
            Presence {
                status: Status::Online,
                custom: None,
                custom_emoji: None,
                activities: Vec::new(),
                platforms: rune_model::Platforms {
                    desktop: true,
                    ..Default::default()
                },
            },
        )]);
        for (index, (handle, name, accent, custom)) in PEOPLE.iter().enumerate() {
            let id = UserId(10 + index as u64);
            users.push(User {
                identity: Default::default(),
                avatar: None,
                banner: Default::default(),
                id,
                name: (*handle).into(),
                display_name: Some((*name).into()),
                accent: Rgb(*accent),
                bot: false,
                pronouns: [Some("ella"), Some("él"), None, Some("elle"), None][index % 5]
                    .map(str::to_owned),
                about: (index % 3 == 0).then(|| ABOUT[index % ABOUT.len()].to_owned()),
                default_avatar: None,
            });
            let status = [
                Status::Online,
                Status::Online,
                Status::Idle,
                Status::DoNotDisturb,
                Status::Offline,
                Status::Offline,
            ][rng.below(6) as usize];
            presences.insert(
                id,
                Presence {
                    status,
                    custom: custom.map(str::to_owned),
                    custom_emoji: identity::status_emoji(index),
                    activities: identity::activities(index, config.history_end),
                    platforms: if status.is_around() {
                        identity::platforms(index)
                    } else {
                        Default::default()
                    },
                },
            );
        }
        let bot = UserId(9);
        users.push(User {
            identity: Default::default(),
            avatar: None,
            banner: Default::default(),
            id: bot,
            name: "runebot".into(),
            display_name: Some("RuneBot".into()),
            accent: Rgb(0x6C_8E_BF),
            bot: true,
            pronouns: None,
            about: Some("Un bot de la simulación: contesta y reacciona.".into()),
            default_avatar: None,
        });
        presences.insert(
            bot,
            Presence {
                status: Status::Online,
                custom: Some(t("simulado").into()),
                custom_emoji: None,
                activities: Vec::new(),
                platforms: rune_model::Platforms {
                    desktop: true,
                    ..Default::default()
                },
            },
        );

        let mut world = Self {
            seed: config.seed,
            history_end: config.history_end,
            rng: mix(&[config.seed, 2]),
            live: mix(&[config.seed, 16]),
            users,
            presences,
            servers: Vec::new(),
            channels: Vec::new(),
            members: Vec::new(),
            voice: Vec::new(),
            sims: HashMap::new(),
            read: HashMap::new(),
            sent: HashMap::new(),
            bans: HashMap::new(),
            audit: HashMap::new(),
            invites: HashMap::new(),
            webhooks: HashMap::new(),
            integrations: HashMap::new(),
            relationships: social::starting(config.history_end),
            closed: Vec::new(),
            next_id: 1_000_000,
            now: config.history_end,
            thread_starts: HashMap::new(),
            server_profiles: HashMap::new(),
            account: account::AccountSim::starting(),
            indexed: Default::default(),
            app: Default::default(),
            calls: Default::default(),
            hosted: Default::default(),
            voice_protocol: Default::default(),
            onboarding: Default::default(),
            events: Default::default(),
            automod: Default::default(),
        };
        let people: Vec<UserId> = world
            .users
            .iter()
            .map(|user| user.id)
            .filter(|id| *id != ME)
            .collect();
        let mut next_channel = 100;
        for (server_index, (name, accent, categories)) in SERVERS.iter().enumerate() {
            let server = ServerId(1 + server_index as u64);
            let roles = vec![
                Role::everyone(server, Permissions::EVERYONE),
                Role {
                    style: Default::default(),
                    id: RoleId(server.0 * 10 + 1),
                    name: "Moderación".into(),
                    color: Some(Rgb(0xE0_6C_75)),
                    position: 3,
                    hoisted: true,
                    mentionable: true,
                    permissions: Permissions::MANAGE_MESSAGES
                        | Permissions::KICK
                        | Permissions::BAN
                        | Permissions::MANAGE_NICKS
                        | Permissions::MUTE_MEMBERS
                        | Permissions::MOVE_MEMBERS
                        | Permissions::TIMEOUT_MEMBERS
                        | Permissions::VIEW_AUDIT_LOG
                        | Permissions::MENTION_EVERYONE,
                },
                Role {
                    style: Default::default(),
                    id: RoleId(server.0 * 10 + 2),
                    name: "Colaboración".into(),
                    color: Some(Rgb(0x61_AF_EF)),
                    position: 2,
                    hoisted: true,
                    mentionable: false,
                    permissions: Permissions::MANAGE_THREADS | Permissions::PRIORITY_SPEAKER,
                },
                Role {
                    style: Default::default(),
                    id: RoleId(server.0 * 10 + 3),
                    name: "Bots".into(),
                    color: Some(Rgb(0x6C_8E_BF)),
                    position: 1,
                    hoisted: true,
                    mentionable: false,
                    permissions: Permissions::EMBED_LINKS,
                },
            ];
            // Each server has most people, the account, and the bot in the first.
            let mut members: Vec<UserId> = people
                .iter()
                .copied()
                .filter(|user| *user != bot && rng.chance(0.75))
                .collect();
            if server_index == 0 {
                members.push(bot);
            }
            members.push(ME);
            for (position, user) in members.iter().enumerate() {
                let role = match (*user, position % 7) {
                    (user, _) if user == bot => vec![roles[3].id],
                    (user, _) if user == ME => Vec::new(),
                    (_, 0) => vec![roles[1].id],
                    (_, 1 | 2) => vec![roles[2].id],
                    _ => Vec::new(),
                };
                world.members.push(Member {
                    user: *user,
                    server,
                    nick: None,
                    roles: role,
                    ..Default::default()
                });
            }
            // The account owns the second server; someone else owns the rest.
            let owner = if server_index == 1 { ME } else { members[0] };
            let moderation = roles[1].id;
            world.servers.push(Server {
                stickers: Vec::new(),
                emojis: Vec::new(),
                icon: None,
                id: server,
                name: (*name).into(),
                accent: Rgb(*accent),
                roles,
                owner: Some(owner),
                description: Some(tr!("Servidor simulado: {name}.", name)),
                options: Default::default(),
            });
            for (category_index, (category_name, channels)) in categories.iter().enumerate() {
                let category = ChannelId(next_channel);
                next_channel += 1;
                world.channels.push(Channel {
                    id: category,
                    server: Some(server),
                    kind: ChannelKind::Category,
                    name: (*category_name).into(),
                    topic: None,
                    topic_pictures: Vec::new(),
                    parent: None,
                    position: category_index as i32,
                    recipients: Vec::new(),
                    owner: None,
                    permissions: Permissions::MEMBER,
                    overwrites: Vec::new(),
                    slowmode_secs: 0,
                    nsfw: false,
                    icon: None,
                    thread: None,
                    tags: Vec::new(),
                    forum: Default::default(),
                });
                for (position, (channel_name, kind, count)) in channels.iter().enumerate() {
                    let id = ChannelId(next_channel);
                    next_channel += 1;
                    let read_only = *channel_name == "anuncios";
                    world.channels.push(Channel {
                        id,
                        server: Some(server),
                        kind: *kind,
                        name: (*channel_name).into(),
                        topic: topic(channel_name),
                        topic_pictures: topic(channel_name)
                            .map(|topic| identity::pictures_in(&topic))
                            .unwrap_or_default(),
                        parent: Some(category),
                        position: position as i32,
                        recipients: Vec::new(),
                        owner: None,
                        // Worked out from the overwrites once the world is built.
                        permissions: Permissions::NONE,
                        // Announcements: only moderation writes.
                        overwrites: if read_only {
                            vec![
                                Overwrite {
                                    target: OverwriteTarget::Role(RoleId(server.0)),
                                    allow: Permissions::NONE,
                                    deny: Permissions::SEND | Permissions::ATTACH,
                                },
                                Overwrite {
                                    target: OverwriteTarget::Role(moderation),
                                    allow: Permissions::SEND | Permissions::ATTACH,
                                    deny: Permissions::NONE,
                                },
                            ]
                        } else if *channel_name == "off-topic" {
                            // Only this server's expressions (2026-10-18):
                            // the picker's lock for another's shows.
                            vec![Overwrite {
                                target: OverwriteTarget::Role(RoleId(server.0)),
                                allow: Permissions::NONE,
                                deny: Permissions::EXTERNAL_EMOJI | Permissions::EXTERNAL_STICKERS,
                            }]
                        } else {
                            Vec::new()
                        },
                        // Questions wait a little between two: the slow mode shows.
                        slowmode_secs: if *channel_name == "ayuda" { 10 } else { 0 },
                        nsfw: false,
                        icon: None,
                        thread: None,
                        tags: Vec::new(),
                        forum: Default::default(),
                    });
                    if *kind == ChannelKind::Voice {
                        // Some voice channels already have people talking.
                        let present = if position == 0 { 3 } else { 0 };
                        for (index, user) in members
                            .iter()
                            .filter(|user| **user != ME && **user != bot)
                            .take(present)
                            .enumerate()
                        {
                            let muted = rng.chance(0.3);
                            world.voice.push(VoiceState {
                                user: *user,
                                channel: id,
                                muted,
                                deafened: false,
                                speaking: false,
                                streaming: false,
                                // The first server's moderation silenced someone in its room.
                                server_muted: server_index == 0 && index == 0,
                                server_deafened: false,
                                camera: false,
                                suppressed: false,
                                speak_request: None,
                            });
                        }
                        // The first server's second room is full and shows a
                        // screen and two cameras, from a stream of its own: the rest of the
                        // world comes out as it did.
                        if server_index == 0 && position == 1 {
                            let mut extra = mix(&[config.seed, id.0, 0x56_4F_49_43_45]);
                            let busy: Vec<UserId> =
                                world.voice.iter().map(|state| state.user).collect();
                            for (index, user) in members
                                .iter()
                                .filter(|user| {
                                    **user != ME && **user != bot && !busy.contains(user)
                                })
                                .take(5)
                                .enumerate()
                            {
                                world.voice.push(VoiceState {
                                    user: *user,
                                    channel: id,
                                    muted: extra.chance(0.3),
                                    deafened: false,
                                    speaking: false,
                                    // Two of them share their screen, to watch at once.
                                    streaming: index == 0 || index == 3,
                                    server_muted: false,
                                    server_deafened: false,
                                    // Two of them have their camera on.
                                    camera: index == 1 || index == 2,
                                    suppressed: false,
                                    speak_request: None,
                                });
                            }
                        }
                    }
                    // A voice room's chat has a few messages, as with a provider
                    // that declares `voice_text`.
                    let count = if *kind == ChannelKind::Voice {
                        VOICE_CHAT
                    } else {
                        *count
                    };
                    let sitting_gap = if count > 1_000 {
                        120
                    } else {
                        60 * 6 * 100 / (count as i64).max(30)
                    };
                    world.sims.insert(
                        id,
                        ChannelSim {
                            generated: count,
                            sitting_gap,
                            extra: Vec::new(),
                            overlay: HashMap::new(),
                        },
                    );
                }
            }
        }
        // Direct conversations, and one group.
        let directs: [(&str, &[usize], u64); 4] = [
            ("", &[0], 120),
            ("", &[1], 60),
            ("", &[2], 30),
            ("Proyecto RUNE", &[0, 7, 8], 200),
        ];
        for (position, (name, people_index, count)) in directs.iter().enumerate() {
            let id = ChannelId(next_channel);
            next_channel += 1;
            let recipients: Vec<UserId> = people_index
                .iter()
                .map(|index| UserId(10 + *index as u64))
                .collect();
            world.channels.push(Channel {
                id,
                server: None,
                kind: if recipients.len() > 1 {
                    ChannelKind::Group
                } else {
                    ChannelKind::Direct
                },
                name: (*name).into(),
                topic: None,
                topic_pictures: Vec::new(),
                parent: None,
                position: position as i32,
                // The account made the group; someone else opened each conversation.
                owner: (recipients.len() > 1).then_some(ME),
                recipients,
                permissions: Permissions::MEMBER | Permissions::READ_HISTORY,
                overwrites: Vec::new(),
                slowmode_secs: 0,
                nsfw: false,
                icon: None,
                thread: None,
                tags: Vec::new(),
                forum: Default::default(),
            });
            world.sims.insert(
                id,
                ChannelSim {
                    generated: *count,
                    sitting_gap: 600,
                    extra: Vec::new(),
                    overlay: HashMap::new(),
                },
            );
        }
        world.add_threads(&mut next_channel);
        // A stage beside the first server's rooms, with two speakers and an
        // audience. Added last so no other id moves.
        if let Some(room) = world
            .channels
            .iter()
            .find(|found| found.kind == ChannelKind::Voice)
            .cloned()
        {
            let stage = ChannelId(next_channel);
            world.channels.push(Channel {
                id: stage,
                kind: ChannelKind::Stage,
                name: format!("Escenario {}", t("(simulado)")),
                position: 2,
                ..room
            });
            // Its chat, like a voice room's.
            world.sims.insert(
                stage,
                ChannelSim {
                    generated: VOICE_CHAT,
                    sitting_gap: 60 * 6 * 100 / 30,
                    extra: Vec::new(),
                    overlay: HashMap::new(),
                },
            );
            world.add_stage_people(stage);
        }
        // Desktop (P6): a channel of the first server only its moderation
        // sees, for «Mostrar los canales ocultos». Last, so no other id moves.
        if let Some(server) = world.servers.first().map(|server| server.id) {
            let moderation = world.servers[0].roles.get(1).map(|role| role.id);
            let category = world
                .channels
                .iter()
                .find(|found| found.server == Some(server) && found.kind == ChannelKind::Category)
                .map(|found| found.id);
            world.channels.push(Channel {
                id: ChannelId(next_channel + 1),
                server: Some(server),
                kind: ChannelKind::Text,
                name: "moderación".into(),
                topic: Some("Solo para la moderación.".into()),
                topic_pictures: Vec::new(),
                parent: category,
                position: 9,
                recipients: Vec::new(),
                owner: None,
                permissions: Permissions::NONE,
                overwrites: std::iter::once(Overwrite {
                    target: OverwriteTarget::Role(RoleId(server.0)),
                    allow: Permissions::NONE,
                    deny: Permissions::VIEW,
                })
                .chain(moderation.map(|role| Overwrite {
                    target: OverwriteTarget::Role(role),
                    allow: Permissions::VIEW,
                    deny: Permissions::NONE,
                }))
                .collect(),
                slowmode_secs: 0,
                nsfw: false,
                icon: None,
                thread: None,
                tags: Vec::new(),
                forum: Default::default(),
            });
        }
        world.add_admin();
        world.add_server_options();
        // Rules and onboarding (2026-10-18).
        world.add_onboarding();
        // Scheduled events (eventos-programados).
        world.add_events();
        // AutoMod (automod).
        world.add_automod();
        world.add_integrations();
        world.add_identities();
        // Most channels were read to the end; a few have something new.
        let ids: Vec<ChannelId> = world.sims.keys().copied().collect();
        for channel in ids {
            let total = world.sims[&channel].generated;
            let unread = match world.channel(channel).map(|channel| channel.name.as_str()) {
                Some("general") => 14,
                Some("ayuda") => 3,
                Some("charla") => 6,
                Some("Salida fotográfica del sábado") => 2,
                Some("") if channel == world.first_direct() => 2,
                _ => 0,
            };
            let last_read = total
                .checked_sub(1 + unread)
                .map(|index| message_id(channel, index));
            world.read.insert(
                channel,
                ReadCursor {
                    message: last_read,
                    ..Default::default()
                },
            );
        }
        let servers: Vec<ServerId> = world.servers.iter().map(|server| server.id).collect();
        for server in servers {
            world.refresh_permissions(server);
        }
        world.restrict(config);
        world
    }

    fn channel(&self, id: ChannelId) -> Option<&Channel> {
        self.channels.iter().find(|channel| channel.id == id)
    }

    fn first_direct(&self) -> ChannelId {
        self.channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Direct)
            .map_or(ChannelId(0), |channel| channel.id)
    }

    fn is_direct(&self, channel: ChannelId) -> bool {
        self.channel(channel)
            .is_some_and(|channel| channel.server.is_none())
    }

    /// Who writes in `channel`: its server's members, or the people of the conversation.
    fn participants(&self, channel: ChannelId) -> Vec<UserId> {
        match self.channel(channel) {
            Some(Channel {
                server: Some(server),
                ..
            }) => self
                .members
                .iter()
                .filter(|member| member.server == *server)
                .map(|member| member.user)
                .collect(),
            Some(channel) => channel.recipients.iter().copied().chain([ME]).collect(),
            None => Vec::new(),
        }
    }

    fn snapshot(&self) -> Snapshot {
        let mut last_messages = Vec::new();
        let mut read_states = Vec::new();
        let mut read_versions = Vec::new();
        let mut first_unread = Vec::new();
        for (channel, sim) in &self.sims {
            let total = sim.generated + sim.extra.len() as u64;
            // A closed conversation keeps its history but is not listed;
            // an archived thread is read with its page.
            if total == 0 || self.channel(*channel).is_none_or(World::archived_thread) {
                continue;
            }
            let last = sim
                .extra
                .last()
                .map_or(message_id(*channel, total - 1), |message| message.id);
            last_messages.push((*channel, last));
            let read = self.read.get(channel).and_then(|read| read.message);
            let mut first = None;
            let mentions = (0..total)
                .map(|index| message_id(*channel, index))
                .filter(|id| read.is_none_or(|read| *id > read))
                .filter_map(|id| self.message(*channel, id))
                .inspect(|message| {
                    first.get_or_insert(message.id);
                })
                .filter(|message| message.mentions(ME))
                .count() as u32;
            read_states.push((*channel, read, mentions));
            read_versions.push((
                *channel,
                self.read.get(channel).map_or(1, |read| read.version),
            ));
            if let Some(first) = first {
                first_unread.push((*channel, first));
            }
        }
        last_messages.sort();
        read_states.sort_by_key(|state| state.0);
        read_versions.sort_by_key(|state| state.0);
        first_unread.sort_by_key(|state| state.0);
        let mut presences: Vec<(UserId, Presence)> = self
            .presences
            .iter()
            .map(|(user, presence)| (*user, presence.clone()))
            .collect();
        presences.sort_by_key(|(user, _)| *user);
        Snapshot {
            me: self.users.first().cloned(),
            users: self.users.clone(),
            servers: self.servers.clone(),
            channels: self
                .channels
                .iter()
                .filter(|channel| !World::archived_thread(channel))
                .cloned()
                .collect(),
            members: self.members.clone(),
            presences,
            voice: self.voice.clone(),
            last_messages,
            read_states,
            read_versions,
            first_unread,
            relationships: Some(self.relationships.clone()),
        }
    }

    /// The message `id` of `channel` as it is now: made up, written since,
    /// or changed; `None` once deleted or when there is no such message.
    fn message(&self, channel: ChannelId, id: MessageId) -> Option<Message> {
        // A closed conversation's history waits until it is listed again.
        self.channel(channel)?;
        let sim = self.sims.get(&channel)?;
        if let Some(changed) = sim.overlay.get(&id) {
            return changed.clone();
        }
        let index = id.0.checked_sub(channel.0 * MESSAGES_PER_CHANNEL)?;
        if index < sim.generated {
            Some(self.generate(channel, sim, index))
        } else {
            sim.extra.get((index - sim.generated) as usize).cloned()
        }
    }

    /// Up to `limit` messages of `channel` before `before`, oldest first, and
    /// whether they reach its first message.
    fn history(
        &self,
        channel: ChannelId,
        before: Option<MessageId>,
        limit: usize,
    ) -> (Vec<Message>, bool) {
        let Some(sim) = self
            .sims
            .get(&channel)
            .filter(|_| self.channel(channel).is_some())
        else {
            return (Vec::new(), true);
        };
        let total = sim.generated + sim.extra.len() as u64;
        let end = before
            .and_then(|before| before.0.checked_sub(channel.0 * MESSAGES_PER_CHANNEL))
            .map_or(total, |index| index.min(total));
        let start = end.saturating_sub(limit as u64);
        let messages = (start..end)
            .filter_map(|index| self.message(channel, message_id(channel, index)))
            .collect();
        (messages, start == 0)
    }

    /// Neighbors of an existing exact anchor, without a continuity claim.
    fn context(&self, channel: ChannelId, anchor: MessageId, limit: usize) -> Option<Vec<Message>> {
        let channel_info = self.channel(channel)?;
        if !channel_info.kind.has_messages()
            || !channel_info
                .permissions
                .contains(Permissions::VIEW | Permissions::READ_HISTORY)
        {
            return None;
        }
        self.message(channel, anchor)?;
        let sim = self.sims.get(&channel)?;
        let index = anchor
            .0
            .checked_sub(channel.0.checked_mul(MESSAGES_PER_CHANNEL)?)?;
        let total = sim.generated + sim.extra.len() as u64;
        let limit = limit.clamp(1, 100) as u64;
        let start = index.saturating_sub(limit / 2);
        let end = start.saturating_add(limit).min(total);
        Some(
            (start..end)
                .filter_map(|index| self.message(channel, message_id(channel, index)))
                .collect(),
        )
    }

    /// Message `index` of the made-up history of `channel`.
    fn generate(&self, channel: ChannelId, sim: &ChannelSim, index: u64) -> Message {
        let mut rng = mix(&[self.seed, channel.0, index]);
        let last = sim.generated.saturating_sub(1);
        let (sitting, within) = (index / BURST, index % BURST);
        let back = (last - index) as i64 * 2 + (last / BURST - sitting) as i64 * sim.sitting_gap;
        let timestamp = Timestamp(self.history_end.0 - back * 60_000 + rng.below(50_000) as i64);
        let participants = self.participants(channel);
        let name = self
            .channel(channel)
            .map_or("", |channel| channel.name.as_str());
        // A sitting is a couple of people; each keeps the word for a few messages.
        let mut sitting_rng = mix(&[self.seed, channel.0, sitting, 7]);
        let first = participants[sitting_rng.below(participants.len() as u64) as usize];
        let second = participants[sitting_rng.below(participants.len() as u64) as usize];
        let mut author = first;
        for _ in 0..within {
            if !sitting_rng.chance(0.6) {
                author = if author == first { second } else { first };
            }
        }
        if author == ME && !self.is_direct(channel) && !rng.chance(0.3) {
            author = second;
        }
        let id = message_id(channel, index);
        if name == "bienvenida" {
            return Message {
                id,
                channel,
                author: participants[(index as usize * 5) % participants.len()],
                content: String::new(),
                timestamp,
                edited: None,
                kind: MessageKind::Join,
                reply_to: None,
                mentions: Vec::new(),
                reactions: vec![Reaction {
                    emoji: "👋".into(),
                    count: 1 + rng.below(6) as u32,
                    me: false,
                    ..Default::default()
                }],
                attachments: Vec::new(),
                stickers: Vec::new(),
                text_references: Vec::new(),
                delivery: Delivery::Sent,
                pinned: false,
                embeds: Vec::new(),
                forwarded: None,
                poll: None,
                components: Default::default(),
                call: None,
                mention_roles: Vec::new(),
                mention_everyone: false,
            };
        }
        let mut content = PHRASES[rng.below(PHRASES.len() as u64) as usize].to_owned();
        let mut mentions = Vec::new();
        if author != ME && rng.chance(0.04) {
            content = format!("@invitado {content}");
            mentions.push(ME);
        } else if rng.chance(0.05) {
            let other = participants[rng.below(participants.len() as u64) as usize];
            if let Some(user) = self.users.iter().find(|user| user.id == other) {
                content = format!("@{} {content}", user.name);
                mentions.push(other);
            }
        }
        if name == "anuncios" {
            content = format!(
                "## Novedades de la semana {}\n\n- Se publicó una versión nueva con **mejoras de rendimiento**.\n- La documentación está en https://github.com/Niiihuel/limn\n\nCualquier duda, en #ayuda.",
                index + 1
            );
            mentions.clear();
        }
        let reply_to = (index > 3 && rng.chance(0.07))
            .then(|| message_id(channel, index - 1 - rng.below(index.min(12))))
            .filter(|reply| reply.0 < id.0);
        let mut reactions = Vec::new();
        if rng.chance(0.16) {
            for _ in 0..1 + rng.below(3) {
                let emoji = EMOJIS[rng.below(EMOJIS.len() as u64) as usize];
                if reactions
                    .iter()
                    .all(|reaction: &Reaction| reaction.emoji != emoji)
                {
                    reactions.push(Reaction {
                        emoji: emoji.into(),
                        count: 1 + rng.below(6) as u32,
                        me: rng.chance(0.2),
                        ..Default::default()
                    });
                }
            }
        }
        let mut attachments = Vec::new();
        let pictures = if name == "galería" { 0.45 } else { 0.03 };
        if rng.chance(pictures) {
            let (filename, width, height) = PICTURES[rng.below(PICTURES.len() as u64) as usize];
            attachments.push(Attachment {
                id: AttachmentId(id.0),
                filename: filename.into(),
                size_bytes: 180_000 + rng.below(2_400_000),
                content_type: Some(
                    if filename.ends_with(".png") {
                        "image/png"
                    } else {
                        "image/jpeg"
                    }
                    .into(),
                ),
                dimensions: Some((width, height)),
                preview: None,
                download: None,
                spoiler: false,
                voice: None,
                explicit: false,
                placeholder: None,
            });
        } else if rng.chance(0.015) {
            let (filename, size_bytes, content_type) =
                FILES[rng.below(FILES.len() as u64) as usize];
            attachments.push(Attachment {
                id: AttachmentId(id.0),
                filename: filename.into(),
                size_bytes,
                content_type: Some(content_type.into()),
                dimensions: None,
                preview: None,
                download: None,
                spoiler: false,
                voice: None,
                explicit: false,
                placeholder: None,
            });
        }
        // Separate draws keep every other field of the history as it was.
        let mut rich = mix(&[self.seed, channel.0, index, 14]);
        let embeds = if rich.chance(0.03) {
            let (site, title, text, color) = CARDS[rich.below(CARDS.len() as u64) as usize];
            vec![rune_model::Embed {
                provider: Some(site.into()),
                title: Some(title.into()),
                url: Some(format!("https://example.invalid/{}", id.0)),
                description: Some(text.into()),
                color: Some(color),
                ..Default::default()
            }]
        } else {
            Vec::new()
        };
        // A poll a few messages from the end of #general, open two days.
        let poll = (name == "general" && index + 6 == sim.generated).then(|| {
            let mut votes = mix(&[self.seed, channel.0, index, 15]);
            let answer = |id: u32, text: &str, emoji: Option<&str>, votes: u32| PollAnswer {
                id,
                text: text.into(),
                emoji: emoji.map(str::to_owned),
                votes,
                me: false,
            };
            Poll {
                question: format!("¿Qué probamos en la próxima reunión? {}", t("(simulada)")),
                answers: vec![
                    answer(1, "Hilos y foros", Some("🧵"), 2 + votes.below(6) as u32),
                    answer(
                        2,
                        "Mensajes privados",
                        Some("👋"),
                        1 + votes.below(5) as u32,
                    ),
                    answer(3, "Llamadas de voz", None, votes.below(4) as u32),
                ],
                expiry: Some(self.history_end.plus_minutes(48 * 60)),
                multiselect: false,
                results_known: true,
                finalized: false,
            }
        });
        if poll.is_some() {
            content.clear();
            mentions.clear();
        }
        // A voice message a few messages from the end of #general. It has no
        // file to fetch: the host plays a tone made on this machine.
        if name == "general" && index + 4 == sim.generated {
            content = t("Mensaje de voz simulado: un tono generado en este equipo.").into();
            mentions.clear();
            attachments = vec![Attachment {
                id: AttachmentId(id.0),
                filename: "voice-message.ogg".into(),
                size_bytes: 8_708,
                content_type: Some("audio/ogg".into()),
                dimensions: None,
                preview: None,
                download: None,
                spoiler: false,
                voice: Some(rune_model::VoiceClip {
                    duration_secs: 4.0,
                    waveform: SIMULATED_VOICE.to_vec(),
                }),
                explicit: false,
                placeholder: None,
            }];
        }
        // Rail and notifications: a mention of a role of the server the
        // account owns, which tells it once it takes that role there.
        if name == "componentes" && index + 2 == sim.generated {
            let role = self
                .channel(channel)
                .and_then(|found| found.server)
                .and_then(|server| self.server(server).ok())
                .and_then(|server| server.roles.iter().find(|role| role.mentionable));
            if let Some(role) = role {
                content = format!("<@&{}> ¿alguien puede revisar el cambio de hoy?", role.id);
                mentions.clear();
            }
        }
        let (mention_roles, mention_everyone) = self.told(channel, author, &content);
        let mut message = Message {
            id,
            channel,
            author,
            content,
            timestamp,
            edited: rng.chance(0.04).then(|| timestamp.plus_minutes(3)),
            kind: MessageKind::Default,
            reply_to,
            mentions,
            reactions,
            attachments,
            stickers: Vec::new(),
            text_references: Vec::new(),
            delivery: Delivery::Sent,
            pinned: made_pins(self.seed, channel, name, sim.generated).contains(&index),
            embeds,
            forwarded: None,
            poll,
            components: Default::default(),
            call: None,
            mention_roles,
            mention_everyone,
        };
        if name == "general" {
            self.showcase(channel, sim, index, &mut message);
        }
        // Records of calls with Ada (contenido-de-mensajes).
        if self.is_direct(channel) {
            self.calls(channel, sim, index, &mut message);
        }
        message
    }

    /// The account's message for `nonce`: written now, or the one the same
    /// nonce already made, so retrying a send never repeats it.
    fn post_once(
        &mut self,
        nonce: u64,
        channel: ChannelId,
        content: String,
        reply_to: Option<MessageId>,
        timestamp: Timestamp,
    ) -> Option<Message> {
        if let Some(sent) = self.sent.get(&nonce) {
            if sent.channel != channel || sent.upload_digest.is_some() {
                return None;
            }
            return self.message(channel, sent.id);
        }
        let message = self.post(channel, ME, content, reply_to, timestamp)?;
        self.sent.insert(
            nonce,
            SentMessage {
                channel,
                id: message.id,
                upload_digest: None,
            },
        );
        Some(message)
    }

    /// The voice channel the account is in.
    fn my_call(&self) -> Option<ChannelId> {
        self.voice
            .iter()
            .find(|state| state.user == ME)
            .map(|state| state.channel)
    }

    /// A made-up measure of the call's connection: plausible, not measured.
    fn quality(&mut self) -> CallQuality {
        CallQuality {
            rtt_ms: self.rng.range(28, 64) as u32,
            loss_permille: self.rng.range(0, 12) as u16,
        }
    }

    /// Write a new message in `channel`.
    fn post(
        &mut self,
        channel: ChannelId,
        author: UserId,
        content: String,
        reply_to: Option<MessageId>,
        timestamp: Timestamp,
    ) -> Option<Message> {
        let mentions = self
            .users
            .iter()
            .filter(|user| content.contains(&format!("@{}", user.name)))
            .map(|user| user.id)
            .collect();
        let text_references = message_actions::references(&content);
        let (mention_roles, mention_everyone) = self.told(channel, author, &content);
        let sim = self.sims.get_mut(&channel)?;
        let index = sim.generated + sim.extra.len() as u64;
        let message = Message {
            id: message_id(channel, index),
            channel,
            author,
            content,
            timestamp,
            edited: None,
            kind: MessageKind::Default,
            reply_to,
            mentions,
            reactions: Vec::new(),
            attachments: Vec::new(),
            stickers: Vec::new(),
            text_references,
            delivery: Delivery::Sent,
            pinned: false,
            embeds: Vec::new(),
            forwarded: None,
            poll: None,
            components: Default::default(),
            call: None,
            mention_roles,
            mention_everyone,
        };
        sim.extra.push(message.clone());
        self.count_reply(channel, timestamp);
        Some(message)
    }

    /// Change the account's own message `id` with `change`.
    fn edit(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        change: impl FnOnce(&mut Message),
    ) -> Result<Message, String> {
        let mut message = self.message(channel, id).ok_or(t("el mensaje no existe"))?;
        if message.author != ME {
            return Err(t("solo se pueden editar los mensajes propios").into());
        }
        change(&mut message);
        self.store(channel, message.clone());
        Ok(message)
    }

    /// Delete the message `id` of `channel`: the account's own, or anyone's
    /// in a server where it may manage messages.
    fn delete(&mut self, channel: ChannelId, id: MessageId) -> Result<(), String> {
        let message = self.message(channel, id).ok_or(t("el mensaje no existe"))?;
        let manages = self.channel(channel).is_some_and(|found| {
            found.server.is_some() && found.permissions.contains(Permissions::MANAGE_MESSAGES)
        });
        if message.author != ME && !manages {
            return Err(t("solo podés borrar tus mensajes").into());
        }
        let sim = self.sims.get_mut(&channel).ok_or(t("el canal no existe"))?;
        sim.overlay.insert(id, None);
        if let Some(position) = sim.extra.iter().position(|message| message.id == id) {
            sim.extra[position].content.clear();
        }
        Ok(())
    }

    fn react(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        emoji: &str,
        add: bool,
    ) -> Result<Message, String> {
        let mut message = self.message(channel, id).ok_or(t("el mensaje no existe"))?;
        message.set_reaction(emoji, add);
        self.store(channel, message.clone());
        Ok(message)
    }

    /// Someone else reacts to, or votes on, one of the latest messages of a
    /// text channel, as a live change.
    fn tally(&mut self, now: Timestamp) -> Option<Event> {
        if !self.live.chance(0.5) {
            return None;
        }
        let channels: Vec<ChannelId> = self
            .channels
            .iter()
            .filter(|channel| channel.kind == ChannelKind::Text)
            .map(|channel| channel.id)
            .collect();
        let channel = channels[self.live.below(channels.len() as u64) as usize];
        let total = self
            .sims
            .get(&channel)
            .map(|sim| sim.generated + sim.extra.len() as u64)
            .filter(|total| *total > 0)?;
        let index = total - 1 - self.live.below(total.min(8));
        let id = message_id(channel, index);
        let mut message = self.message(channel, id)?;
        let open = message.poll.as_ref().filter(|poll| !poll.ended(now));
        let event = if let Some(poll) = open {
            let answer = poll.answers[self.live.below(poll.answers.len() as u64) as usize].id;
            let poll = message.poll.as_mut().expect("an open poll");
            poll.count_vote(answer, false, true)
                .then_some(Event::PollVoteChanged {
                    channel,
                    message: id,
                    answer,
                    me: false,
                    added: true,
                })
        } else {
            let emoji = EMOJIS[self.live.below(EMOJIS.len() as u64) as usize];
            // Now and then a super reaction, with the colors a service gives it.
            if self.live.chance(0.15) {
                let colors = vec![Rgb(0xf9a62b), Rgb(0xe8433a)];
                message.count_burst(emoji, false, true, &colors).then(|| {
                    Event::BurstReactionChanged {
                        channel,
                        message: id,
                        emoji: emoji.into(),
                        me: false,
                        added: true,
                        colors,
                    }
                })
            } else {
                message
                    .count_reaction(emoji, false, true)
                    .then(|| Event::ReactionChanged {
                        channel,
                        message: id,
                        emoji: emoji.into(),
                        me: false,
                        added: true,
                    })
            }
        };
        if event.is_some() {
            self.store(channel, message);
        }
        event
    }

    fn vote(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        answers: &[u32],
        now: Timestamp,
    ) -> Result<Message, String> {
        let mut message = self.message(channel, id).ok_or(t("el mensaje no existe"))?;
        let poll = message
            .poll
            .as_mut()
            .ok_or(t("el mensaje no tiene encuesta"))?;
        if poll.ended(now) {
            return Err(t("la encuesta terminó").into());
        }
        if !poll.valid_choice(answers) {
            return Err(t("esas respuestas no valen para la encuesta").into());
        }
        poll.vote(answers);
        self.store(channel, message.clone());
        Ok(message)
    }

    /// The pinned messages of `channel`, the newest first.
    fn pins(&self, channel: ChannelId) -> Vec<Message> {
        let Some(sim) = self.sims.get(&channel) else {
            return Vec::new();
        };
        let name = self
            .channel(channel)
            .map_or("", |channel| channel.name.as_str());
        let mut ids: Vec<MessageId> = made_pins(self.seed, channel, name, sim.generated)
            .into_iter()
            .map(|index| message_id(channel, index))
            .chain(sim.overlay.keys().copied())
            .chain(sim.extra.iter().map(|message| message.id))
            .collect();
        ids.sort_unstable_by(|a, b| b.cmp(a));
        ids.dedup();
        ids.into_iter()
            .filter_map(|id| self.message(channel, id))
            .filter(|message| message.pinned)
            .collect()
    }

    /// Pin or unpin the message `id` of `channel`: in a server, only with
    /// permission to pin and to read the history, as the official client
    /// asks; in a direct conversation, anyone.
    fn pin(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        pinned: bool,
    ) -> Result<Vec<Event>, String> {
        let found = self.channel(channel).ok_or(t("el canal no existe"))?;
        if found.server.is_some()
            && !found
                .permissions
                .contains(Permissions::PIN_MESSAGES | Permissions::READ_HISTORY)
        {
            return Err(t("no tenés permiso para fijar mensajes").into());
        }
        let mut message = self.message(channel, id).ok_or(t("el mensaje no existe"))?;
        if pinned && !message.pinned && self.pins(channel).len() >= PINS_MAX {
            return Err(tr!(
                "un canal admite hasta {max} mensajes fijados",
                max = PINS_MAX
            ));
        }
        message.pinned = pinned;
        self.store(channel, message.clone());
        Ok(vec![Event::MessageUpdated(message)])
    }

    fn store(&mut self, channel: ChannelId, message: Message) {
        let Some(sim) = self.sims.get_mut(&channel) else {
            return;
        };
        match sim.extra.iter_mut().find(|extra| extra.id == message.id) {
            Some(extra) => *extra = message,
            None => {
                sim.overlay.insert(message.id, Some(message));
            }
        }
    }

    fn join_voice(&mut self, channel: ChannelId, muted: bool, deafened: bool) -> Vec<Event> {
        let mut events = self.leave_voice();
        let mut state = VoiceState {
            user: ME,
            channel,
            muted: muted || deafened,
            deafened,
            speaking: false,
            streaming: false,
            server_muted: false,
            server_deafened: false,
            camera: false,
            suppressed: false,
            speak_request: None,
        };
        self.seat_on_join(&mut state);
        self.voice.push(state.clone());
        events.push(Event::VoiceChanged(state));
        let quality = Some(self.quality());
        // Alone, it waits for the others (voz-protocolo).
        let phase = self.call_phase(channel);
        self.call_shown(channel, &phase);
        events.push(Event::CallPhase {
            channel,
            phase,
            quality,
        });
        // Desktop, language and local voice (P6).
        events.push(Event::CallPrivacyCode {
            channel,
            code: Some(desktop::privacy_code(channel)),
        });
        events
    }

    /// Someone streaming in the account's call: their stream goes live.
    fn watch_stream(&mut self, channel: ChannelId, owner: UserId) -> Vec<Event> {
        let here = |user: UserId| {
            self.voice
                .iter()
                .any(|state| state.user == user && state.channel == channel)
        };
        let phase = if here(ME)
            && self
                .voice
                .iter()
                .any(|s| s.user == owner && s.streaming && s.channel == channel)
        {
            StreamPhase::Live
        } else {
            StreamPhase::Ended(Some(
                t("Esa persona no está transmitiendo en tu llamada.").into(),
            ))
        };
        vec![Event::StreamPhase {
            channel,
            owner,
            phase,
        }]
    }

    /// The account's screen, «shared» in its call in `channel` or not.
    fn share(&mut self, channel: ChannelId, on: bool) -> Vec<Event> {
        let Some(state) = self
            .voice
            .iter_mut()
            .find(|state| state.user == ME && state.channel == channel)
        else {
            return (on)
                .then(|| Event::StreamPhase {
                    channel,
                    owner: ME,
                    phase: StreamPhase::Ended(Some(
                        t("Entrá a la llamada para compartir la pantalla.").into(),
                    )),
                })
                .into_iter()
                .collect();
        };
        state.streaming = on;
        let mut events = vec![Event::VoiceChanged(state.clone())];
        if on {
            events.push(Event::StreamPhase {
                channel,
                owner: ME,
                phase: StreamPhase::Live,
            });
        }
        events
    }

    /// The account's «shared» screen paused or resumed, while shared.
    fn pause_share(&mut self, channel: ChannelId, paused: bool) -> Vec<Event> {
        let sharing = self
            .voice
            .iter()
            .any(|state| state.user == ME && state.channel == channel && state.streaming);
        let phase = if paused {
            StreamPhase::Paused
        } else {
            StreamPhase::Live
        };
        sharing
            .then_some(Event::StreamPhase {
                channel,
                owner: ME,
                phase,
            })
            .into_iter()
            .collect()
    }

    /// The account's camera on or off in its call in `channel`.
    fn camera(&mut self, channel: ChannelId, on: bool) -> Vec<Event> {
        let Some(state) = self
            .voice
            .iter_mut()
            .find(|state| state.user == ME && state.channel == channel)
        else {
            return (on)
                .then(|| Event::Failed {
                    failure: Failure::Camera { channel },
                    reason: t("Entrá a la llamada para encender la cámara.").into(),
                    ambiguous: false,
                })
                .into_iter()
                .collect();
        };
        state.camera = on;
        vec![Event::VoiceChanged(state.clone())]
    }

    fn leave_voice(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let mut left = None;
        self.voice.retain(|state| {
            let mine = state.user == ME;
            if mine {
                left = Some(state.channel);
                events.push(Event::VoiceLeft {
                    user: ME,
                    channel: state.channel,
                });
                events.push(Event::CallEnded {
                    channel: state.channel,
                    reason: None,
                });
            }
            !mine
        });
        // Private calls: with nobody left in it, a conversation's call is over.
        if let Some(channel) = left {
            events.extend(self.call_over_if_empty(channel));
        }
        events
    }

    fn voice_self(&mut self, muted: bool, deafened: bool) -> Option<VoiceState> {
        let state = self.voice.iter_mut().find(|state| state.user == ME)?;
        state.muted = muted || deafened;
        state.deafened = deafened;
        Some(state.clone())
    }
}

/// What some of the made-up people say about themselves.
const ABOUT: [&str; 3] = [
    "Aprendiendo Rust de a poco.",
    "Hago interfaces nativas y café.",
    "Fotos, música y algún que otro borrow checker.",
];

fn message_id(channel: ChannelId, index: u64) -> MessageId {
    MessageId(channel.0 * MESSAGES_PER_CHANNEL + index)
}

/// The made-up messages of a channel that start pinned: the newest
/// announcements, a few in the busier channels, none elsewhere.
fn made_pins(seed: u64, channel: ChannelId, name: &str, generated: u64) -> Vec<u64> {
    match name {
        "anuncios" => [1, 3]
            .into_iter()
            .filter_map(|back| generated.checked_sub(back))
            .collect(),
        "general" | "ayuda" | "limn-core" | "componentes" | "charla" | "galería" => {
            let mut rng = mix(&[seed, channel.0, 0x50_49_4E]);
            (0..2 + rng.below(2))
                .map(|_| rng.below(generated))
                .collect()
        }
        _ => Vec::new(),
    }
}

fn topic(channel: &str) -> Option<String> {
    let topic = match channel {
        "general" => "Charla general del taller. Sé amable.",
        "ayuda" => {
            "Preguntas de Rust <:engranaje:900002>: pegá el error completo y qué intentaste."
        }
        "anuncios" => "Solo lectura: novedades del taller.",
        "rendimiento" => {
            "Mediciones, perfiles y benchmarks. 5 000 mensajes para probar el timeline."
        }
        "galería" => "Compartí tus fotos.",
        _ => return None,
    };
    Some(topic.into())
}

#[cfg(test)]
mod tests {
    mod read_state;
    mod search;
    mod uploads;
    // Private calls.
    mod calls;

    use super::*;
    use rune_model::CallPhase;

    fn config() -> MockConfig {
        MockConfig {
            seed: 7,
            history_end: Timestamp(1_790_000_000_000),
            clock: || Timestamp(1_790_000_100_000),
            latency: Duration::from_millis(50),
            ambient: false,
            replies: false,
        }
    }

    fn general(world: &World) -> ChannelId {
        world
            .channels
            .iter()
            .find(|channel| channel.name == "general")
            .unwrap()
            .id
    }

    #[test]
    fn read_markers_are_opt_in_and_the_simulation_accepts_them() {
        assert!(!Capabilities::default().read_state);
        assert!(MockProvider::new(config()).info().capabilities.read_state);
    }

    #[test]
    fn the_same_seed_makes_the_same_world() {
        let (one, two) = (World::build(&config()), World::build(&config()));
        let channel = general(&one);
        assert_eq!(one.snapshot(), two.snapshot());
        assert_eq!(
            one.history(channel, None, 50),
            two.history(channel, None, 50)
        );
        let other = World::build(&MockConfig {
            seed: 8,
            ..config()
        });
        assert_ne!(
            one.history(channel, None, 50),
            other.history(channel, None, 50)
        );
    }

    #[test]
    fn history_pages_go_back_in_order_to_the_first_message() {
        let world = World::build(&config());
        let channel = general(&world);
        let mut before = None;
        let mut seen = Vec::new();
        loop {
            let (page, reached_start) = world.history(channel, before, 100);
            assert!(page
                .windows(2)
                .all(|pair| pair[0].id < pair[1].id && pair[0].timestamp < pair[1].timestamp));
            before = page.first().map(|message| message.id);
            seen.splice(0..0, page);
            if reached_start {
                break;
            }
        }
        assert_eq!(seen.len(), 900);
        assert!(seen.windows(2).all(|pair| pair[0].id < pair[1].id));
        assert!(seen.last().unwrap().timestamp <= config().history_end.plus_minutes(1));
    }

    #[test]
    fn people_write_in_sittings_so_messages_group() {
        let world = World::build(&config());
        let (page, _) = world.history(general(&world), None, 300);
        let grouped = page
            .windows(2)
            .filter(|pair| {
                pair[0].author == pair[1].author
                    && pair[1].timestamp.minutes_since(pair[0].timestamp) < 7
            })
            .count();
        assert!(grouped > 60, "{grouped} of 300 continue a group");
    }

    #[test]
    fn edits_and_deletions_are_kept_and_only_for_own_messages() {
        let mut world = World::build(&config());
        let channel = general(&world);
        let mine = world
            .post(channel, ME, "hola".into(), None, Timestamp(5))
            .unwrap();
        let edited = world
            .edit(channel, mine.id, |message| message.content = "chau".into())
            .unwrap();
        assert_eq!(world.message(channel, mine.id), Some(edited));
        let (page, _) = world.history(channel, Some(mine.id), 20);
        let theirs = page.iter().find(|message| message.author != ME).unwrap();
        assert!(world.edit(channel, theirs.id, |_| {}).is_err());
        let reacted = world.react(channel, theirs.id, "🦀", true).unwrap();
        assert!(reacted
            .reactions
            .iter()
            .any(|reaction| reaction.emoji == "🦀" && reaction.me));
        world.delete(channel, mine.id).unwrap();
        assert_eq!(world.message(channel, mine.id), None);
        assert!(world.delete(channel, theirs.id).is_err(), "a plain member");
        // Where the account manages messages, it deletes anyone's.
        let core = world
            .channels
            .iter()
            .find(|channel| channel.name == "limn-core")
            .unwrap()
            .id;
        let (page, _) = world.history(core, None, 20);
        let theirs = page.iter().find(|message| message.author != ME).unwrap();
        world.delete(core, theirs.id).unwrap();
        assert_eq!(world.message(core, theirs.id), None);
    }

    #[test]
    fn the_simulated_poll_takes_valid_votes_while_open() {
        let mut world = World::build(&config());
        let channel = general(&world);
        let generated = world.sims[&channel].generated;
        let id = message_id(channel, generated - 6);
        let poll = world.message(channel, id).unwrap().poll.unwrap();
        assert!(poll.results_known && !poll.voted());
        let open = world.history_end;
        assert!(
            world.vote(channel, id, &[1, 2], open).is_err(),
            "single choice"
        );
        assert!(world.vote(channel, id, &[9], open).is_err());
        let voted = world.vote(channel, id, &[2], open).unwrap().poll.unwrap();
        assert!(voted.answers[1].me);
        assert_eq!(voted.answers[1].votes, poll.answers[1].votes + 1);
        assert_eq!(world.message(channel, id).unwrap().poll, Some(voted));
        assert!(!world
            .vote(channel, id, &[], open)
            .unwrap()
            .poll
            .unwrap()
            .voted());
        let late = open.plus_minutes(49 * 60);
        assert!(world.vote(channel, id, &[1], late).is_err(), "ended");
        let plain = message_id(channel, generated - 7);
        assert!(world.vote(channel, plain, &[1], open).is_err(), "no poll");
    }

    #[test]
    fn general_has_one_simulated_voice_message_labelled_as_such() {
        let world = World::build(&config());
        let channel = general(&world);
        let generated = world.sims[&channel].generated;
        let voiced: Vec<_> = (0..generated)
            .filter_map(|index| world.message(channel, message_id(channel, index)))
            .filter(|message| message.is_voice())
            .collect();
        assert_eq!(voiced.len(), 1);
        assert!(voiced[0].content.contains("simulado"));
        let file = &voiced[0].attachments[0];
        assert!(file.download.is_none(), "nothing to fetch from a network");
        assert_eq!(file.voice.as_ref().unwrap().duration_secs, 4.0);
    }

    #[test]
    fn simulated_live_tallies_keep_the_world_in_step_and_leave_history_alone() {
        let mut world = World::build(&config());
        let before = World::build(&config());
        let (mut seen, mut bursts) = (0, 0);
        for _ in 0..80 {
            let Some(event) = world.tally(world.history_end) else {
                continue;
            };
            seen += 1;
            bursts += usize::from(matches!(event, Event::BurstReactionChanged { .. }));
            let (channel, message) = match &event {
                Event::ReactionChanged {
                    channel,
                    message,
                    me,
                    ..
                }
                | Event::BurstReactionChanged {
                    channel,
                    message,
                    me,
                    ..
                }
                | Event::PollVoteChanged {
                    channel,
                    message,
                    me,
                    ..
                } => {
                    assert!(!me, "others only");
                    (*channel, *message)
                }
                other => panic!("{other:?}"),
            };
            let kept = world.message(channel, message).unwrap();
            let original = before.message(channel, message).unwrap();
            assert_ne!(kept, original, "the change is kept for later loads");
        }
        assert!(seen > 0 && bursts > 0, "{seen} changes, {bursts} super");
        assert_eq!(world.rng.0, before.rng.0, "other draws are untouched");
    }

    /// Whether `event` is one of those the account's settings follow Ready with.
    pub(super) fn account_setting(event: &Event) -> bool {
        matches!(
            event,
            Event::NotificationSettings { .. }
                | Event::ServerFolders(_)
                | Event::MessagingPrivacy(_)
                | Event::ActivitySharing(_)
        )
    }

    /// A session of `provider` running on the test's runtime.
    struct Session {
        events: mpsc::Receiver<crate::Envelope>,
        latest: crate::LatestValues,
        commands: mpsc::Sender<Command>,
        task: tokio::task::JoinHandle<Result<(), ProviderError>>,
        snapshot: Snapshot,
    }

    impl Session {
        async fn start(provider: MockProvider) -> Self {
            let (events_tx, mut events) = mpsc::channel(256);
            let latest = crate::LatestValues::default();
            let sink = EventSink::new(events_tx, latest.clone(), Arc::new(|| {})).for_session(3);
            let (commands, mut receiver) = mpsc::channel(16);
            let task = tokio::spawn(async move { provider.connect(&mut receiver, &sink).await });
            let first = events.recv().await.unwrap();
            assert_eq!(first.generation, 3, "events carry their session");
            let Event::Ready(snapshot) = first.event else {
                panic!("ready first")
            };
            // Then what the account keeps: notifications, folders, privacy
            // and activity sharing.
            for _ in 0..4 {
                let event = events.recv().await.unwrap().event;
                assert!(account_setting(&event), "{event:?}");
            }
            // And what its plan allows (P5).
            let perks = events.recv().await.unwrap().event;
            assert!(matches!(perks, Event::Perks(_)), "{perks:?}");
            // And the app someone uses in voice.
            let activity = events.recv().await.unwrap().event;
            assert!(
                matches!(&activity, Event::VoiceActivity(found) if !found.people.is_empty()),
                "{activity:?}"
            );
            // And since when each server's voice rooms are on (voz-protocolo).
            for _ in &snapshot.servers {
                let started = events.recv().await.unwrap().event;
                assert!(
                    matches!(started, Event::VoiceStartTimes { whole: true, .. }),
                    "{started:?}"
                );
            }
            Self {
                events,
                latest,
                commands,
                task,
                snapshot: *snapshot,
            }
        }

        fn channel(&self, name: &str) -> ChannelId {
            self.snapshot
                .channels
                .iter()
                .find(|channel| channel.name == name)
                .unwrap()
                .id
        }

        async fn send(&self, command: Command) {
            self.commands.send(command).await.unwrap();
        }

        async fn next(&mut self) -> Event {
            self.events.recv().await.unwrap().event
        }
    }

    fn send(channel: ChannelId, content: &str, nonce: u64) -> Command {
        Command::SendMessage {
            channel,
            content: content.into(),
            reply_to: None,
            nonce,
            mention_reply: false,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_session_answers_history_confirms_sends_and_cancels() {
        let mut session = Session::start(MockProvider::new(config())).await;
        let channel = session.channel("general");
        let read = session
            .snapshot
            .read_states
            .iter()
            .find(|state| state.0 == channel);
        assert_eq!(read.map(|state| state.1.is_some()), Some(true));
        session
            .send(Command::LoadHistory {
                request: 1,
                channel,
                before: None,
                limit: 50,
            })
            .await;
        session
            .send(Command::LoadHistory {
                request: 2,
                channel,
                before: None,
                limit: 50,
            })
            .await;
        session.send(Command::Cancel(2)).await;
        session.send(send(channel, "hola @ada", 99)).await;
        let seen = [session.next().await, session.next().await];
        assert!(seen.iter().any(|event| matches!(event, Event::HistoryLoaded { request: 1, messages, .. } if messages.len() == 50)));
        assert!(seen.iter().any(|event| matches!(event, Event::MessageCreated { nonce: Some(99), message } if message.mentions.len() == 1)));
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(
            session.events.try_recv().is_err(),
            "the cancelled request never answers"
        );
        drop(session.commands);
        assert_eq!(session.task.await.unwrap(), Ok(()));
    }

    #[tokio::test(start_paused = true)]
    async fn a_nonce_sent_again_returns_the_same_message() {
        let mut session = Session::start(MockProvider::new(config())).await;
        let channel = session.channel("general");
        session.send(send(channel, "una vez", 7)).await;
        let Event::MessageCreated { message: first, .. } = session.next().await else {
            panic!()
        };
        session.send(send(channel, "una vez", 7)).await;
        let Event::MessageCreated {
            message: again,
            nonce,
        } = session.next().await
        else {
            panic!()
        };
        assert_eq!(
            (again.id, nonce),
            (first.id, Some(7)),
            "idempotent, as the provider declares"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_rejected_send_names_its_nonce_and_an_ignored_one_never_answers() {
        let provider = MockProvider::new(config());
        let control = provider.control();
        let mut session = Session::start(provider).await;
        let channel = session.channel("general");
        control.fail_next_send();
        session.send(send(channel, "no llega", 8)).await;
        assert!(matches!(
            session.next().await,
            Event::Failed {
                failure: Failure::Send { nonce: 8, .. },
                ambiguous: false,
                ..
            }
        ));
        control.ignore_next_send();
        session.send(send(channel, "silencio", 9)).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(
            session.events.try_recv().is_err(),
            "nothing comes back for an ignored send"
        );
    }

    // Desktop, language and local voice (P6).
    #[tokio::test(start_paused = true)]
    async fn the_call_says_its_code_and_the_local_activity_shows_as_the_account_s() {
        let mut session = Session::start(MockProvider::new(config())).await;
        let channel = session.channel("Sala común");
        session
            .send(Command::JoinVoice {
                channel,
                muted: false,
                deafened: false,
            })
            .await;
        let code = loop {
            if let Event::CallPrivacyCode { code, .. } = session.next().await {
                break code;
            }
        };
        assert_eq!(code, Some(desktop::privacy_code(channel)));
        // Nobody of that ID is in the call: there is nothing to compare.
        session
            .send(Command::VoiceVerificationCode {
                channel,
                user: UserId(999_999),
            })
            .await;
        session
            .send(Command::SetLocalActivity(Some(rune_model::LocalActivity {
                application: Some(1_001),
                details: Some("Apertura".into()),
                ..Default::default()
            })))
            .await;
        session.send(Command::LoadDetectableGames).await;
        let (mut verified, mut shown, mut games) = (None, None, None);
        while verified.is_none() || shown.is_none() || games.is_none() {
            match session.next().await {
                Event::VoiceVerification { code, .. } => verified = Some(code),
                Event::PresenceChanged { user: ME, presence } => {
                    shown = presence.activities.into_iter().next()
                }
                Event::DetectableGames(list) => games = Some(list),
                _ => {}
            }
        }
        assert_eq!(verified, Some(None));
        let shown = shown.unwrap();
        assert_eq!(shown.name, "Aplicación 1001 (simulada)");
        assert_eq!(shown.detail.as_deref(), Some("Apertura"));
        assert_eq!(games, Some(desktop::games()));
    }

    #[tokio::test(start_paused = true)]
    async fn joining_a_voice_channel_connects_the_call_with_the_account_muted_as_asked() {
        let mut session = Session::start(MockProvider::new(config())).await;
        let channel = session.channel("Sala común");
        session
            .send(Command::JoinVoice {
                channel,
                muted: true,
                deafened: false,
            })
            .await;
        assert!(
            matches!(session.next().await, Event::VoiceChanged(state) if state.user == ME && state.muted)
        );
        assert!(matches!(
            session.next().await,
            Event::CallPhase {
                phase: CallPhase::Connected,
                quality: Some(_),
                ..
            }
        ));
        // Who speaks arrives as a latest value, never lost.
        tokio::time::sleep(Duration::from_secs(10)).await;
        let speaking = session
            .latest
            .lock()
            .unwrap()
            .keys()
            .any(|key| matches!(key, Latest::Speaking(_)));
        assert!(speaking);
        session.send(Command::LeaveVoice).await;
        loop {
            if let Event::CallEnded {
                channel: ended,
                reason: None,
            } = session.next().await
            {
                assert_eq!(ended, channel);
                break;
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_simulated_stream_goes_live_only_for_someone_streaming_in_the_call() {
        let mut session = Session::start(MockProvider::new(config())).await;
        let channel = session.channel("Pair programming");
        let in_room = |streaming: bool| {
            session
                .snapshot
                .voice
                .iter()
                .find(|state| state.channel == channel && state.streaming == streaming)
                .unwrap()
                .user
        };
        let (streamer, quiet) = (in_room(true), in_room(false));
        session
            .send(Command::JoinVoice {
                channel,
                muted: true,
                deafened: false,
            })
            .await;
        for owner in [streamer, quiet] {
            session.send(Command::WatchStream { channel, owner }).await;
        }
        let mut phases = Vec::new();
        while phases.len() < 2 {
            if let Event::StreamPhase { owner, phase, .. } = session.next().await {
                phases.push((owner, phase));
            }
        }
        assert_eq!(phases[0], (streamer, StreamPhase::Live));
        assert!(matches!(phases[1], (owner, StreamPhase::Ended(Some(_))) if owner == quiet));
    }

    #[tokio::test(start_paused = true)]
    async fn a_simulated_stream_has_no_picture_to_show_and_a_quiet_person_no_answer() {
        let mut session = Session::start(MockProvider::new(config())).await;
        let channel = session.channel("Pair programming");
        let server = session
            .snapshot
            .channels
            .iter()
            .find(|known| known.id == channel)
            .and_then(|known| known.server)
            .unwrap();
        let in_room = |streaming: bool| {
            session
                .snapshot
                .voice
                .iter()
                .find(|state| state.channel == channel && state.streaming == streaming)
                .unwrap()
                .user
        };
        let (streamer, quiet) = (in_room(true), in_room(false));
        for owner in [quiet, streamer] {
            session
                .send(Command::LoadStreamPreview {
                    server,
                    channel,
                    owner,
                })
                .await;
        }
        let answer = loop {
            if let event @ Event::StreamPreview { .. } = session.next().await {
                break event;
            }
        };
        assert_eq!(
            answer,
            Event::StreamPreview {
                channel,
                owner: streamer,
                image: None,
                retry_after: None,
            }
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_simulated_share_marks_the_account_streaming_until_stopped() {
        let provider = MockProvider::new(config());
        let mut session = Session::start(provider).await;
        let channel = session
            .snapshot
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Voice)
            .unwrap()
            .id;
        let quality = rune_model::StreamQuality::default();
        // Outside the call it ends at once, with why.
        session
            .send(Command::StartStream { channel, quality })
            .await;
        assert!(matches!(
            session.next().await,
            Event::StreamPhase {
                owner: ME,
                phase: StreamPhase::Ended(Some(_)),
                ..
            }
        ));
        session
            .send(Command::JoinVoice {
                channel,
                muted: false,
                deafened: false,
            })
            .await;
        session
            .send(Command::StartStream { channel, quality })
            .await;
        let mut live = false;
        while !live {
            match session.next().await {
                Event::VoiceChanged(state) if state.user == ME && state.streaming => {}
                Event::StreamPhase {
                    owner: ME, phase, ..
                } => {
                    assert_eq!(phase, StreamPhase::Live);
                    live = true;
                }
                _ => {}
            }
        }
        // Paused and resumed, as the phase says.
        for paused in [true, false] {
            session.send(Command::PauseStream { channel, paused }).await;
            let phase = loop {
                if let Event::StreamPhase {
                    owner: ME, phase, ..
                } = session.next().await
                {
                    break phase;
                }
            };
            let expected = if paused {
                StreamPhase::Paused
            } else {
                StreamPhase::Live
            };
            assert_eq!(phase, expected);
        }
        session.send(Command::StopStream { channel }).await;
        loop {
            if let Event::VoiceChanged(state) = session.next().await {
                if state.user == ME {
                    assert!(!state.streaming);
                    break;
                }
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_simulated_camera_marks_the_account_s_voice_state_until_off() {
        let provider = MockProvider::new(config());
        let mut session = Session::start(provider).await;
        let channel = session
            .snapshot
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Voice)
            .unwrap()
            .id;
        // Outside the call it fails, with why.
        session.send(Command::SetCamera { channel, on: true }).await;
        assert!(matches!(
            session.next().await,
            Event::Failed {
                failure: Failure::Camera { .. },
                ..
            }
        ));
        session
            .send(Command::JoinVoice {
                channel,
                muted: false,
                deafened: false,
            })
            .await;
        for on in [true, false] {
            session.send(Command::SetCamera { channel, on }).await;
            loop {
                if let Event::VoiceChanged(state) = session.next().await {
                    if state.user == ME && state.camera == on {
                        break;
                    }
                }
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_dropped_connection_ends_the_session_as_transient() {
        let provider = MockProvider::new(config());
        let control = provider.control();
        let session = Session::start(provider).await;
        control.drop_connection();
        assert!(matches!(
            session.task.await.unwrap(),
            Err(ProviderError::Transient(_))
        ));
    }

    #[test]
    fn some_people_are_doing_something() {
        let world = World::build(&config());
        assert!(
            world
                .presences
                .values()
                .filter(|presence| presence.activity().is_some())
                .count()
                >= 3
        );
    }

    #[test]
    fn context_requires_an_existing_anchor_and_is_bounded_and_ordered() {
        let world = World::build(&config());
        let channel = world
            .channels
            .iter()
            .find(|channel| channel.name == "general")
            .unwrap()
            .id;
        let latest = world.history(channel, None, 1).0.pop().unwrap();
        for limit in [0, 1, 51, usize::MAX] {
            let messages = world.context(channel, latest.id, limit).unwrap();
            assert!(messages.len() <= limit.clamp(1, 100));
            assert!(messages.iter().any(|message| message.id == latest.id));
            assert!(messages.windows(2).all(|pair| pair[0].id < pair[1].id));
        }
        assert!(world.context(channel, MessageId(u64::MAX), 51).is_none());
        assert!(world.context(ChannelId(u64::MAX), latest.id, 51).is_none());
    }
}
