//! The one place RUNE's data lives on the interface's side: what the provider
//! reported, applied event by event. Each event changes only what it names: a
//! new message touches one conversation's timeline and its unread count, not
//! the rest. Timelines hand out shared snapshots (`Arc`), so the interface
//! reads a conversation of thousands of messages without copying it, and a
//! change copies only the list of pointers it holds.
//!
//! What the account does is shown before the provider answers: a message
//! being sent, an edit, a reaction. The state keeps what each of those
//! changed, so a refusal puts it back, and a message that did not get there
//! says so and can be sent again or discarded.

mod abilities;
mod admin;
mod context;
mod conversations;
mod message_patches;
use context::ContextRead;
mod pins;
mod profile_edit;
mod profiles;
mod reads;
mod relationships;
mod remote;
mod servers;
mod stage;
mod threads;
mod timeline;
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
// Private calls.
mod private_calls;
// Rail and notifications.
mod rail;
pub use private_calls::{CallStatus, DECLINED_WITHIN};
// Message content (contenido-de-mensajes).
mod content_filter;
// Voice protocol (voz-protocolo).
mod voice_started;
// Rules and onboarding (2026-10-18).
mod onboarding;
pub use onboarding::JoinSteps;
// Scheduled events (eventos-programados).
mod events;
pub use events::EventAbilities;
// AutoMod (automod).
mod automod;

use std::cell::OnceCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rune_i18n::{es, t, tr};
use rune_model::{
    Call, CallPhase, Channel, ChannelId, ChannelKind, Delivery, Member, Message, MessageId,
    Presence, Role, Server, ServerId, StreamPhase, Timestamp, User, UserId, VoiceState,
};
use rune_provider::{
    AuthenticationState, ConnectionState, Created, Event, Failure, InvitationDestination,
    InvitationJoinError, ProviderInfo, RequestId, Snapshot,
};

pub use abilities::{Abilities, Permit, ServerAbilities, NO_STAGES, NO_VIDEO_SENDING};
pub use profile_edit::{ProfileAbilities, CREDENTIALS_DENIED};

pub use account::{AccountAbilities, RailEntry};
pub use admin::{MemberPages, Paged, AUDIT_MAX};
pub use conversations::{GroupAbilities, GROUP_OTHERS};
pub use interactions::{
    CommandsState, InteractionAbilities, Pending, Problem, COMMANDS_KEPT, INTERACTION_WAIT,
};
use pins::{PinnedMessages, PinsRead};
pub use reads::ReadState;
pub use relationships::SocialAbilities;
pub use stage::StageAbilities;
// Desktop, language and local voice (P6).
pub use desktop::Verification;
pub use threads::{ThreadAbilities, ThreadPages};
pub use timeline::Timeline;
// Message actions and moderation (P4).
pub use message_actions::VoiceModerationAbilities;

/// How long someone shows as typing after the last sign of it.
pub const TYPING_SHOWN: Duration = Duration::from_secs(8);
/// People shown typing in one conversation at most.
const TYPING_SLOTS: usize = 8;

/// A group of the member list: a heading and its people, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct MemberGroup {
    pub title: String,
    pub color: Option<rune_model::Rgb>,
    pub members: Vec<UserId>,
}

/// A message of the account that did not get there, as retrying needs it.
/// Who is in voice in one server: see [`Store::voice_by_server`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerVoice<'a> {
    pub people: Vec<&'a VoiceState>,
    /// The account's own call is in this server.
    pub own: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Unsent {
    pub nonce: u64,
    pub content: String,
    pub reply_to: Option<MessageId>,
    pub sticker: Option<rune_model::StickerId>,
    pub files: Vec<rune_model::PendingAttachment>,
    pub upload_request: Option<RequestId>,
    /// It may have arrived: retry only with the same nonce, and only with a
    /// provider whose sends are idempotent.
    pub ambiguous: bool,
    // Message actions (P4).
    /// A reply that notifies who it answers.
    pub mention_reply: bool,
}

/// What an event changed, for the interface to answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Applied {
    Nothing,
    Authentication,
    /// The whole state was replaced by a new session.
    Ready,
    Connection,
    /// An observational acknowledgement: server, channel and relationship
    /// events grant access.
    InvitationAcceptance {
        request: RequestId,
        destination: InvitationDestination,
        result: Result<(), InvitationJoinError>,
    },
    /// Messages of a conversation changed; `added` were new at its end.
    Messages {
        channel: ChannelId,
        added: usize,
    },
    /// A page of history arrived.
    History {
        channel: ChannelId,
        request: RequestId,
        count: usize,
    },
    Typing(ChannelId),
    Presence(UserId),
    Voice(ChannelId),
    /// The account's call changed.
    Call,
    /// The account's call ended, with the reason when it was not the account's doing.
    CallEnded {
        reason: Option<String>,
    },
    /// A command failed; what it changed here was undone or marked.
    Failed {
        failure: Failure,
        reason: String,
        ambiguous: bool,
    },
    /// A server is new or changed: its name, roles, profile.
    Server(ServerId),
    /// The account is no longer in a server; `reason` when it was not its doing.
    ServerRemoved {
        server: ServerId,
        reason: Option<String>,
    },
    /// A channel is new or changed.
    Channel(ChannelId),
    ChannelRemoved(ChannelId),
    /// Someone of a server changed, joined or left.
    Members(ServerId),
    /// Someone's account changed: name, profile.
    User(UserId),
    /// Someone's profile, asked for, arrived.
    Profile(UserId),
    /// A server's bans arrived.
    Bans(ServerId),
    /// A page of a server's audit log arrived.
    Audit(ServerId),
    /// A server's invitations arrived or changed.
    Invites(ServerId),
    /// A channel's pinned messages arrived.
    Pins {
        channel: ChannelId,
        request: RequestId,
    },
    Context {
        channel: ChannelId,
        request: RequestId,
        anchor: MessageId,
    },
    ReadState(ChannelId),
    /// The account's relationship with someone changed or ended.
    Relationship(UserId),
    /// Something asked for with `request` was made.
    Created {
        request: RequestId,
        created: Created,
    },
    /// The account's own profile was saved, in `server` or across all.
    ProfileSaved {
        server: Option<ServerId>,
    },
    // Verification of writes.
    /// The action `failure` names waits for the person to solve `challenge`.
    Challenge {
        challenge: RequestId,
        failure: Failure,
    },
    // Administration (19).
    /// The invitation asked for with `request` was made: its `code`.
    InviteCreated {
        request: RequestId,
        server: ServerId,
        code: String,
    },
    /// A page of a server's members arrived.
    MemberPage(ServerId),
    /// A prune asked for with `request` was counted or done.
    Pruned {
        request: RequestId,
        server: ServerId,
        days: u32,
        count: Option<u32>,
        removed: bool,
    },
    /// A server's webhooks arrived or changed.
    Webhooks(ServerId),
    /// A server's integrations arrived or changed.
    Integrations(ServerId),
    /// The GIF favorites the provider keeps arrived or changed.
    GifFavorites,
    // The account's synced settings (P1).
    /// Notifications, folders or messaging privacy the account keeps changed.
    AccountSettings,
    /// The account's private note on someone arrived or changed.
    Note(UserId),
    // Reads, search and remote members (P2).
    /// Channels the provider only summarizes have something new to read.
    Activity,
    /// A page of a search at the provider, for whoever asked with `request`.
    SearchResults {
        request: RequestId,
        result: Result<rune_model::RemotePage, rune_model::RemoteSearchError>,
    },
    /// Members of `server` named like `query` were found and are known now.
    MembersFound {
        server: ServerId,
        query: String,
    },
    // Message actions and moderation (P4).
    /// A page of who reacted or voted, for whoever asked with `request`;
    /// the people are known now.
    People {
        request: RequestId,
        people: Vec<UserId>,
        reached_end: bool,
    },
    /// The conversation asked for with `request` is known now.
    ChannelLoaded {
        request: RequestId,
        channel: ChannelId,
    },
    // Desktop, language and local voice (P6).
    /// The games the provider recognizes arrived: see [`Store::detectable_games`].
    DetectableGames,
    // Presence, identity and expressions (P5).
    /// What the account may use beyond each server changed.
    Perks,
    /// The standard sticker packs arrived.
    StickerPacks,
    /// A page of the provider's GIFs, for whoever asked.
    GifResults(rune_provider::GifResult),
    /// What to search for GIFs, for whoever asked with `request`.
    GifSuggestions {
        request: RequestId,
        query: String,
        terms: Vec<String>,
    },
    // Server administration, rest (P7).
    /// How many members have each role of a server arrived.
    RoleCounts(ServerId),
    // Private calls.
    /// The call of a direct or group conversation started, rings someone
    /// else, or ended.
    PrivateCall(ChannelId),
    // Sends and uploads (envios-y-subidas).
    /// The files of the new post `request` went this far; only the dialog
    /// that asked for it shows it.
    PostProgress {
        request: RequestId,
        sent_bytes: u64,
        total_bytes: u64,
    },
    // Calls and the voice panel (llamadas-y-panel-de-voz).
    /// A direct call may be made, and ring if `ringable`.
    CallChecked {
        channel: ChannelId,
        ringable: bool,
    },
    // Rules and onboarding (2026-10-18).
    /// A server's rules arrived, asked for with `request`.
    ServerRules {
        request: RequestId,
        server: ServerId,
    },
    /// What came of accepting a server's rules.
    RulesAnswered {
        request: RequestId,
        server: ServerId,
        outcome: rune_model::JoinOutcome,
    },
    /// A server's onboarding arrived, asked for or saved with `request`.
    Onboarding {
        request: RequestId,
        server: ServerId,
    },
    /// The account finished a server's onboarding.
    OnboardingAnswered {
        request: RequestId,
        server: ServerId,
    },
    // Scheduled events (eventos-programados).
    /// Something of a server's events changed: what the account asked for
    /// with `request`, or what the service reported (`None`).
    ScheduledEvents {
        request: Option<RequestId>,
        server: ServerId,
        /// The event it was about, when it was about one.
        event: Option<rune_model::ScheduledEventId>,
    },
    // AutoMod (automod).
    /// A server's AutoMod rules arrived, asked for with `request`.
    AutoModRules {
        request: RequestId,
        server: ServerId,
    },
    /// Something of a server's AutoMod changed; `request` is the save that
    /// changed it, when it was asked for here.
    AutoMod {
        server: ServerId,
        request: Option<RequestId>,
    },
}

/// RUNE's data.
/// Stream pictures kept at once, for the column.
pub const MAX_STREAM_PREVIEWS: usize = 32;

/// Someone's stream the account watches, and how far it got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Watching {
    pub channel: ChannelId,
    pub owner: UserId,
    pub phase: StreamPhase,
}

/// The channels by id. Reads go through `Deref`; a change goes through the
/// methods below, which drop the index so the next read rebuilds it.
#[derive(Clone, Debug, Default)]
struct Channels {
    map: HashMap<ChannelId, Channel>,
    index: OnceCell<ChannelIndex>,
}

impl Channels {
    fn index(&self) -> &ChannelIndex {
        self.index.get_or_init(|| ChannelIndex::of(&self.map))
    }

    fn insert(&mut self, id: ChannelId, channel: Channel) -> Option<Channel> {
        self.index.take();
        self.map.insert(id, channel)
    }

    fn remove(&mut self, id: &ChannelId) -> Option<Channel> {
        self.index.take();
        self.map.remove(id)
    }

    fn get_mut(&mut self, id: &ChannelId) -> Option<&mut Channel> {
        self.index.take();
        self.map.get_mut(id)
    }
}

impl Deref for Channels {
    type Target = HashMap<ChannelId, Channel>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl From<HashMap<ChannelId, Channel>> for Channels {
    fn from(map: HashMap<ChannelId, Channel>) -> Self {
        Self {
            map,
            index: OnceCell::new(),
        }
    }
}

// Equal channels are equal whether or not the index was built yet.
impl PartialEq for Channels {
    fn eq(&self, other: &Self) -> bool {
        self.map == other.map
    }
}

/// The orders `channel_tree`, `threads` and `server_unread` read, sorted once
/// per change: sorting every channel on each call costs too much per frame.
#[derive(Clone, Debug, Default)]
struct ChannelIndex {
    /// Every channel of a server, threads too, in `(position, id)` order.
    of_server: HashMap<ServerId, Vec<ChannelId>>,
    /// The threads under each parent, the most recently active first.
    threads: HashMap<ChannelId, Vec<ChannelId>>,
}

impl ChannelIndex {
    fn of(channels: &HashMap<ChannelId, Channel>) -> Self {
        let mut by_position: Vec<&Channel> = channels.values().collect();
        by_position.sort_by_key(|channel| (channel.position, channel.id));
        let mut index = Self::default();
        for channel in by_position {
            if let Some(server) = channel.server {
                index.of_server.entry(server).or_default().push(channel.id);
            }
            if let (true, Some(parent)) = (channel.is_thread(), channel.parent) {
                index.threads.entry(parent).or_default().push(channel.id);
            }
        }
        for threads in index.threads.values_mut() {
            threads.sort_by_key(|id| {
                let last = channels[id].thread.as_ref().map(|t| t.last_activity);
                (std::cmp::Reverse(last), std::cmp::Reverse(*id))
            });
        }
        index
    }
}

#[derive(Default)]
pub struct Store {
    pub provider: Option<ProviderInfo>,
    /// Ephemeral authorization state; never saved in SQLite.
    pub authentication: AuthenticationState,
    /// Whether the provider remembers this session for the next start;
    /// `None` when it never said. Cleared with the account.
    pub remembered: Option<bool>,
    /// The accounts the provider remembers on this device, the most
    /// recently used first. Not an account's data: kept across sign-outs.
    pub accounts: Vec<rune_provider::RememberedAccount>,
    pub connection: ConnectionState,
    /// What is shown came from the local store, not from a session yet.
    pub from_cache: bool,
    me: Option<UserId>,
    users: HashMap<UserId, User>,
    servers: Vec<Server>,
    channels: Channels,
    members: HashMap<ServerId, Vec<Member>>,
    /// Where each person is in their server's `members`.
    member_index: HashMap<(ServerId, UserId), usize>,
    presences: HashMap<UserId, Presence>,
    timelines: HashMap<ChannelId, Timeline>,
    reads: HashMap<ChannelId, ReadState>,
    read_requests: u64,
    typing: HashMap<ChannelId, Vec<(UserId, Instant)>>,
    voice: Vec<VoiceState>,
    call: Option<Call>,
    call_connected: Option<Instant>,
    /// The call's phase as its provider last told it: shown again once the
    /// connection to the service is back, which says nothing of the call.
    call_reported: Option<CallPhase>,
    /// The streams the account watches in its call, the oldest first.
    watching: Vec<Watching>,
    /// The account's own screen shared in its call; its `owner` is the account.
    sharing: Option<Watching>,
    /// The latest picture of each stream, by its owner, and where: `None`
    /// when the provider had none, with how long it asked to wait. Kept
    /// while they stream there.
    stream_previews: HashMap<
        UserId,
        (
            ChannelId,
            Option<rune_model::RemoteImage>,
            Option<std::time::Duration>,
        ),
    >,
    /// Messages being sent, by nonce, and where they are shown meanwhile.
    pending: HashMap<u64, (ChannelId, MessageId)>,
    pending_files: HashMap<u64, uploads::PendingFiles>,
    // Message actions (P4).
    /// Replies sent from here that do not notify who they answer, by nonce.
    quiet_replies: std::collections::HashSet<u64>,
    /// Messages as they were before an edit or a reaction made here, until
    /// the provider confirms or refuses it.
    before_change: HashMap<(ChannelId, MessageId), Arc<Message>>,
    /// Bans, audit log and invitations of each server asked for.
    admin: HashMap<ServerId, admin::AdminLists>,
    /// Administration changes shown before the provider answers.
    ahead: HashMap<(ServerId, admin::Target), admin::Ahead>,
    /// The pinned messages of each channel asked for, the newest first.
    pins: HashMap<ChannelId, PinnedMessages>,
    pins_read: Option<PinsRead>,
    context_read: Option<ContextRead>,
    context_request: RequestId,
    /// `None` until the provider reports a whole list.
    relationships: Option<HashMap<UserId, rune_model::Relationship>>,
    revision: u64,
    /// The clock restrictions are read at; `None` reads the system's.
    pub clock: Option<fn() -> Timestamp>,
    /// The profile read in flight: its request, person and scope.
    profile_read: Option<(RequestId, UserId, Option<ServerId>)>,
    /// Profiles loaded lately, the newest last.
    profiles: Vec<rune_model::UserProfile>,
    /// The member list the provider keeps current, when it orders one.
    member_list: Option<Arc<rune_model::MemberList>>,
    // Attachments and platform (07/08).
    /// The largest file the account may attach, by server; `None` outside
    /// servers and for a server without its own value.
    upload_limits: HashMap<Option<ServerId>, u64>,
    /// What has been read of each channel's or forum's threads.
    thread_pages: HashMap<ChannelId, threads::ThreadPages>,
    // Administration (19).
    favorites: admin::Favorites,
    /// What the account keeps with the provider beside its profile (P1).
    account: account::AccountState,
    /// Apps' commands and interactions of this session (P3).
    interactions: interactions::InteractionState,
    /// The local activity, the call's codes and the games (P6).
    desktop: desktop::DesktopState,
    /// The account's perks and the standard sticker packs (P5).
    identity: identity::IdentityState,
    // Private calls.
    /// The calls of direct and group conversations, and how the account's went.
    private_calls: private_calls::PrivateCalls,
    // Rail and notifications.
    /// The apps launched in voice channels, by the provider's name of each.
    voice_activities: HashMap<String, rune_model::VoiceActivity>,
    // Voice protocol (voz-protocolo).
    /// Since when each voice channel has had someone in it, as told.
    voice_started: HashMap<ChannelId, Timestamp>,
    // Profile card and cosmetics (perfil-tarjeta-y-cosmeticos).
    /// When each loaded profile arrived, by the store's clock.
    profile_at: HashMap<(UserId, Option<ServerId>), Timestamp>,
    // Rules and onboarding (2026-10-18).
    join: onboarding::JoinState,
    // Scheduled events (eventos-programados).
    events: events::EventsState,
    // AutoMod (automod).
    automod: automod::AutoModState,
}

/// Where a message being sent with `nonce` is shown: after every real message,
/// and different for each nonce.
fn local_id(nonce: u64) -> MessageId {
    MessageId(u64::MAX - nonce)
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    /// Grows with every change: a view built for one revision is current
    /// until the next.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Apply one event of the provider. `now` dates typing and calls.
    pub fn apply(&mut self, event: Event, now: Instant) -> Applied {
        self.revision += 1;
        self.prune_pins();
        match &event {
            Event::MessageCreated { message, .. } | Event::MessageUpdated(message) => {
                self.dirty_context(message.channel)
            }
            Event::MessagePatched(patch) => self.dirty_context(patch.channel),
            Event::MessageDeleted { channel, .. } | Event::HistoryLoaded { channel, .. } => {
                self.dirty_context(*channel)
            }
            // A live reaction or vote changes a neighbor the window may hold.
            Event::ReactionChanged { channel, .. }
            | Event::ReactionsCleared { channel, .. }
            | Event::PollVoteChanged { channel, .. }
            | Event::BurstReactionChanged { channel, .. } => self.dirty_context(*channel),
            _ => {}
        }
        match event {
            Event::Authentication(state) => {
                if matches!(
                    state,
                    AuthenticationState::SignedOut | AuthenticationState::Failed { .. }
                ) {
                    // Ready intentionally keeps timelines during reconnection;
                    // an account boundary must discard all of that state.
                    let provider = self.provider.take();
                    let accounts = std::mem::take(&mut self.accounts);
                    let revision = self.revision;
                    let read_requests = self.read_requests;
                    let clock = self.clock;
                    *self = Self::new();
                    self.clock = clock;
                    self.provider = provider;
                    self.accounts = accounts;
                    self.revision = revision;
                    self.read_requests = read_requests;
                    self.connection = ConnectionState::Disconnected {
                        reason: match &state {
                            AuthenticationState::Failed { reason } => reason.clone(),
                            // A key, translated where shown: the interface tells it apart.
                            _ => es("Sesión cerrada").into(),
                        },
                    };
                }
                self.authentication = state;
                Applied::Authentication
            }
            Event::SessionRemembered(remembered) => {
                self.remembered = Some(remembered);
                Applied::Nothing
            }
            Event::RememberedAccounts(accounts) => {
                self.accounts = accounts;
                Applied::Nothing
            }
            Event::Ready(snapshot) => {
                self.ready(*snapshot);
                self.interactions_ready();
                self.from_cache = false;
                self.connection = ConnectionState::Connected;
                Applied::Ready
            }
            Event::Connection(state) => {
                if let Some(call) = self.call.as_mut() {
                    match &state {
                        ConnectionState::Reconnecting { attempt, .. } => {
                            call.phase = CallPhase::Reconnecting { attempt: *attempt };
                        }
                        ConnectionState::Disconnected { reason } => {
                            call.phase = CallPhase::Failed {
                                reason: reason.clone(),
                            };
                        }
                        // Back: the call is as its provider last said, not
                        // «reconnecting» until its next change.
                        ConnectionState::Connected => {
                            if let (CallPhase::Reconnecting { .. }, Some(reported)) =
                                (&call.phase, &self.call_reported)
                            {
                                call.phase = reported.clone();
                            }
                        }
                        _ => {}
                    }
                }
                self.connection = state;
                Applied::Connection
            }
            Event::InvitationAcceptance {
                request,
                destination,
                result,
            } => Applied::InvitationAcceptance {
                request,
                destination,
                result,
            },
            // Only the account sees it, and only now (P3).
            Event::MessageCreated { message, .. } if message.components.ephemeral => {
                self.ephemeral_created(message)
            }
            Event::MessageCreated { message, nonce } => self.created(message, nonce),
            Event::UploadProgress {
                scope,
                sent_bytes,
                total_bytes,
            } => self.upload_progress(scope, sent_bytes, total_bytes),
            Event::ReadStateConfirmed {
                account,
                request,
                channel,
                message,
                version,
            } => self.confirmed_read(account, request, channel, message, version),
            Event::MessageUpdated(message) => {
                let channel = message.channel;
                self.before_change.remove(&(channel, message.id));
                self.keep_pinned(&message);
                let timeline = self.timelines.entry(channel).or_default();
                if timeline.get(message.id).is_some() {
                    timeline.upsert(message);
                }
                Applied::Messages { channel, added: 0 }
            }
            Event::MessagePatched(patch) => self.patched(patch),
            Event::ReactionChanged {
                channel,
                message,
                emoji,
                me,
                added,
            } => self.tallied(channel, message, |message| {
                message.count_reaction(&emoji, me, added)
            }),
            Event::ReactionsCleared {
                channel,
                message,
                emoji,
            } => self.tallied(channel, message, |message| {
                message.clear_reactions(emoji.as_deref())
            }),
            // Reactions, pins and search (2026-10-18).
            Event::BurstReactionChanged {
                channel,
                message,
                emoji,
                me,
                added,
                colors,
            } => self.tallied(channel, message, |message| {
                message.count_burst(&emoji, me, added, &colors)
            }),
            Event::PollVoteChanged {
                channel,
                message,
                answer,
                me,
                added,
            } => self.tallied(channel, message, |message| {
                message
                    .poll
                    .as_mut()
                    .is_some_and(|poll| poll.count_vote(answer, me, added))
            }),
            Event::MessageDeleted { channel, message } => {
                if let Some(read) = self.reads.get_mut(&channel) {
                    if read.first_unread == Some(message) {
                        read.first_unread = None;
                    }
                }
                self.before_change.remove(&(channel, message));
                self.delete_pinned(channel, message);
                self.timelines.entry(channel).or_default().delete(message);
                Applied::Messages { channel, added: 0 }
            }
            Event::HistoryLoaded {
                request,
                channel,
                before,
                messages,
                reached_start,
            } => {
                let count = messages.len();
                let timeline = self.timelines.entry(channel).or_default();
                timeline.apply_page(messages, before, reached_start);
                if let Some(newest) = timeline
                    .messages()
                    .iter()
                    .rev()
                    .find(|message| !message.delivery.is_local())
                    .map(|message| message.id)
                {
                    let read = self.reads.entry(channel).or_default();
                    read.last_message = read.last_message.max(Some(newest));
                }
                Applied::History {
                    channel,
                    request,
                    count,
                }
            }
            Event::Typing { channel, user } => {
                let typing = self.typing.entry(channel).or_default();
                typing.retain(|(known, since)| {
                    *known != user && now.saturating_duration_since(*since) < TYPING_SHOWN
                });
                if typing.len() >= TYPING_SLOTS {
                    typing.remove(0);
                }
                typing.push((user, now));
                Applied::Typing(channel)
            }
            Event::PresenceChanged { user, presence } => {
                self.presences.insert(user, presence);
                Applied::Presence(user)
            }
            Event::VoiceChanged(state) => {
                let channel = state.channel;
                self.private_call_joined(state.user, channel);
                // A watched stream waits while its owner is not streaming, and
                // connects again when they do (the provider asks for it again).
                for watching in self
                    .watching
                    .iter_mut()
                    .filter(|watching| watching.owner == state.user && watching.channel == channel)
                {
                    let next = match (&watching.phase, state.streaming) {
                        (StreamPhase::Waiting, true) => Some(StreamPhase::Connecting),
                        (
                            StreamPhase::Connecting
                            | StreamPhase::Live
                            | StreamPhase::Paused
                            | StreamPhase::Reconnecting,
                            false,
                        ) => Some(StreamPhase::Waiting),
                        _ => None,
                    };
                    if let Some(next) = next {
                        watching.phase = next;
                    }
                }
                let before = match self.voice.iter_mut().find(|known| known.user == state.user) {
                    Some(known) => Some(std::mem::replace(known, state).channel),
                    None => {
                        self.voice.push(state);
                        None
                    }
                };
                // Voice protocol (voz-protocolo): a room left empty.
                if let Some(before) = before.filter(|before| *before != channel) {
                    self.voice_room_left(before);
                }
                Applied::Voice(channel)
            }
            Event::VoiceLeft { user, channel } => {
                self.voice.retain(|state| state.user != user);
                self.voice_room_left(channel);
                Applied::Voice(channel)
            }
            Event::Speaking {
                channel,
                user,
                speaking,
            } => {
                self.set_speaking(user, speaking);
                Applied::Voice(channel)
            }
            Event::CallPhase {
                channel,
                phase,
                quality,
            } => {
                // A call the provider reports that was not asked for here is
                // one that came back after reconnecting.
                let (muted, deafened) = self
                    .my_voice()
                    .map_or((false, false), |state| (state.muted, state.deafened));
                let call = self.call.get_or_insert(Call {
                    channel,
                    phase: CallPhase::Joining,
                    muted,
                    deafened,
                    quality: None,
                });
                if call.channel != channel {
                    return Applied::Nothing;
                }
                if phase.is_connected() && !call.phase.is_connected() {
                    self.call_connected = Some(now);
                }
                self.call_reported = Some(phase.clone());
                call.phase = phase;
                if quality.is_some() {
                    call.quality = quality;
                }
                Applied::Call
            }
            Event::StreamPhase {
                channel,
                owner,
                phase,
            } => {
                // An ended stream stays shown, with why, until put away.
                let slot = if Some(owner) == self.me {
                    self.sharing.as_mut()
                } else {
                    self.watching
                        .iter_mut()
                        .find(|watching| watching.owner == owner)
                };
                match slot {
                    Some(watching) if (watching.channel, watching.owner) == (channel, owner) => {
                        watching.phase = phase;
                        Applied::Call
                    }
                    _ => Applied::Nothing,
                }
            }
            Event::CallEnded { channel, reason } => {
                if self
                    .call
                    .as_ref()
                    .is_some_and(|call| call.channel == channel)
                {
                    self.call = None;
                    self.call_connected = None;
                    self.watching.clear();
                    self.sharing = None;
                    self.desktop_call_ended();
                }
                Applied::CallEnded { reason }
            }
            event @ (Event::ServerChanged(_)
            | Event::ServerRemoved { .. }
            | Event::ChannelChanged(_)
            | Event::ChannelRemoved { .. }
            | Event::MemberChanged(_)
            | Event::MemberRemoved { .. }
            | Event::UserChanged(_)
            | Event::BansLoaded { .. }
            | Event::BanAdded { .. }
            | Event::BanRemoved { .. }
            | Event::AuditLoaded { .. }
            | Event::InvitesLoaded { .. }
            | Event::InviteRemoved { .. }
            | Event::Created { .. }) => self.apply_management(event),
            // Administration (19).
            event @ (Event::InviteCreated { .. }
            | Event::MembersLoaded { .. }
            | Event::Pruned { .. }
            | Event::WebhooksLoaded { .. }
            | Event::WebhookChanged { .. }
            | Event::WebhookRemoved { .. }
            | Event::IntegrationsLoaded { .. }
            | Event::IntegrationRemoved { .. }
            | Event::GifFavorites(_)
            // Server administration, rest (P7).
            | Event::RoleCounts { .. }) => self.apply_admin(event),
            Event::PinsLoaded {
                request,
                channel,
                messages,
            } => self.loaded_pins(channel, request, messages),
            Event::RelationshipChanged(relationship) => self.relationship_changed(relationship),
            Event::RelationshipRemoved { user } => self.relationship_removed(user),
            Event::StreamPreview {
                channel,
                owner,
                image,
                retry_after,
            } => self.stream_preview_loaded(channel, owner, image, retry_after),
            Event::UploadLimit {
                server,
                max_file_bytes,
            } => {
                self.upload_limits.insert(server, max_file_bytes);
                Applied::Nothing
            }
            Event::ProfileLoaded { request, profile } => self.loaded_profile(request, profile),
            Event::ProfileChanged(profile) => self.profile_changed(profile),
            Event::MemberListChanged(list) => self.member_list_changed(list),
            Event::ContextLoaded {
                channel,
                request,
                anchor,
                messages,
            } => self.loaded_context(
                channel,
                request,
                anchor,
                messages,
                rune_model::HistorySource::Provider,
            ),
            Event::Failed {
                failure,
                reason,
                ambiguous,
            } => {
                if let Failure::SendFiles {
                    request,
                    channel,
                    nonce,
                } = &failure
                {
                    if !self.expects_upload(*channel, *nonce, *request) {
                        return Applied::Nothing;
                    }
                    if ambiguous
                        && self
                            .pending
                            .get(nonce)
                            .and_then(|(_, message)| self.timeline(*channel)?.get(*message))
                            .is_some_and(|message| {
                                matches!(message.delivery, Delivery::Failed { .. })
                            })
                    {
                        return Applied::Nothing;
                    }
                }
                if let Failure::Send { channel, nonce } = &failure {
                    if self.pending_files.contains_key(nonce)
                        || self
                            .pending
                            .get(nonce)
                            .is_none_or(|(found, _)| found != channel)
                    {
                        return Applied::Nothing;
                    }
                }
                if let Failure::Pins { channel, request } = &failure {
                    if !self.expects_pins(*channel, *request) {
                        return Applied::Nothing;
                    }
                }
                if let Failure::Context { channel, request } = &failure {
                    if !self.expects_context(*channel, *request) {
                        return Applied::Nothing;
                    }
                    self.cancel_context(*request);
                }
                if let Failure::ProfileLoad { request, .. } = &failure {
                    if !self.profile_failed(*request) {
                        return Applied::Nothing;
                    }
                }
                if let Failure::ReadState { channel, request } = &failure {
                    if !self.read_failed(*channel, *request) {
                        return Applied::Nothing;
                    }
                }
                if matches!(
                    failure,
                    Failure::AppCommands { .. } | Failure::Interaction { .. }
                ) && !self.interaction_failed(&failure, &reason)
                {
                    return Applied::Nothing;
                }
                self.failed(&failure, &reason, ambiguous);
                Applied::Failed {
                    failure,
                    reason,
                    ambiguous,
                }
            }
            Event::ThreadsLoaded {
                request,
                parent,
                scope,
                next,
                ..
            } => self.threads_page(request, parent, scope, next),
            Event::ChallengeRequired { challenge, failure } => {
                self.interaction_challenged(&failure, now);
                Applied::Challenge { challenge, failure }
            }
            Event::NotificationSettings { settings, replace } => {
                self.notifications_changed(settings, replace)
            }
            Event::ServerFolders(folders) => self.folders_changed(folders),
            Event::MessagingPrivacy(privacy) => self.privacy_changed(privacy),
            Event::NoteChanged { user, note } => self.note_changed(user, note),
            // Reads, search and remote members (P2).
            Event::MarkedUnread {
                account,
                channel,
                through,
                anchor,
                mentions,
                version,
            } => self.marked_unread(account, channel, through, anchor, mentions, version),
            Event::LatestMessages(latest) => self.latest_messages(latest),
            Event::SearchResults { request, result } => Applied::SearchResults { request, result },
            Event::MembersFound {
                server,
                query,
                members,
            } => self.members_found(server, query, members),
            // Commands and interactions (P3).
            Event::AppCommands {
                request,
                channel,
                commands,
            } => self.commands_loaded(request, channel, commands, now),
            Event::Interaction { request, done } => self.interaction_answered(request, done),
            Event::ModalOpened {
                request,
                channel,
                modal,
            } => self.modal_opened(request, channel, modal),
            Event::CommandChoices { request, choices } => self.choices_suggested(request, choices),
            Event::AppCommandsChanged { server } => self.commands_changed(server),
            // Message actions and moderation (P4).
            Event::PeopleLoaded {
                request,
                people,
                reached_end,
            } => Applied::People {
                request,
                people,
                reached_end,
            },
            Event::ChannelLoaded { request, channel } => {
                Applied::ChannelLoaded { request, channel }
            }
            // Desktop, language and local voice (P6).
            Event::DetectableGames(games) => self.games_listed(games),
            Event::CallPrivacyCode { channel, code } => self.call_code_changed(channel, code),
            Event::VoiceVerification {
                channel,
                user,
                code,
            } => self.verified(channel, user, code),
            // Presence, identity and expressions (P5).
            event @ (Event::Perks(_)
            | Event::StickerPacks(_)
            | Event::GifResults(_)
            | Event::GifSuggestions { .. }) => self.apply_identity(event),
            // Private calls.
            Event::PrivateCall(call) => self.private_call_changed(call, now),
            Event::PrivateCallEnded { channel } => self.private_call_ended(channel),
            // Sends and uploads (envios-y-subidas).
            Event::PostProgress {
                request,
                sent_bytes,
                total_bytes,
            } => Applied::PostProgress {
                request,
                sent_bytes,
                total_bytes,
            },
            // Encrypted local cache: the core's alone.
            Event::StorageIdentity(_) | Event::ForgetStorage(_) => Applied::Nothing,
            // Rail and notifications.
            Event::VoiceActivity(activity) => self.activity_changed(activity),
            // Voice protocol (voz-protocolo).
            Event::VoiceStartTimes {
                server,
                times,
                whole,
            } => self.voice_started_changed(server, times, whole),
            // Calls and the voice panel: the interface that asked goes on.
            Event::CallChecked { channel, ringable } => Applied::CallChecked { channel, ringable },
            // Rules and onboarding (2026-10-18).
            event @ (Event::ServerRules { .. }
            | Event::RulesAnswered { .. }
            | Event::OnboardingLoaded { .. }
            | Event::OnboardingAnswered { .. }) => self.apply_join(event),
            // Desktop session and games (escritorio-sesion-y-juegos).
            Event::ActivitySharing(on) => self.sharing_changed(on),
            Event::ActivityVisibility(seen) => self.visibility_changed(seen),
            // Scheduled events (eventos-programados).
            event @ (Event::ScheduledEvents { .. }
            | Event::ScheduledEventChanged { .. }
            | Event::ScheduledEventDeleted { .. }
            | Event::EventInterest { .. }
            | Event::EventInterested { .. }) => self.apply_events(event),
            // AutoMod (automod).
            event @ (Event::AutoModRules { .. }
            | Event::AutoModRuleChanged { .. }
            | Event::AutoModRuleRemoved { .. }
            | Event::AutoModExecuted(_)) => self.apply_automod(event),
            // The core's: it arrives here as the failure of its command.
            Event::AutoModBlocked { .. } => Applied::Nothing,
        }
    }

    fn created(&mut self, message: Message, nonce: Option<u64>) -> Applied {
        self.keep_pinned(&message);
        let channel = message.channel;
        if Some(message.author) == self.me
            && message.delivery == Delivery::Sent
            && message.id.0 != 0
            && message.kind == rune_model::MessageKind::Default
        {
            if let Some(nonce) = nonce.filter(|nonce| {
                self.pending
                    .get(nonce)
                    .is_some_and(|(pending_channel, _)| *pending_channel == channel)
                    && self.pending_files.get(nonce).is_none_or(|pending| {
                        let mut remote_ids = std::collections::HashSet::new();
                        pending.files.len() == message.attachments.len()
                            && pending.files.iter().zip(&message.attachments).all(
                                |(expected, actual)| {
                                    expected.filename == actual.filename
                                        && expected.size_bytes == actual.size_bytes
                                        && actual.id.0 != 0
                                        && remote_ids.insert(actual.id)
                                },
                            )
                    })
            }) {
                if let Some((_, shown)) = self.pending.remove(&nonce) {
                    self.timelines.entry(channel).or_default().remove(shown);
                    self.pending_files.remove(&nonce);
                }
            }
        }
        if let Some(typing) = self.typing.get_mut(&channel) {
            typing.retain(|(user, _)| *user != message.author);
        }
        // By name, or through the account's roles (rail and notifications).
        let mentioned = self
            .me
            .is_some_and(|me| self.counts_as_mention(&message, me));
        let read = self.reads.entry(channel).or_default();
        let was_read = !read.unread();
        let fresh = Some(message.id) > read.last_message && !message.delivery.is_local();
        if !message.delivery.is_local() {
            read.last_message = read.last_message.max(Some(message.id));
        }
        if fresh && Some(message.id) > read.last_read && mentioned {
            read.mentions = read.mentions.saturating_add(1);
        }
        if fresh && was_read && Some(message.id) > read.last_read {
            read.first_unread = Some(message.id);
        }
        self.thread_message(&message, fresh);
        let timeline = self.timelines.entry(channel).or_default();
        // A conversation not loaded to its end would get a gap: the message
        // is fetched with the newest page when it is opened.
        if !timeline.latest {
            return Applied::Messages { channel, added: 0 };
        }
        let added = timeline.upsert(message) as usize;
        Applied::Messages { channel, added }
    }

    /// Undo or mark what a failed command changed here.
    fn failed(&mut self, failure: &Failure, reason: &str, ambiguous: bool) {
        self.admin_failed(failure);
        self.threads_failed(failure);
        match failure {
            Failure::Send { channel, nonce } | Failure::SendFiles { channel, nonce, .. } => {
                let Some((_, shown)) = self.pending.get(nonce).copied() else {
                    return;
                };
                let delivery = if ambiguous {
                    Delivery::Unknown {
                        reason: reason.to_owned(),
                    }
                } else {
                    Delivery::Failed {
                        reason: reason.to_owned(),
                    }
                };
                self.update_message(*channel, shown, |message| message.delivery = delivery);
            }
            Failure::Edit { channel, message }
            | Failure::Vote { channel, message }
            | Failure::React {
                channel, message, ..
            } => {
                if let Some(original) = self.before_change.remove(&(*channel, *message)) {
                    if let Some(timeline) = self.timelines.get_mut(channel) {
                        timeline.restore(original);
                    }
                }
            }
            Failure::Stream { channel, owner } => {
                if let Some(watching) = self
                    .watching
                    .iter_mut()
                    .find(|watching| (watching.channel, watching.owner) == (*channel, *owner))
                {
                    watching.phase = StreamPhase::Ended(Some(reason.to_owned()));
                }
            }
            Failure::Share { channel } => {
                if let Some(sharing) = self
                    .sharing
                    .as_mut()
                    .filter(|sharing| sharing.channel == *channel)
                {
                    sharing.phase = StreamPhase::Ended(Some(reason.to_owned()));
                }
            }
            Failure::Voice { channel } => {
                if let Some(call) = self
                    .call
                    .as_mut()
                    .filter(|call| channel.is_none_or(|channel| channel == call.channel))
                {
                    call.phase = CallPhase::Failed {
                        reason: reason.to_owned(),
                    };
                }
            }
            Failure::History { channel, .. } => {
                self.timelines
                    .entry(*channel)
                    .or_default()
                    .fail_load(reason);
            }
            // Nothing of these is shown before the provider confirms it.
            Failure::Delete { .. }
            | Failure::Context { .. }
            | Failure::Invitation { .. }
            | Failure::Status
            | Failure::Server { .. }
            // The interface turns the camera off.
            | Failure::Camera { .. }
            | Failure::Channel { .. }
            | Failure::Role { .. }
            | Failure::Member { .. }
            | Failure::Invite { .. }
            | Failure::Expression { .. }
            | Failure::Profile
            | Failure::ProfileLoad { .. }
            | Failure::Pin { .. }
            | Failure::Relationship { .. }
            | Failure::Direct { .. }
            | Failure::Group { .. }
            | Failure::NewGroup { .. }
            | Failure::Close { .. }
            | Failure::Other => {}
            Failure::Pins { request, .. } => self.cancel_pins(*request),
            Failure::ReadState { .. } => {}
            // See `threads_failed`; nothing of a thread is shown ahead.
            Failure::NewThread { .. } | Failure::Thread { .. } | Failure::ThreadList { .. } => {}
            // A stage's seat moves only when the provider reports it.
            Failure::Stage { .. } => {}
            // A profile in a server changes only when the provider reports it.
            Failure::ServerProfile { .. } => {}
            // Administration (19): see `admin_failed`.
            Failure::NewInvite { .. }
            | Failure::NewExpression { .. }
            | Failure::Prune { .. }
            | Failure::Webhook { .. }
            | Failure::Integration { .. }
            | Failure::GifFavorites => {}
            // The account's settings change only when the provider reports them.
            Failure::AccountSettings
            | Failure::Notifications { .. }
            | Failure::Folders
            | Failure::Privacy
            | Failure::Note { .. } => {}
            // Reads, search and remote members (P2): a reminder and a server
            // read stay marked here; a search owns its own error.
            Failure::Unread { .. } | Failure::ServerRead { .. } | Failure::Search { .. } => {}
            // See `interaction_failed`.
            Failure::AppCommands { .. } | Failure::Interaction { .. } => {}
            // Message actions (P4): nothing of them is shown ahead; the
            // interface owns its requests' errors.
            Failure::Forward { .. }
            | Failure::NewPoll { .. }
            | Failure::EndPoll { .. }
            | Failure::People { .. }
            | Failure::Place { .. } => {}
            // Presence, identity and expressions (P5): nothing is shown
            // ahead; the picker owns its own error.
            Failure::StickerPacks | Failure::Gifs { .. } => {}
            // Server administration, rest (P7): nothing is shown ahead; the
            // settings own these errors.
            Failure::ServerLoad { .. } | Failure::ChannelOrder { .. } => {}
            // Private calls: whom a call rings comes from the provider again.
            Failure::Ring { .. } => {}
            // Nothing was started before the answer.
            Failure::CallCheck { .. } => {}
            // Rules and onboarding (2026-10-18): nothing is shown ahead; the
            // dialog and the settings own these errors.
            Failure::Rules { .. } | Failure::Onboarding { .. } | Failure::OnboardingEdit { .. } => {}
            // Desktop session and games (escritorio-sesion-y-juegos): as read.
            Failure::ActivitySharing => self.sharing_failed(),
            // Scheduled events (eventos-programados): nothing is shown
            // ahead; the dialog owns these errors.
            Failure::ScheduledEvents { .. }
            | Failure::ScheduledEvent { .. }
            | Failure::EventInterest { .. } => {}
            // AutoMod (automod): nothing is shown ahead.
            Failure::AutoModRules { .. } | Failure::AutoModSave { .. } | Failure::AutoModRule { .. } => {}
        }
    }

    fn update_message(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        change: impl FnOnce(&mut Message),
    ) {
        let Some(timeline) = self.timelines.get_mut(&channel) else {
            return;
        };
        let Some(message) = timeline.get(id) else {
            return;
        };
        let mut message = Message::clone(message);
        change(&mut message);
        timeline.remove(id);
        timeline.upsert(message);
    }

    fn ready(&mut self, snapshot: Snapshot) {
        let account = snapshot.me.as_ref().map(|me| me.id);
        let same_account = self.me.is_some() && self.me == account;
        if !same_account {
            self.timelines.clear();
            self.pending.clear();
            self.pending_files.clear();
            self.before_change.clear();
            self.account = Default::default();
            self.identity_reset();
            self.desktop_account_changed();
        }
        self.me = account;
        self.users = snapshot
            .users
            .into_iter()
            .map(|user| (user.id, user))
            .collect();
        self.servers = snapshot.servers;
        let read_by_page = std::mem::take(&mut self.channels);
        self.channels = snapshot
            .channels
            .into_iter()
            .map(|channel| (channel.id, channel))
            .collect::<HashMap<_, _>>()
            .into();
        if same_account {
            self.keep_paged_threads(read_by_page.map.into_values());
        }
        self.members.clear();
        self.member_index.clear();
        for member in snapshot.members {
            let members = self.members.entry(member.server).or_default();
            self.member_index
                .insert((member.server, member.user), members.len());
            members.push(member);
        }
        self.presences = snapshot.presences.into_iter().collect();
        // A new session subscribes again: the old order may be stale.
        self.member_list = None;
        self.ready_relationships(snapshot.relationships);
        self.voice = snapshot.voice;
        // A new session reports its apps again.
        self.voice_activities.clear();
        // And when its voice channels started (voz-protocolo).
        self.voice_started.clear();
        // Rules and onboarding (2026-10-18): read again when needed.
        self.join_ready(same_account);
        // Scheduled events (eventos-programados): read again when shown.
        self.events_ready();
        // AutoMod (automod): read again when its page opens.
        self.automod_ready();
        self.private_calls_ready();
        self.forget_admin(None);
        self.pins.clear();
        self.pins_read = None;
        self.context_read = None;
        // The threads read by page stay (`keep_paged_threads`); where the
        // pages go on is asked again, from the open ones.
        self.thread_pages.clear();
        self.ready_reads(
            same_account,
            snapshot.read_states,
            snapshot.read_versions,
            snapshot.last_messages,
            snapshot.first_unread,
        );
        // A new session may have missed messages: conversations are fetched
        // again to their end when opened, and keep what they had meanwhile.
        for timeline in self.timelines.values_mut() {
            timeline.latest = false;
        }
        self.typing.clear();
    }

    /// Show what the last session left in the local store while the next
    /// one starts: the servers, channels and people, with nobody shown as
    /// around and nothing shown as speaking, since that is not known yet.
    pub fn apply_cached_snapshot(&mut self, mut snapshot: Snapshot) -> Applied {
        if self.me.is_some() {
            return Applied::Nothing;
        }
        self.revision += 1;
        snapshot.presences.clear();
        snapshot.voice.clear();
        self.ready(snapshot);
        self.from_cache = true;
        Applied::Ready
    }

    /// Show what the local store kept of `channel` while the provider is
    /// asked: only into an empty timeline still waiting for its newest page.
    /// An authoritative empty page must stay empty even if the cache is late.
    pub fn apply_cached(&mut self, channel: ChannelId, messages: Vec<Message>) -> Applied {
        let timeline = self.timelines.entry(channel).or_default();
        if timeline.latest || !timeline.is_empty() && timeline.context_source().is_none() {
            return Applied::Nothing;
        }
        timeline.clear_context();
        self.revision += 1;
        let added = messages.len();
        for message in messages {
            timeline.upsert(message);
        }
        Applied::Messages { channel, added }
    }

    /// Show a message being sent until the provider confirms it.
    pub fn add_pending(
        &mut self,
        channel: ChannelId,
        content: String,
        reply_to: Option<MessageId>,
        nonce: u64,
        timestamp: Timestamp,
    ) {
        let Some(me) = self.me else { return };
        let id = local_id(nonce);
        let message = Message {
            id,
            channel,
            author: me,
            content,
            timestamp,
            edited: None,
            kind: Default::default(),
            reply_to,
            mentions: Vec::new(),
            reactions: Vec::new(),
            attachments: Vec::new(),
            stickers: Vec::new(),
            text_references: Vec::new(),
            delivery: Delivery::Sending,
            pinned: false,
            embeds: Vec::new(),
            forwarded: None,
            poll: None,
            components: Default::default(),
            call: None,
            mention_roles: Vec::new(),
            mention_everyone: false,
        };
        self.revision += 1;
        self.pending.insert(nonce, (channel, id));
        self.timelines.entry(channel).or_default().upsert(message);
    }

    /// Show a pending sticker without consuming the composer's text draft.
    pub fn add_pending_sticker(
        &mut self,
        channel: ChannelId,
        sticker: rune_model::Sticker,
        nonce: u64,
        timestamp: Timestamp,
    ) {
        self.add_pending(channel, String::new(), None, nonce, timestamp);
        self.update_local(channel, local_id(nonce), |message| {
            message.stickers = vec![sticker]
        });
    }

    /// The account's message `id` that did not get there, as retrying needs it.
    pub fn unsent(&self, channel: ChannelId, id: MessageId) -> Option<Unsent> {
        let message = self.timelines.get(&channel)?.get(id)?;
        let ambiguous = match &message.delivery {
            Delivery::Failed { .. } => false,
            Delivery::Unknown { .. } => true,
            _ => return None,
        };
        let nonce = self
            .pending
            .iter()
            .find(|(_, shown)| **shown == (channel, id))
            .map(|(nonce, _)| *nonce)?;
        Some(Unsent {
            nonce,
            content: message.content.clone(),
            reply_to: message.reply_to,
            sticker: message.stickers.first().map(|sticker| sticker.id),
            files: self
                .pending_files
                .get(&nonce)
                .map_or_else(Vec::new, |files| files.files.clone()),
            upload_request: self.pending_files.get(&nonce).map(|files| files.request),
            ambiguous,
            mention_reply: !self.quiet_replies.contains(&nonce),
        })
    }

    /// Show the unsent message `id` as being sent again, under `nonce`.
    pub fn resend(&mut self, channel: ChannelId, id: MessageId, nonce: u64) {
        if !self.pending_attachments(channel, id).is_empty() {
            return;
        }
        let Some(old) = self
            .pending
            .iter()
            .find(|(_, shown)| **shown == (channel, id))
            .map(|(nonce, _)| *nonce)
        else {
            return;
        };
        let Some(mut message) = self
            .timelines
            .get_mut(&channel)
            .and_then(|timeline| timeline.remove(id))
            .map(|message| Message::clone(&message))
        else {
            return;
        };
        self.revision += 1;
        self.pending.remove(&old);
        message.id = local_id(nonce);
        message.delivery = Delivery::Sending;
        self.pending.insert(nonce, (channel, message.id));
        self.timelines.entry(channel).or_default().upsert(message);
    }

    /// Drop the unsent message `id`. Returns what it said.
    pub fn discard(&mut self, channel: ChannelId, id: MessageId) -> Option<String> {
        let nonce = self
            .pending
            .iter()
            .find(|(_, shown)| **shown == (channel, id))
            .map(|(nonce, _)| *nonce)?;
        self.pending.remove(&nonce);
        self.pending_files.remove(&nonce);
        self.revision += 1;
        let message = self.timelines.get_mut(&channel)?.remove(id)?;
        Some(message.content.clone())
    }

    /// Change a loaded message before the provider confirms it, keeping how
    /// it was so a refusal can put it back.
    pub fn update_local(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        change: impl FnOnce(&mut Message),
    ) {
        let Some(timeline) = self.timelines.get_mut(&channel) else {
            return;
        };
        let Some(original) = timeline.get(id).cloned() else {
            return;
        };
        self.before_change
            .entry((channel, id))
            .or_insert_with(|| original.clone());
        let mut message = Message::clone(&original);
        change(&mut message);
        self.revision += 1;
        timeline.restore(Arc::new(message));
    }

    /// Whether read markers can be sent to the provider.
    pub fn read_sync(&self) -> bool {
        self.provider
            .as_ref()
            .is_some_and(|provider| provider.capabilities.read_state)
    }

    /// Forget all but the newest `keep` messages of every conversation but
    /// `except`, to bound memory: they come back from the provider or the
    /// cache when opened again.
    pub fn trim(&mut self, except: Option<ChannelId>, keep: usize) {
        for (channel, timeline) in &mut self.timelines {
            if Some(*channel) != except {
                timeline.trim(keep);
            }
        }
    }

    /// Start joining the voice channel `channel`, before the provider answers.
    pub fn begin_call(&mut self, channel: ChannelId, muted: bool, deafened: bool) {
        self.revision += 1;
        self.call = Some(Call {
            channel,
            phase: CallPhase::Joining,
            muted,
            deafened,
            quality: None,
        });
        self.call_connected = None;
        self.call_reported = None;
        self.watching.clear();
        self.sharing = None;
        self.private_call_begun(channel);
    }

    /// Leave the call, before the provider answers.
    pub fn end_call(&mut self) {
        self.revision += 1;
        self.call = None;
        self.call_connected = None;
        self.watching.clear();
        self.sharing = None;
    }

    /// Start watching `owner`'s stream in `channel`, besides the others,
    /// before the provider answers.
    pub fn begin_watch(&mut self, channel: ChannelId, owner: UserId) {
        self.revision += 1;
        self.watching.retain(|watching| watching.owner != owner);
        self.watching.push(Watching {
            channel,
            owner,
            phase: StreamPhase::Connecting,
        });
    }

    /// Stop watching `owner`'s stream, before the provider answers.
    pub fn stop_watch(&mut self, owner: UserId) {
        self.revision += 1;
        self.watching.retain(|watching| watching.owner != owner);
    }

    /// The streams watched, the oldest first.
    pub fn watching(&self) -> &[Watching] {
        &self.watching
    }

    /// A stream's picture arrived: kept while its owner streams there, at
    /// most [`MAX_STREAM_PREVIEWS`] of them.
    fn stream_preview_loaded(
        &mut self,
        channel: ChannelId,
        owner: UserId,
        image: Option<rune_model::RemoteImage>,
        retry_after: Option<std::time::Duration>,
    ) -> Applied {
        let voice = &self.voice;
        let streaming = |user: UserId, channel: ChannelId| {
            voice
                .iter()
                .any(|state| state.user == user && state.channel == channel && state.streaming)
        };
        self.stream_previews
            .retain(|user, (at, _, _)| streaming(*user, *at));
        let full = self.stream_previews.len() >= MAX_STREAM_PREVIEWS
            && !self.stream_previews.contains_key(&owner);
        if full || !streaming(owner, channel) {
            return Applied::Nothing;
        }
        self.stream_previews
            .insert(owner, (channel, image, retry_after));
        Applied::Voice(channel)
    }

    /// The latest picture of `owner`'s stream in `channel`: `None` until it
    /// arrives, `Some(None)` when the provider has none.
    pub fn stream_preview(
        &self,
        channel: ChannelId,
        owner: UserId,
    ) -> Option<Option<&rune_model::RemoteImage>> {
        self.stream_previews
            .get(&owner)
            .filter(|(at, _, _)| *at == channel)
            .map(|(_, image, _)| image.as_ref())
    }

    /// How long the provider asked to wait before `owner`'s picture is
    /// asked for again, when its latest answer had none because of that.
    pub fn stream_preview_retry(&self, owner: UserId) -> Option<std::time::Duration> {
        self.stream_previews.get(&owner)?.2
    }

    /// Start sharing the account's screen in `channel`, before the provider
    /// answers.
    pub fn begin_share(&mut self, channel: ChannelId) {
        let Some(owner) = self.me else { return };
        self.revision += 1;
        self.sharing = Some(Watching {
            channel,
            owner,
            phase: StreamPhase::Connecting,
        });
    }

    /// Stop sharing, before the provider answers.
    pub fn stop_share(&mut self) {
        self.revision += 1;
        self.sharing = None;
    }

    /// The account's own shared screen and how far it got.
    pub fn sharing(&self) -> Option<&Watching> {
        self.sharing.as_ref()
    }

    /// Mute or deafen the account in its call, before the provider answers.
    pub fn set_self_voice(&mut self, muted: bool, deafened: bool) {
        self.revision += 1;
        if let Some(call) = self.call.as_mut() {
            call.muted = muted;
            call.deafened = deafened;
        }
        let me = self.me;
        if let Some(state) = self.voice.iter_mut().find(|state| Some(state.user) == me) {
            state.muted = muted;
            state.deafened = deafened;
        }
    }

    /// Mark `user` speaking or not: from the provider for others, from the
    /// local microphone for the account.
    pub fn set_speaking(&mut self, user: UserId, speaking: bool) {
        if let Some(state) = self
            .voice
            .iter_mut()
            .find(|state| state.user == user && state.speaking != speaking)
        {
            self.revision += 1;
            state.speaking = speaking;
        }
    }

    pub fn call(&self) -> Option<&Call> {
        self.call.as_ref()
    }

    /// Why the account's microphone stays off in its call whatever it
    /// chooses: the server muted it, or it may not speak there.
    pub fn mic_held(&self) -> Option<String> {
        let call = self.call.as_ref()?;
        if let Some(reason) = self.stage_hold(call.channel) {
            return Some(reason.into());
        }
        if self
            .my_voice()
            .is_some_and(|state| state.channel == call.channel && state.server_muted)
        {
            return Some(t("El servidor te silenció en este canal").into());
        }
        self.abilities(call.channel)?
            .speak
            .reason()
            .map(str::to_owned)
    }

    /// Whether the call only takes push-to-talk: voice activity is not
    /// allowed in its channel.
    pub fn push_to_talk_required(&self) -> bool {
        self.call
            .as_ref()
            .and_then(|call| self.abilities(call.channel))
            .is_some_and(|abilities| !abilities.voice_activity.allowed())
    }

    /// How long the call has been connected.
    pub fn call_duration(&self, now: Instant) -> Option<Duration> {
        self.call_connected
            .map(|since| now.saturating_duration_since(since))
    }

    /// Messages loaded across every conversation.
    pub fn loaded_messages(&self) -> usize {
        self.timelines.values().map(Timeline::len).sum()
    }

    pub fn me(&self) -> Option<UserId> {
        self.me
    }

    pub fn user(&self, id: UserId) -> Option<&User> {
        self.users.get(&id)
    }

    pub fn users(&self) -> impl Iterator<Item = &User> {
        self.users.values()
    }

    pub fn presence(&self, id: UserId) -> Presence {
        let mut presence = self.presences.get(&id).cloned().unwrap_or_default();
        // Desktop (P6): what this device publishes shows as the account's own.
        if Some(id) == self.me && presence.activities.is_empty() {
            presence.activities.extend(self.local_activity().cloned());
        }
        presence
    }

    pub fn servers(&self) -> &[Server] {
        &self.servers
    }

    pub fn server(&self, id: ServerId) -> Option<&Server> {
        self.servers.iter().find(|server| server.id == id)
    }

    pub fn channel(&self, id: ChannelId) -> Option<&Channel> {
        self.channels.get(&id)
    }

    /// What the account may do in `channel`, with the reason when it may not.
    pub fn abilities(&self, channel: ChannelId) -> Option<Abilities> {
        let capabilities = self
            .provider
            .as_ref()
            .map(|info| info.capabilities)
            .unwrap_or_default();
        let conversation = self.channel(channel)?;
        let mut abilities = abilities::abilities(conversation, capabilities);
        self.restrict(conversation.server, &mut abilities);
        self.hold_stage(channel, &mut abilities);
        if abilities.attach.allowed()
            && (!self.can_keep_attachments(channel)
                || self.from_cache
                || self.connection != ConnectionState::Connected)
        {
            abilities.attach = Permit::Denied(
                t("Espera a que la cuenta esté conectada para adjuntar archivos").into(),
            );
        }
        if let Some(user) = self.blocked_recipient(channel) {
            let blocked = Permit::Denied(tr!(
                "Bloqueaste a {name}: desbloquéala para escribirle",
                name = user.shown_name()
            ));
            for permit in [
                &mut abilities.write,
                &mut abilities.typing,
                &mut abilities.react,
                &mut abilities.attach,
                &mut abilities.stickers,
                &mut abilities.create_poll,
            ] {
                if permit.allowed() {
                    *permit = blocked.clone();
                }
            }
        }
        // Private calls: nobody blocked is called.
        if let Some(user) = self.blocked_recipient(channel) {
            let blocked = Permit::Denied(tr!(
                "Bloqueaste a {name}: desbloquéala para llamarle",
                name = user.shown_name()
            ));
            for permit in [&mut abilities.ring, &mut abilities.join_voice] {
                if permit.allowed() {
                    *permit = blocked.clone();
                }
            }
        }
        // Watching happens inside the call: the account must be in this one.
        let here = self.call.as_ref().is_some_and(|call| {
            call.channel == channel && !matches!(call.phase, CallPhase::Failed { .. })
        });
        if abilities.watch_stream.allowed() && !here {
            abilities.watch_stream =
                Permit::Denied(t("Entra a la llamada para ver la transmisión").into());
        }
        if abilities.watch_camera.allowed() && !here {
            abilities.watch_camera =
                Permit::Denied(t("Entra a la llamada para ver las cámaras").into());
        }
        Some(abilities)
    }

    /// What the account may do across `server`, with the reason when it may not.
    pub fn server_abilities(&self, server: ServerId) -> Option<ServerAbilities> {
        let capabilities = self
            .provider
            .as_ref()
            .map(|info| info.capabilities)
            .unwrap_or_default();
        let mut abilities = abilities::server_abilities(
            self.server(server)?,
            self.server_permissions(server),
            self.owns(server),
            capabilities,
        );
        self.restrict_server(server, &mut abilities);
        Some(abilities)
    }

    /// The channels of `server` under their categories, both in order. Channels
    /// without a category come first, under `None`.
    pub fn channel_tree(&self, server: ServerId) -> Vec<(Option<&Channel>, Vec<&Channel>)> {
        self.tree_of(server, Self::visible)
    }

    /// [`Self::channel_tree`] of the channels `keep` keeps.
    fn tree_of(
        &self,
        server: ServerId,
        keep: impl Fn(&Channel) -> bool,
    ) -> Vec<(Option<&Channel>, Vec<&Channel>)> {
        let of_server: Vec<&Channel> = self
            .channels
            .index()
            .of_server
            .get(&server)
            .into_iter()
            .flatten()
            .filter_map(|id| self.channels.get(id))
            // Threads hang from their channel, not from a category: see `threads`.
            .filter(|channel| {
                channel.server == Some(server) && !channel.is_thread() && keep(channel)
            })
            .collect();
        let mut tree: Vec<(Option<&Channel>, Vec<&Channel>)> = vec![(None, Vec::new())];
        for category in of_server
            .iter()
            .filter(|channel| channel.kind == ChannelKind::Category)
        {
            tree.push((Some(category), Vec::new()));
        }
        for channel in of_server
            .iter()
            .filter(|channel| channel.kind != ChannelKind::Category)
        {
            let slot = tree
                .iter()
                .position(|(category, _)| category.map(|category| category.id) == channel.parent)
                .unwrap_or(0);
            tree[slot].1.push(channel);
        }
        tree.retain(|(category, channels)| category.is_some() || !channels.is_empty());
        tree
    }

    /// The threads of `parent` the account sees, the most recently active
    /// first; archived ones only when `archived`. None when it does not see
    /// `parent` itself.
    pub fn threads(&self, parent: ChannelId, archived: bool) -> Vec<&Channel> {
        if !self.channel(parent).is_some_and(Self::visible) {
            return Vec::new();
        }
        self.channels
            .index()
            .threads
            .get(&parent)
            .into_iter()
            .flatten()
            .filter_map(|id| self.channels.get(id))
            .filter(|channel| {
                Self::visible(channel)
                    && (archived || !channel.thread.as_ref().is_some_and(|t| t.archived))
            })
            .collect()
    }

    /// The first channel with messages of `server`.
    pub fn first_text_channel(&self, server: ServerId) -> Option<ChannelId> {
        self.channel_tree(server)
            .into_iter()
            .flat_map(|(_, channels)| channels)
            .find(|channel| channel.kind.has_messages())
            .map(|channel| channel.id)
    }

    /// Direct and group conversations, the most recently active first.
    pub fn directs(&self) -> Vec<&Channel> {
        let mut directs: Vec<&Channel> = self
            .channels
            .values()
            .filter(|channel| channel.server.is_none())
            .collect();
        directs.sort_by_key(|channel| {
            (
                std::cmp::Reverse(self.read(channel.id).last_message),
                channel.id,
            )
        });
        directs
    }

    /// What a conversation is called: its name, or the people in it.
    pub fn channel_title(&self, id: ChannelId) -> String {
        let Some(channel) = self.channel(id) else {
            return String::new();
        };
        // A friend's private name titles the conversation with them, as
        // Serein and the official client show it.
        if let ([user], ChannelKind::Direct) = (&channel.recipients[..], channel.kind) {
            if let Some(nickname) = self
                .relationship(*user)
                .and_then(|known| known.nickname.as_deref())
                .filter(|nickname| !nickname.trim().is_empty())
            {
                return nickname.to_owned();
            }
        }
        // A provider's placeholder, in the language shown now: only a
        // conversation outside servers gets one.
        if channel.server.is_none() && channel.name == Channel::UNNAMED {
            return t(Channel::UNNAMED).to_owned();
        }
        if !channel.name.is_empty() {
            return channel.name.clone();
        }
        channel
            .recipients
            .iter()
            .filter_map(|user| self.user(*user))
            .map(User::shown_name)
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn timeline(&self, channel: ChannelId) -> Option<&Timeline> {
        self.timelines.get(&channel)
    }

    pub fn read(&self, channel: ChannelId) -> ReadState {
        self.reads.get(&channel).copied().unwrap_or_default()
    }

    /// Message conversations whose history the current account may read.
    pub fn read_channels(&self) -> impl Iterator<Item = &Channel> {
        self.channels.values().filter(|channel| {
            Self::visible(channel)
                && self
                    .abilities(channel.id)
                    .is_some_and(|a| a.read_history.allowed())
        })
    }

    /// Whether anything in `server` is unread, and how many mentions wait there.
    pub fn server_unread(&self, server: ServerId) -> (bool, u32) {
        self.channels
            .index()
            .of_server
            .get(&server)
            .into_iter()
            .flatten()
            .filter_map(|id| self.channels.get(id))
            .filter(|channel| channel.kind.has_messages() && Self::visible(channel))
            .map(|channel| self.read(channel.id))
            .fold((false, 0), |(unread, mentions), read| {
                (
                    unread || read.unread(),
                    mentions.saturating_add(read.mentions),
                )
            })
    }

    /// Who is typing in `channel`, besides the account.
    pub fn typing(&self, channel: ChannelId, now: Instant) -> Vec<UserId> {
        self.typing
            .get(&channel)
            .into_iter()
            .flatten()
            .filter(|(user, since)| {
                Some(*user) != self.me && now.saturating_duration_since(*since) < TYPING_SHOWN
            })
            .map(|(user, _)| *user)
            .collect()
    }

    /// When someone's typing will stop showing, so the interface knows to
    /// look again.
    pub fn typing_expires(&self, now: Instant) -> Option<Duration> {
        self.typing
            .values()
            .flatten()
            .map(|(_, since)| TYPING_SHOWN.saturating_sub(now.saturating_duration_since(*since)))
            .filter(|left| !left.is_zero())
            .min()
    }

    pub fn voice_in(&self, channel: ChannelId) -> Vec<&VoiceState> {
        self.voice
            .iter()
            .filter(|state| state.channel == channel)
            .collect()
    }

    /// Who is in voice in each server, by channel in the order of its list
    /// and then in the order they arrived, and whether the account's call
    /// is there: one pass, for the whole rail.
    pub fn voice_by_server(&self) -> HashMap<ServerId, ServerVoice<'_>> {
        let mut servers: HashMap<ServerId, ServerVoice<'_>> = HashMap::new();
        for state in &self.voice {
            if let Some(server) = self.channel(state.channel).and_then(|c| c.server) {
                let entry = servers.entry(server).or_default();
                entry.own |= Some(state.user) == self.me;
                entry.people.push(state);
            }
        }
        for (server, voice) in servers
            .iter_mut()
            .filter(|(_, voice)| voice.people.len() > 1)
        {
            let order = self.channel_order(*server);
            voice
                .people
                .sort_by_key(|state| order.get(&state.channel).copied().unwrap_or(usize::MAX));
        }
        // Joining: the account's own state may not have arrived yet.
        if let Some(server) = self
            .call
            .as_ref()
            .and_then(|call| self.channel(call.channel))
            .and_then(|channel| channel.server)
        {
            servers.entry(server).or_default().own = true;
        }
        servers
    }

    /// Where the account is talking, if anywhere.
    pub fn my_voice(&self) -> Option<&VoiceState> {
        self.voice.iter().find(|state| Some(state.user) == self.me)
    }

    fn member(&self, server: ServerId, user: UserId) -> Option<&Member> {
        let index = *self.member_index.get(&(server, user))?;
        self.members.get(&server)?.get(index)
    }

    /// Someone's name in `server`: their nick there, else their name.
    pub fn display_name(&self, user: UserId, server: Option<ServerId>) -> String {
        let nick = server
            .and_then(|server| self.member(server, user))
            .and_then(|member| member.nick.clone());
        nick.or_else(|| self.user(user).map(|user| user.shown_name().to_owned()))
            .unwrap_or_else(|| "desconocido".into())
    }

    /// The highest colored role of `user` in `server`.
    pub fn top_role(&self, user: UserId, server: ServerId) -> Option<&Role> {
        let member = self.member(server, user)?;
        self.server(server)?
            .roles
            .iter()
            .filter(|role| member.roles.contains(&role.id) && role.color.is_some())
            .max_by_key(|role| role.position)
    }

    /// The people of `server` as the member list shows them: by hoisted
    /// role, then whoever else is around, then whoever is not. Each group is
    /// sorted by name.
    pub fn member_groups(&self, server: ServerId) -> Vec<MemberGroup> {
        let Some(members) = self.members.get(&server) else {
            return Vec::new();
        };
        let roles = self
            .server(server)
            .map(|server| server.roles.as_slice())
            .unwrap_or_default();
        let mut hoisted: Vec<&Role> = roles.iter().filter(|role| role.hoisted).collect();
        hoisted.sort_by_key(|role| std::cmp::Reverse(role.position));
        let mut groups: Vec<MemberGroup> = hoisted
            .iter()
            .map(|role| MemberGroup {
                title: role.name.clone(),
                color: role.color,
                members: Vec::new(),
            })
            .collect();
        let (online, offline) = (groups.len(), groups.len() + 1);
        groups.push(MemberGroup {
            title: t("En línea").into(),
            color: None,
            members: Vec::new(),
        });
        groups.push(MemberGroup {
            title: t("Desconectados").into(),
            color: None,
            members: Vec::new(),
        });
        for member in members {
            // The account shows where others see it: invisible is offline.
            let around = self.presence(member.user).status.is_around();
            let slot = if around {
                hoisted
                    .iter()
                    .position(|role| member.roles.contains(&role.id))
                    .unwrap_or(online)
            } else {
                offline
            };
            groups[slot].members.push(member.user);
        }
        for group in &mut groups {
            let mut keyed: Vec<(String, UserId)> = group
                .members
                .iter()
                .map(|user| (self.display_name(*user, Some(server)).to_lowercase(), *user))
                .collect();
            keyed.sort();
            group.members = keyed.into_iter().map(|(_, user)| user).collect();
        }
        groups.retain(|group| !group.members.is_empty());
        groups
    }

    /// The people of a direct conversation, the account included.
    pub fn direct_members(&self, channel: ChannelId) -> Vec<UserId> {
        let Some(channel) = self.channel(channel) else {
            return Vec::new();
        };
        channel.recipients.iter().copied().chain(self.me).collect()
    }
}

#[cfg(test)]
mod tests;
