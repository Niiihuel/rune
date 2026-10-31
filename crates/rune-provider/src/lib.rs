//! The boundary between RUNE and a service. A [`Provider`] receives
//! [`Command`]s and reports [`Event`]s in RUNE's own types, so the state and
//! the interface never see a protocol. Each provider declares what it can do
//! in [`Capabilities`], and says what it is in [`ProviderInfo`]: the interface
//! shows that, so a simulated or limited connection is never presented as
//! something it is not.
//!
//! [`mock::MockProvider`] is a deterministic simulated service for developing
//! and testing everything above this crate.

mod gifs;
mod invitations;
pub mod uploads;
pub mod video;
pub mod voice;
pub use gifs::{
    FavoriteGif, GifAttribution, GifCategory, GifError, GifItem, GifPage, GifQuery, GifResult,
    GifThumbnail, MAX_GIF_CATEGORIES, MAX_GIF_SUGGESTIONS,
};
pub mod mock;
pub use invitations::{
    InvitationDestination, InvitationError, InvitationJoinError, InvitationPreview,
    InvitationResult, InvitationTarget,
};
pub use uploads::{
    PreparedUpload, UploadError, UploadFence, UploadLimits, UploadSendScope, UploadUpdate,
};
mod social;
pub use social::GroupIcon;
mod threads;
pub use threads::{ThreadEdit, ThreadScope};
mod profiles;
pub use profiles::{Crop, ProfileImage, ProfileImageKind, ServerProfileEdit};
mod admin;
pub use admin::{ExpressionImage, ExpressionKind, NewExpression};
// Message actions and moderation (P4).
mod message_actions;
pub use message_actions::{
    without_silent, NewPoll, NewPollAnswer, VoiceModeration, MAX_FORWARDS, POLL_ANSWERS,
    POLL_ANSWER_MAX, POLL_HOURS, POLL_QUESTION_MAX, SILENT,
};
// Server administration, rest (P7).
mod server_admin;
pub use server_admin::RoleIcon;
// Desktop session and games (escritorio-sesion-y-juegos).
mod proxy;
pub use proxy::ApiProxy;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use rune_i18n::t;
use rune_model::{
    AuditEntry, AuditQuery, Ban, CallPhase, CallQuality, Channel, ChannelId, ChannelKind,
    Integration, IntegrationId, Member, Message, MessageId, MessagePatch, Overwrite,
    OverwriteTarget, Permissions, Presence, Relationship, RemoteImage, Rgb, RoleId, Server,
    ServerId, Status, StreamPhase, StreamQuality, TagId, Timestamp, User, UserId, VoiceState,
    Webhook, WebhookId,
};
pub use rune_model::{ExpressionId, Invite};
use rune_model::{
    MessagingPrivacy, NotificationChange, Notifications, PrivacyChange, ServerFolder,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Names one request, so its answer can be matched and a stale one dropped.
pub type RequestId = u64;

/// Something RUNE asks a provider to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Start an interactive authorization flow; never starts automatically.
    BeginSignIn,
    /// Cancel the current authorization attempt and discard its result.
    CancelSignIn,
    /// Forget this account's local session and attempt remote revocation.
    SignOut,
    /// Stop remembering the current session on this device; it stays open.
    ForgetSession,
    /// Enter a remembered account, or a new one through the sign-in flow
    /// (`None`). An open session ends first, as at a sign-out, but stays
    /// remembered.
    UseAccount(Option<UserId>),
    /// Stop remembering one account on this device. If it is the open one,
    /// it stays open, as with [`Command::ForgetSession`].
    ForgetAccount(UserId),
    /// Explicitly accept the invitation shown for this account and destination.
    /// The adapter revalidates both bindings before sending. An acknowledgement
    /// does not grant membership; normal server, channel and relationship
    /// events remain authoritative.
    AcceptInvitation {
        request: RequestId,
        target: InvitationTarget,
        destination: InvitationDestination,
        account: UserId,
        /// Absolute, process-local deadline set at the explicit UI action.
        /// Every queue and adapter must reject an expired command before any
        /// write; this monotonic timestamp is never persisted or sent remotely.
        expires_at: std::time::Instant,
    },
    /// Show the challenge that answered the acceptance `request` with
    /// [`InvitationJoinError::ChallengeRequired`] in the provider's own
    /// window, for the person to solve. Only with a solution, and while the
    /// same account, invitation and request still hold, the adapter sends that
    /// acceptance exactly once more; closing the window sends nothing. The
    /// outcome arrives as [`Event::InvitationAcceptance`] for `request`.
    VerifyInvitation {
        request: RequestId,
        destination: InvitationDestination,
        account: UserId,
        /// As in [`Command::AcceptInvitation`], covering the time to solve it.
        expires_at: std::time::Instant,
    },
    /// Up to `limit` messages before `before`, or the latest ones.
    LoadHistory {
        request: RequestId,
        channel: ChannelId,
        before: Option<MessageId>,
        limit: usize,
    },
    /// A bounded window around one exact message. It proves neither history
    /// continuity nor that either end of the conversation has been reached.
    LoadContext {
        request: RequestId,
        channel: ChannelId,
        anchor: MessageId,
        limit: usize,
    },
    /// Stop working on a request whose answer is no longer wanted.
    Cancel(RequestId),
    /// `nonce` comes back on the created message, to replace the one shown
    /// while it was being sent.
    SendMessage {
        channel: ChannelId,
        content: String,
        reply_to: Option<MessageId>,
        nonce: u64,
        /// A reply notifies the author of `reply_to`; without it, only the
        /// mentions in `content` notify. Needs [`Capabilities::reply_mentions`]
        /// to have any effect.
        mention_reply: bool,
    },
    /// Service-owned immutable files for one scoped, cancellable send attempt.
    SendFiles {
        scope: UploadSendScope,
        content: String,
        reply_to: Option<MessageId>,
        files: Vec<PreparedUpload>,
        fence: UploadFence,
        /// As in [`Command::SendMessage`]: a reply with files notifies its
        /// author only with this.
        mention_reply: bool,
    },
    /// Send one available sticker from this conversation's server.
    SendSticker {
        channel: ChannelId,
        sticker: rune_model::StickerId,
        nonce: u64,
    },
    EditMessage {
        channel: ChannelId,
        message: MessageId,
        content: String,
    },
    DeleteMessage {
        channel: ChannelId,
        message: MessageId,
    },
    /// Make the account's `emoji` on `message` be there (`add`) or not. A
    /// state, not a flip: of two quick ones, the last wins even if the first
    /// fails.
    ToggleReaction {
        channel: ChannelId,
        message: MessageId,
        emoji: String,
        add: bool,
    },
    /// Make exactly `answers` the account's vote on the poll of `message`,
    /// replacing an earlier one; none takes the vote back.
    VotePoll {
        channel: ChannelId,
        message: MessageId,
        answers: Vec<u32>,
    },
    /// The account is typing in `channel`.
    StartTyping {
        channel: ChannelId,
    },
    /// Everything through `message` was read. This inclusive boundary was
    /// previously issued in the channel and may already be deleted; it does
    /// not assert that the message is still available for a context read.
    MarkRead {
        request: RequestId,
        account: UserId,
        channel: ChannelId,
        message: MessageId,
    },
    /// Join the voice channel `channel` with the microphone and the sound
    /// as the account has them, so the call never starts out of step. With
    /// [`Capabilities::private_calls`], `channel` may be a direct or group
    /// conversation: its call starts, or the account answers it.
    JoinVoice {
        channel: ChannelId,
        muted: bool,
        deafened: bool,
    },
    LeaveVoice,
    /// Mute the microphone or stop hearing the channel.
    SetVoiceSelf {
        muted: bool,
        deafened: bool,
    },
    /// Watch the screen `owner` streams in `channel`, the account's call:
    /// its frames reach the [`video::VideoLink`], its sound the call's.
    /// Answered by [`Event::StreamPhase`]. Up to [`video::MAX_STREAMS`]
    /// are watched at once; one more lets go of the one watched longest.
    WatchStream {
        channel: ChannelId,
        owner: UserId,
    },
    StopWatching {
        channel: ChannelId,
        owner: UserId,
    },
    /// Share the account's screen in its call in `channel`, at `quality`:
    /// the host sends its encoded frames through the [`video::VideoLink`].
    /// Answered by [`Event::StreamPhase`] with the account as its owner.
    StartStream {
        channel: ChannelId,
        quality: StreamQuality,
    },
    /// Stop sharing the screen in `channel`.
    StopStream {
        channel: ChannelId,
    },
    /// Turn the account's camera on or off in its call in `channel`: the
    /// host sends its encoded frames through the [`video::VideoLink`]
    /// while it is on. The account's voice state follows.
    SetCamera {
        channel: ChannelId,
        on: bool,
    },
    SetStatus(Status),
    /// The line of text the account shows beside its status, after
    /// `emoji` when one was chosen; neither takes it away. Answered by
    /// [`Event::PresenceChanged`], whose line is the emoji, a space and the text.
    SetCustomStatus {
        text: Option<String>,
        emoji: Option<String>,
        /// When the service takes it away by itself; `None` keeps it.
        until: Option<Timestamp>,
    },
    /// Make a server owned by the account. [`Event::Created`] names it.
    CreateServer {
        request: RequestId,
        name: String,
    },
    EditServer {
        server: ServerId,
        edit: ServerEdit,
    },
    /// Delete a server the account owns, for everyone.
    DeleteServer {
        server: ServerId,
    },
    /// Stop being a member of a server.
    LeaveServer {
        server: ServerId,
    },
    /// Make a channel or a category. `private` hides it from `@everyone`.
    CreateChannel {
        request: RequestId,
        server: ServerId,
        kind: ChannelKind,
        name: String,
        parent: Option<ChannelId>,
        private: bool,
    },
    EditChannel {
        channel: ChannelId,
        edit: ChannelEdit,
    },
    DeleteChannel {
        channel: ChannelId,
    },
    /// Put `overwrite` on `channel`, replacing the one for the same target.
    SetOverwrite {
        channel: ChannelId,
        overwrite: Overwrite,
    },
    RemoveOverwrite {
        channel: ChannelId,
        target: OverwriteTarget,
    },
    /// Make a role with nothing but a name. [`Event::Created`] names it.
    CreateRole {
        request: RequestId,
        server: ServerId,
        name: String,
    },
    EditRole {
        server: ServerId,
        role: RoleId,
        edit: RoleEdit,
    },
    DeleteRole {
        server: ServerId,
        role: RoleId,
    },
    /// Give `user` exactly `roles` (besides `@everyone`).
    SetMemberRoles {
        server: ServerId,
        user: UserId,
        roles: Vec<RoleId>,
    },
    /// The name `user` goes by in `server`; `None` for their own.
    SetNick {
        server: ServerId,
        user: UserId,
        nick: Option<String>,
    },
    /// Remove `user` from `server`; they may join again.
    Kick {
        server: ServerId,
        user: UserId,
        reason: Option<String>,
    },
    /// Remove `user` from `server` and keep them out, and delete the messages
    /// they sent in the last `delete_message_seconds` (0: none).
    Ban {
        server: ServerId,
        user: UserId,
        reason: Option<String>,
        delete_message_seconds: u32,
    },
    Unban {
        server: ServerId,
        user: UserId,
    },
    /// Up to `limit` of who is banned from `server`, by ascending user id
    /// after `after`; answered by [`Event::BansLoaded`].
    LoadBans {
        request: RequestId,
        server: ServerId,
        after: Option<UserId>,
        limit: usize,
    },
    /// The invitations of `server`, answered by [`Event::InvitesLoaded`].
    LoadInvites {
        request: RequestId,
        server: ServerId,
    },
    /// Make the invitation `code` stop working. Answered by [`Event::InviteRemoved`].
    RevokeInvite {
        server: ServerId,
        code: String,
    },
    /// Rename a custom expression; the change arrives as [`Event::ServerChanged`].
    RenameExpression {
        server: ServerId,
        expression: ExpressionId,
        name: String,
    },
    /// Delete a custom expression; the change arrives as [`Event::ServerChanged`].
    DeleteExpression {
        server: ServerId,
        expression: ExpressionId,
    },
    /// Change the account's own profile.
    EditProfile(ProfileEdit),
    /// Up to `limit` changes made to `server` matching `query`, newest
    /// first, older than the entry `before`; answered by [`Event::AuditLoaded`].
    LoadAudit {
        request: RequestId,
        server: ServerId,
        query: AuditQuery,
        before: Option<u64>,
        limit: usize,
    },
    /// Keep `message` in its channel's list of pinned messages. The change
    /// arrives as [`Event::MessageUpdated`].
    PinMessage {
        channel: ChannelId,
        message: MessageId,
    },
    UnpinMessage {
        channel: ChannelId,
        message: MessageId,
    },
    /// What a profile adds to `user`'s account, as seen in `server` (or
    /// outside any), answered by [`Event::ProfileLoaded`].
    LoadProfile {
        request: RequestId,
        user: UserId,
        server: Option<ServerId>,
        /// Whether to read the effect they wear too, which takes a read of
        /// its own; false while this device hides profile effects, so that
        /// nothing of them is asked for (2026-10-18).
        effect: bool,
    },
    /// The link of rich-presence button `index` (0 or 1) of `user`'s activity
    /// in `session`, which the service asks the app for (`application`, when
    /// the activity has one). Needs [`Capabilities::activity_buttons`];
    /// answered by [`Event::ActivityButtonUrl`], or by [`Event::Failed`].
    OpenActivityButton {
        request: RequestId,
        user: UserId,
        session: String,
        application: Option<u64>,
        index: u8,
    },
    /// Keep `server`'s member list, as seen from `channel`, current around
    /// its rows `first..=last` (the ones in view), answered by
    /// [`Event::MemberListChanged`] as it changes. Replaces any list watched.
    WatchMemberList {
        server: ServerId,
        channel: ChannelId,
        first: u32,
        last: u32,
    },
    /// Stop keeping any member list current.
    UnwatchMemberList,
    /// The pinned messages of `channel`, answered by [`Event::PinsLoaded`].
    LoadPins {
        request: RequestId,
        channel: ChannelId,
    },
    /// Ask the person whose handle is `username` to be friends. Answered by
    /// [`Event::RelationshipChanged`]; accepting their own request if they
    /// sent one first.
    SendFriendRequest {
        username: String,
    },
    /// Accept the request `user` sent.
    AcceptFriendRequest {
        user: UserId,
    },
    /// End the relationship with `user`, whatever it is: remove a friend,
    /// ignore or cancel a request, or unblock. Answered by
    /// [`Event::RelationshipRemoved`].
    RemoveRelationship {
        user: UserId,
    },
    /// Block `user`, ending a friendship or request with them.
    BlockUser {
        user: UserId,
    },
    /// Ignore `user`: their messages fold, they raise no notifications, and
    /// they are not told. Answered by [`Event::RelationshipChanged`], which
    /// keeps any relationship they have.
    IgnoreUser {
        user: UserId,
    },
    /// Stop ignoring `user`. Answered by [`Event::RelationshipChanged`], or
    /// [`Event::RelationshipRemoved`] when that was all there was.
    UnignoreUser {
        user: UserId,
    },
    /// The direct conversation with `user`, made if there is none. The
    /// provider reports the channel and then [`Event::Created`]; asking again
    /// names the same channel.
    OpenDirect {
        request: RequestId,
        user: UserId,
    },
    /// Rename a group conversation; empty takes the name away.
    RenameGroup {
        channel: ChannelId,
        name: String,
    },
    AddGroupMember {
        channel: ChannelId,
        user: UserId,
    },
    RemoveGroupMember {
        channel: ChannelId,
        user: UserId,
    },
    /// Stop being in a group conversation. Answered by [`Event::ChannelRemoved`].
    LeaveGroup {
        channel: ChannelId,
    },
    // Video, phase 5.
    /// Pause or resume the account's shared screen in `channel`: nothing
    /// goes out while paused, and those watching are told. Answered by
    /// [`Event::StreamPhase`] with the account as its owner.
    PauseStream {
        channel: ChannelId,
        paused: bool,
    },
    /// The latest picture of the screen `owner` streams in the voice
    /// channel `channel` of `server` (`None`: a conversation's call), for
    /// its row in the column. Answered by [`Event::StreamPreview`], with
    /// none when it cannot be asked; nobody waits on its failure.
    LoadStreamPreview {
        server: Option<ServerId>,
        channel: ChannelId,
        owner: UserId,
    },
    /// A group conversation with `users`, friends of the account, which owns
    /// it. The provider reports the channel and then [`Event::Created`].
    CreateGroup {
        request: RequestId,
        users: Vec<UserId>,
    },
    /// Change a group's picture; `None` takes it away.
    SetGroupIcon {
        channel: ChannelId,
        icon: Option<GroupIcon>,
    },
    /// Take a direct conversation off the list. Nothing is deleted: it comes
    /// back, history and all, when a new message arrives or it is opened
    /// again. Answered by [`Event::ChannelRemoved`].
    CloseDirect {
        channel: ChannelId,
    },
    /// Give a friend a private name; `None` takes it away. Answered by
    /// [`Event::RelationshipChanged`].
    SetFriendNickname {
        user: UserId,
        nickname: Option<String>,
    },
    // Threads and forums.
    /// Start a thread in `channel`: from `message` when given, else on its
    /// own, and `private` only then. The provider reports the thread, then
    /// [`Event::Created`].
    CreateThread {
        request: RequestId,
        channel: ChannelId,
        message: Option<MessageId>,
        name: String,
        private: bool,
    },
    /// Start a post in `forum`: a thread named `title`, carrying `tags`,
    /// whose first message is `content` with `files` attached (none for
    /// text only; they need [`Capabilities::attachments`]). Reported like
    /// [`Command::CreateThread`].
    CreatePost {
        request: RequestId,
        forum: ChannelId,
        title: String,
        tags: Vec<TagId>,
        content: String,
        files: Vec<PreparedUpload>,
    },
    /// A page of the threads of `parent` in `scope`, after the one whose
    /// `next` is `before`. Answered by [`Event::ThreadsLoaded`].
    LoadThreads {
        request: RequestId,
        parent: ChannelId,
        scope: ThreadScope,
        before: Option<String>,
    },
    /// Follow a thread, or stop. Answered by [`Event::ChannelChanged`], or
    /// [`Event::ChannelRemoved`] when leaving hides a private thread.
    FollowThread {
        thread: ChannelId,
        follow: bool,
    },
    /// Rename, close, lock or tag a thread. Answered by [`Event::ChannelChanged`].
    EditThread {
        thread: ChannelId,
        edit: ThreadEdit,
    },
    /// Delete a thread and its messages, for everyone. Answered by
    /// [`Event::ChannelRemoved`].
    DeleteThread {
        thread: ChannelId,
    },
    // Stages.
    /// Raise the account's hand to speak on the stage `channel` of its
    /// call, or lower it. Answered by [`Event::VoiceChanged`].
    RaiseHand {
        channel: ChannelId,
        raised: bool,
    },
    /// Make `user` a speaker on the stage `channel`, or send them to the
    /// audience. Another person is invited and speaks once they accept; the
    /// account itself accepts an invitation this way, or steps down.
    /// Answered by [`Event::VoiceChanged`].
    SetSpeaker {
        channel: ChannelId,
        user: UserId,
        speaker: bool,
    },
    // Profiles.
    /// Change the account's profile in `server`: the name it goes by, its
    /// picture, banner and «Sobre mí» there. Answered by
    /// [`Event::MemberChanged`] and then [`Event::ProfileChanged`].
    EditServerProfile {
        server: ServerId,
        edit: ServerProfileEdit,
    },
    // Verification of writes.
    /// Answer [`Event::ChallengeRequired`] for `challenge`. With `solve`, the
    /// provider shows the challenge in its own window for the person to
    /// solve, never solving it itself; with a solution it sends the waiting
    /// action exactly once more. Without `solve`, or when the window closes,
    /// the action fails and nothing is sent.
    SolveChallenge {
        challenge: RequestId,
        solve: bool,
    },
    // Groups (11.3).
    /// Make `user`, someone in the group `channel`, its owner; the account
    /// stops being it. Answered by [`Event::ChannelChanged`].
    SetGroupOwner {
        channel: ChannelId,
        user: UserId,
    },
    // Administration (19).
    /// Keep `user` from taking part in `server` until `until`; `None` lifts
    /// it. Answered by [`Event::MemberChanged`].
    TimeoutMember {
        server: ServerId,
        user: UserId,
        until: Option<Timestamp>,
        reason: Option<String>,
    },
    /// A new invitation to `server` through `channel`, lasting
    /// `max_age_secs` (0: it never expires) for `max_uses` (0: no limit);
    /// `temporary` members leave when they disconnect unless given a role.
    /// Answered by [`Event::InviteCreated`].
    CreateInvite {
        request: RequestId,
        server: ServerId,
        channel: ChannelId,
        max_age_secs: u32,
        max_uses: u32,
        temporary: bool,
    },
    /// Add a custom emoji or sticker to `server`. Answered by
    /// [`Event::ServerChanged`] and then [`Event::Created`].
    CreateExpression {
        request: RequestId,
        server: ServerId,
        expression: NewExpression,
    },
    /// A page of `server`'s members for its administration, the one after
    /// the page whose `next` is `after`. Answered by [`Event::MembersLoaded`].
    LoadMembers {
        request: RequestId,
        server: ServerId,
        after: Option<String>,
        /// The order and filter of the pages; anything but the default
        /// needs [`Capabilities::member_queries`].
        query: rune_model::MemberQuery,
    },
    /// How many members without a role were not seen in `days` (1 to 30),
    /// or remove them with `remove`. Answered by [`Event::Pruned`].
    PruneMembers {
        request: RequestId,
        server: ServerId,
        days: u32,
        remove: bool,
    },
    /// `server`'s webhooks, answered by [`Event::WebhooksLoaded`].
    LoadWebhooks {
        request: RequestId,
        server: ServerId,
    },
    /// A webhook named `name` that posts in `channel`. Answered by
    /// [`Event::WebhookChanged`].
    CreateWebhook {
        server: ServerId,
        channel: ChannelId,
        name: String,
    },
    /// Rename `webhook` or move it to `channel`; `None` leaves each as it
    /// is. Answered by [`Event::WebhookChanged`].
    EditWebhook {
        server: ServerId,
        webhook: WebhookId,
        name: Option<String>,
        channel: Option<ChannelId>,
    },
    /// Answered by [`Event::WebhookRemoved`].
    DeleteWebhook {
        server: ServerId,
        webhook: WebhookId,
    },
    /// `server`'s integrations, answered by [`Event::IntegrationsLoaded`].
    LoadIntegrations {
        request: RequestId,
        server: ServerId,
    },
    /// Remove `integration` from `server`, with its bot if it has one.
    /// Answered by [`Event::IntegrationRemoved`].
    DeleteIntegration {
        server: ServerId,
        integration: IntegrationId,
    },
    /// The GIF favorites the provider keeps for the account, answered by
    /// [`Event::GifFavorites`]. Needs [`Capabilities::synced_gif_favorites`].
    LoadGifFavorites,
    /// Add `gif` to the favorites the provider keeps, or take it out.
    /// Answered by [`Event::GifFavorites`].
    SetGifFavorite {
        gif: FavoriteGif,
        favorite: bool,
    },
    // The account's synced settings (P1).
    /// Read again what the account keeps with the service: its status, its
    /// folders and its messaging privacy. Answered by
    /// [`Event::PresenceChanged`], [`Event::ServerFolders`] and
    /// [`Event::MessagingPrivacy`], as far as the provider keeps each.
    LoadAccountSettings,
    /// Change how `server` notifies, or the direct conversations (`None`).
    /// Answered by [`Event::NotificationSettings`].
    SetNotifications {
        server: Option<ServerId>,
        change: NotificationChange,
    },
    /// Lay the rail out as `folders` say, only while the service still has
    /// the layout last reported. Answered by [`Event::ServerFolders`].
    SetServerFolders(Vec<ServerFolder>),
    /// Answered by [`Event::MessagingPrivacy`].
    SetMessagingPrivacy(PrivacyChange),
    /// Read the account's private note on `user`. Answered by
    /// [`Event::NoteChanged`].
    LoadNote {
        user: UserId,
    },
    /// Keep `note` as the account's private note on `user`; empty takes it
    /// away. Answered by [`Event::NoteChanged`].
    SetNote {
        user: UserId,
        note: String,
    },
    // Reads, search and remote members (P2).
    /// Hold `channel` unread from `message`, its first unread, at the
    /// provider too, so the account's other devices show it so; `mentions`
    /// is how many of the account's mentions that leaves unread. Needs
    /// [`Capabilities::remote_unread`]; answered by [`Event::MarkedUnread`].
    MarkUnread {
        account: UserId,
        channel: ChannelId,
        message: MessageId,
        mentions: u32,
    },
    /// Everything in `server` was read. Needs [`Capabilities::server_read`];
    /// how far each channel got arrives as [`Event::ReadStateConfirmed`].
    MarkServerRead {
        account: UserId,
        server: ServerId,
    },
    /// A page of what matches `search` at the provider, from its result
    /// `offset`. Needs [`Capabilities::message_search`]; answered by
    /// [`Event::SearchResults`].
    SearchMessages {
        request: RequestId,
        search: rune_model::RemoteSearch,
        offset: u32,
    },
    /// The members of `server` whose name starts with `query`, looked up at
    /// the provider. Needs [`Capabilities::member_search`]; answered by
    /// [`Event::MembersFound`] when it finds any. Nobody waits on it: a
    /// newer lookup replaces one not sent yet.
    SearchMembers {
        server: ServerId,
        query: String,
    },
    // Commands and interactions (P3).
    /// The apps' commands `channel` offers, answered by
    /// [`Event::AppCommands`]. Needs [`Capabilities::app_commands`].
    LoadAppCommands {
        request: RequestId,
        channel: ChannelId,
    },
    /// Tell an app what the account did in `channel`. Followed by
    /// [`Event::Interaction`], [`Event::ModalOpened`] or the app's messages,
    /// or by [`Event::Failed`]. Needs [`Capabilities::interactions`], or
    /// [`Capabilities::app_commands`] for a command.
    Interact {
        request: RequestId,
        channel: ChannelId,
        interaction: rune_model::interactions::Interaction,
        /// The files a modal's answers name (`FieldValue::Files`), each
        /// once; they need [`Capabilities::attachments`]. Core joins them,
        /// as for [`Command::CreatePost`]: the interface sends none.
        files: Vec<PreparedUpload>,
    },
    /// What the app of a command suggests for the option being written
    /// (`CommandUse::focused`), answered by [`Event::CommandChoices`]; a
    /// failure is no suggestion and is not reported.
    SuggestChoices {
        request: RequestId,
        channel: ChannelId,
        command: rune_model::interactions::CommandUse,
    },
    // Message actions and moderation (P4).
    /// Forward `message` of `channel` to the conversation `to`: a copy that
    /// says it was forwarded. A note goes as a message of its own, after
    /// it. Needs [`Capabilities::forward_messages`]; answered by
    /// [`Event::MessageCreated`] in `to`, with `nonce`.
    ForwardMessage {
        channel: ChannelId,
        message: MessageId,
        to: ChannelId,
        nonce: u64,
    },
    /// Start `poll` in `channel` as a message of its own. Needs
    /// [`Capabilities::create_polls`]; answered by [`Event::MessageCreated`]
    /// with `nonce`.
    CreatePoll {
        channel: ChannelId,
        poll: NewPoll,
        nonce: u64,
    },
    /// Close the account's own poll on `message` now, for everyone.
    /// Answered by [`Event::MessageUpdated`].
    EndPoll {
        channel: ChannelId,
        message: MessageId,
    },
    /// A page of who reacted with `emoji` to `message`, those after `after`
    /// by id. Needs [`Capabilities::reaction_lists`]; answered by
    /// [`Event::PeopleLoaded`].
    LoadReactions {
        request: RequestId,
        channel: ChannelId,
        message: MessageId,
        emoji: String,
        after: Option<UserId>,
    },
    /// A page of who voted for `answer` of the poll on `message`, those
    /// after `after` by id. Needs [`Capabilities::poll_voters`]; answered
    /// by [`Event::PeopleLoaded`].
    LoadPollVoters {
        request: RequestId,
        channel: ChannelId,
        message: MessageId,
        answer: u32,
        after: Option<UserId>,
    },
    /// A conversation the account may see but that is not loaded, such as
    /// a closed direct one or an old thread, named by a link. Needs
    /// [`Capabilities::load_channels`]; answered by [`Event::ChannelChanged`]
    /// and then [`Event::ChannelLoaded`].
    LoadChannel {
        request: RequestId,
        channel: ChannelId,
    },
    /// Change `user`'s voice in `server` as its moderation. Needs
    /// [`Capabilities::voice_moderation`]; answered by [`Event::VoiceChanged`]
    /// or [`Event::VoiceLeft`].
    ModerateVoice {
        server: ServerId,
        user: UserId,
        change: VoiceModeration,
    },
    // Desktop, language and local voice (P6).
    /// Publish what this device says the person is doing as the account's
    /// activity, beside its status; `None` takes it away. Needs
    /// [`Capabilities::local_activity`]; the account's presence follows
    /// as [`Event::PresenceChanged`] once published.
    SetLocalActivity(Option<rune_model::LocalActivity>),
    /// The games the provider can recognize by their programs. Needs
    /// [`Capabilities::detectable_games`]; answered by
    /// [`Event::DetectableGames`].
    LoadDetectableGames,
    /// The code the account and `user` compare to know nobody else holds
    /// the keys of the call in `channel`. Needs
    /// [`Capabilities::voice_privacy_code`]; answered by
    /// [`Event::VoiceVerification`].
    VoiceVerificationCode {
        channel: ChannelId,
        user: UserId,
    },
    // Presence, identity and expressions (P5).
    /// The sticker packs the service offers everyone, answered by
    /// [`Event::StickerPacks`]. Needs [`Capabilities::sticker_packs`].
    LoadStickerPacks,
    /// A page of the service's own GIFs for `query` from `offset`, answered
    /// by [`Event::GifResults`]. Needs [`Capabilities::gif_search`].
    SearchGifs {
        request: RequestId,
        query: GifQuery,
        offset: u32,
    },
    /// What to search for: completions of `query`, or what is trending when
    /// it is empty. Answered by [`Event::GifSuggestions`]; nobody waits on it.
    SuggestGifs {
        request: RequestId,
        query: String,
    },
    // Server administration, rest (P7).
    /// Read what `server`'s profile says beyond the server itself: its
    /// traits. Answered by [`Event::ServerChanged`]. Needs
    /// [`Capabilities::server_identity`].
    LoadServerProfile {
        request: RequestId,
        server: ServerId,
    },
    /// Put `server`'s channels where `moves` say, all at once: each one
    /// under its category and at its place among its siblings. Answered by
    /// [`Event::ChannelChanged`] for each one that moved. Needs
    /// [`Capabilities::reorder_channels`].
    ReorderChannels {
        server: ServerId,
        moves: Vec<rune_model::ChannelMove>,
    },
    /// How many members have each role of `server`, answered by
    /// [`Event::RoleCounts`]; nobody waits on it. Needs
    /// [`Capabilities::role_counts`].
    LoadRoleCounts {
        request: RequestId,
        server: ServerId,
    },
    // A real account's first cards (2026-10-17).
    /// Keep the presence of `people` of `server` current while their cards
    /// show, as the member list's rows are; an empty list stops. Answered
    /// by [`Event::PresenceChanged`]; nobody waits on it.
    WatchPeople {
        server: ServerId,
        people: Vec<UserId>,
    },
    // Private calls.
    /// Ring `users` of the direct or group conversation `channel` to its
    /// call, everyone in it but the account when empty. The account starts
    /// or joins that call with [`Command::JoinVoice`]. Needs
    /// [`Capabilities::private_calls`]; whom it rings comes back as
    /// [`Event::PrivateCall`].
    RingCall {
        channel: ChannelId,
        users: Vec<UserId>,
    },
    /// Stop ringing `users` to the call of `channel`; the account alone, to
    /// decline a call coming in. Answered by [`Event::PrivateCall`].
    StopRinging {
        channel: ChannelId,
        users: Vec<UserId>,
    },
    // The opening screen (2026-10-18).
    /// Connect again now instead of at the next attempt, while the
    /// connection waits to try again ([`ConnectionState::Reconnecting`]);
    /// nothing otherwise. Needs [`Capabilities::reconnect_now`].
    ReconnectNow,
    // Voice protocol (voz-protocolo).
    /// The server whose channels the column shows now, `None` for none: the
    /// provider may keep what they show current, such as since when its
    /// voice rooms have had someone ([`Event::VoiceStartTimes`]). Nobody
    /// waits on it.
    WatchServer(Option<ServerId>),
    // Calls and the voice panel (llamadas-y-panel-de-voz).
    /// Ask, before starting it, whether the call of the direct conversation
    /// `channel` may ring the other person, as the official client asks.
    /// Answered by [`Event::CallChecked`]; failed with
    /// [`Failure::CallCheck`], the call is not made: `refused` when the
    /// service itself said no (Discord: they are not friends), not when the
    /// answer never came. Needs [`Capabilities::call_check`].
    CheckCall {
        channel: ChannelId,
    },
    // Rules and onboarding (2026-10-18).
    /// `server`'s rules, which the account accepts before it takes part.
    /// Needs [`Capabilities::server_rules`]; answered by
    /// [`Event::ServerRules`].
    LoadServerRules {
        request: RequestId,
        server: ServerId,
    },
    /// Accept `rules`, as they were shown, with `answers` in the order of
    /// their fields. Needs [`Capabilities::server_rules`]; answered by
    /// [`Event::RulesAnswered`].
    AnswerServerRules {
        request: RequestId,
        server: ServerId,
        rules: rune_model::ServerRules,
        answers: Vec<rune_model::RulesAnswer>,
    },
    /// `server`'s onboarding: its questions and what the account chose.
    /// Needs [`Capabilities::onboarding`]; answered by
    /// [`Event::OnboardingLoaded`].
    LoadOnboarding {
        request: RequestId,
        server: ServerId,
    },
    /// Finish `onboarding`, as it was shown, with the options `chosen`.
    /// Needs [`Capabilities::onboarding`]; answered by
    /// [`Event::OnboardingAnswered`]. The roles they give follow as
    /// [`Event::MemberChanged`].
    AnswerOnboarding {
        request: RequestId,
        server: ServerId,
        onboarding: rune_model::Onboarding,
        chosen: Vec<rune_model::ChoiceId>,
    },
    /// Replace `server`'s onboarding with `onboarding` (but what the
    /// account chose). Needs [`Capabilities::manage_onboarding`]; answered
    /// by [`Event::OnboardingLoaded`] with what was saved.
    SaveOnboarding {
        request: RequestId,
        server: ServerId,
        onboarding: rune_model::Onboarding,
    },
    // Threads, forums and stages (2026-10-18).
    /// Silence the thread the account follows until `until`
    /// ([`rune_model::MUTED_FOREVER`] for good), or, with `None`, hear it
    /// again. Silenced, as a muted channel, it alerts of nothing, mentions
    /// included, and marks only how many mentions it has. Answered by
    /// [`Event::ChannelChanged`]. Needs [`Capabilities::mute_threads`].
    MuteThread {
        thread: ChannelId,
        until: Option<Timestamp>,
    },
    // Desktop session and games (escritorio-sesion-y-juegos).
    /// Let the account show its activity to others, or no longer: the
    /// person's explicit choice. Needs [`Capabilities::activity_sharing`];
    /// answered by [`Event::ActivitySharing`] with what the service kept.
    SetActivitySharing(bool),
    // Scheduled events (eventos-programados).
    /// `server`'s scheduled events. Needs [`Capabilities::scheduled_events`];
    /// answered by [`Event::ScheduledEvents`].
    LoadScheduledEvents {
        request: RequestId,
        server: ServerId,
    },
    /// Who said `event` interests them. Needs
    /// [`Capabilities::scheduled_events`]; answered by
    /// [`Event::EventInterested`].
    LoadEventInterested {
        request: RequestId,
        server: ServerId,
        event: rune_model::ScheduledEventId,
    },
    /// Say whether `event` interests the account. Needs
    /// [`Capabilities::scheduled_events`]; answered by
    /// [`Event::EventInterest`] for the account.
    SetEventInterest {
        request: RequestId,
        server: ServerId,
        event: rune_model::ScheduledEventId,
        interested: bool,
    },
    /// Plan an event in `server`. Needs
    /// [`Capabilities::manage_scheduled_events`]; answered by
    /// [`Event::ScheduledEventChanged`] with `request`.
    CreateScheduledEvent {
        request: RequestId,
        server: ServerId,
        draft: rune_model::EventDraft,
    },
    /// Change `event` as `draft` says; answered as a new one.
    EditScheduledEvent {
        request: RequestId,
        server: ServerId,
        event: rune_model::ScheduledEventId,
        draft: rune_model::EventDraft,
    },
    /// Start a planned event (`Active`) or end one happening (`Completed`);
    /// answered as a new one.
    SetScheduledEventStatus {
        request: RequestId,
        server: ServerId,
        event: rune_model::ScheduledEventId,
        status: rune_model::EventStatus,
    },
    /// Delete `event`; answered by [`Event::ScheduledEventDeleted`] with
    /// `request`.
    DeleteScheduledEvent {
        request: RequestId,
        server: ServerId,
        event: rune_model::ScheduledEventId,
    },
    // AutoMod (automod).
    /// `server`'s AutoMod rules, answered by [`Event::AutoModRules`].
    /// Needs [`Capabilities::automod`], as every AutoMod command.
    LoadAutoModRules {
        request: RequestId,
        server: ServerId,
    },
    /// Make `rule` (without an id) or change the one it names. Answered by
    /// [`Event::AutoModRuleChanged`] with what was saved and `request`.
    SaveAutoModRule {
        request: RequestId,
        server: ServerId,
        rule: rune_model::AutoModRule,
    },
    /// Turn `rule` on or off, leaving the rest of it as the service has it.
    /// Answered by [`Event::AutoModRuleChanged`].
    SetAutoModRuleEnabled {
        server: ServerId,
        rule: rune_model::AutoModRuleId,
        enabled: bool,
    },
    /// Answered by [`Event::AutoModRuleRemoved`].
    DeleteAutoModRule {
        server: ServerId,
        rule: rune_model::AutoModRuleId,
    },
    // Streams (transmisiones).
    /// The picture of the account's own stream in `channel` (of `server`,
    /// `None` in a conversation's call) others see before watching it: a
    /// JPEG that fits 512×288. `owner` is the account. Needs
    /// [`Capabilities::upload_stream_previews`]; nothing answers it and
    /// nobody waits on its failure.
    UploadStreamPreview {
        server: Option<ServerId>,
        channel: ChannelId,
        owner: UserId,
        jpeg: Vec<u8>,
    },
}

/// Changes to a server; `None` leaves a field as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerEdit {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub accent: Option<Rgb>,
    // Server administration, rest (P7).
    /// A new picture, or `Some(None)` to take it away; needs
    /// [`Capabilities::server_identity`], as do the traits.
    pub icon: Option<Option<ProfileImage>>,
    /// What its profile says it is about, at most [`rune_model::MAX_TRAITS`].
    pub traits: Option<Vec<rune_model::ServerTrait>>,
    /// The rest needs [`Capabilities::server_safety`]: where the service
    /// posts its own messages (`Some(None)`: nowhere), and which.
    pub system_channel: Option<Option<ChannelId>>,
    pub system_messages: Option<rune_model::SystemMessages>,
    pub mentions_only: Option<bool>,
    pub afk_channel: Option<Option<ChannelId>>,
    /// One of [`rune_model::AFK_TIMEOUTS`].
    pub afk_timeout: Option<u32>,
    pub verification: Option<rune_model::Verification>,
    pub media_scan: Option<rune_model::MediaScan>,
    /// Pause new invitations, or resume them; needs
    /// [`Capabilities::pause_invites`].
    pub invites_paused: Option<bool>,
    // Icon, banner and community (2026-10-18).
    /// A new banner, or `Some(None)` to take it away; needs
    /// [`Capabilities::server_banner`] and a server whose options allow
    /// one. A GIF as banner or icon needs the options to let it move.
    pub banner: Option<Option<ProfileImage>>,
    /// Make it a community, or stop it being one; needs
    /// [`Capabilities::community`]. To become one it needs, here or
    /// already, what [`rune_model::ServerOptions::community_ready`] asks.
    pub community: Option<bool>,
    /// A community's rules channel, and the one for the service's news.
    pub rules_channel: Option<Option<ChannelId>>,
    pub updates_channel: Option<Option<ChannelId>>,
}

/// Changes to a channel; `None` leaves a field as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChannelEdit {
    pub name: Option<String>,
    pub topic: Option<Option<String>>,
    pub parent: Option<Option<ChannelId>>,
    pub position: Option<i32>,
    pub slowmode_secs: Option<u32>,
    pub nsfw: Option<bool>,
    // Server administration, rest (P7).
    /// A forum's tags, in order: one whose id is 0 is new, one left out is
    /// deleted. Needs [`Capabilities::forum_settings`], as `forum` does.
    pub tags: Option<Vec<rune_model::ForumTag>>,
    /// A forum's layout, order, required tag and default reaction.
    pub forum: Option<rune_model::ForumDefaults>,
}

/// Changes to a role; `None` leaves a field as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoleEdit {
    pub name: Option<String>,
    pub color: Option<Option<Rgb>>,
    pub hoisted: Option<bool>,
    pub mentionable: Option<bool>,
    pub permissions: Option<Permissions>,
    /// Higher outranks lower; the others move to make room.
    pub position: Option<i32>,
    // Server administration, rest (P7).
    /// What it shows beside names; `Some(None)` takes picture and emoji
    /// away. Needs [`Capabilities::role_styles`] and a server whose
    /// options allow role icons.
    pub icon: Option<Option<RoleIcon>>,
    /// The colors after its own for a gradient: none, or one or two. Needs
    /// [`Capabilities::role_styles`] and a server that allows gradients.
    pub gradient: Option<Vec<Rgb>>,
}

/// Changes to the account's profile; `None` leaves a field as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfileEdit {
    pub display_name: Option<Option<String>>,
    pub pronouns: Option<Option<String>>,
    pub about: Option<Option<String>>,
    pub accent: Option<Rgb>,
    /// A new picture, or `Some(None)` to take it away; needs
    /// [`Capabilities::profile_images`].
    pub avatar: Option<Option<ProfileImage>>,
    pub banner: Option<Option<ProfileImage>>,
}

/// What a create command made.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Created {
    Server(ServerId),
    Channel(ChannelId),
    Role(ServerId, RoleId),
    // Administration (19).
    Expression(ServerId, ExpressionId),
}

/// How the connection to the service is.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ConnectionState {
    #[default]
    Connecting,
    Connected,
    /// Lost; trying again in `retry_in_ms`.
    Reconnecting {
        attempt: u32,
        retry_in_ms: u64,
    },
    /// Given up, for `reason`.
    Disconnected {
        reason: String,
    },
}

/// Interactive authorization, independent of a provider's transport.
/// Credentials never cross this boundary or enter persisted snapshots.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum AuthenticationState {
    /// This provider does not have an interactive authorization flow.
    #[default]
    Unavailable,
    SignedOut,
    /// A public authorization URL, not a callback URL or a credential.
    AwaitingBrowser {
        url: String,
    },
    /// Login is pending in a separate window; no credential crosses the UI boundary.
    AwaitingExternal,
    Exchanging,
    SignedIn {
        scopes: Vec<String>,
    },
    /// A user-facing, redacted error, without response bodies or credentials.
    Failed {
        reason: String,
    },
}

/// What the account has to start with, sent when a session is ready. The
/// local store keeps the last one, so the next start shows it at once.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub me: Option<User>,
    pub users: Vec<User>,
    pub servers: Vec<Server>,
    pub channels: Vec<Channel>,
    pub members: Vec<Member>,
    pub presences: Vec<(UserId, Presence)>,
    pub voice: Vec<VoiceState>,
    /// The newest message of each channel that has one.
    pub last_messages: Vec<(ChannelId, MessageId)>,
    /// The last message read in a channel, and how many mentions of the
    /// account came after it.
    pub read_states: Vec<(ChannelId, Option<MessageId>, u32)>,
    /// Monotonic provider versions for `read_states`. Missing entries are
    /// unversioned baselines and cannot replace newer local choices.
    #[serde(default)]
    pub read_versions: Vec<(ChannelId, u64)>,
    /// Exact first-unread IDs when known by the provider. Consumers must not
    /// manufacture an ID by adding to the cursor when this metadata is absent.
    #[serde(default)]
    pub first_unread: Vec<(ChannelId, MessageId)>,
    /// Friends, requests and blocks, one per person. `None` when the provider
    /// did not report them: that is not an empty list.
    #[serde(default)]
    pub relationships: Option<Vec<Relationship>>,
}

/// The command a failure belongs to, so the state can undo or mark what
/// that command changed ahead of the answer.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Failure {
    Invitation {
        request: RequestId,
        destination: InvitationDestination,
    },
    Send {
        channel: ChannelId,
        nonce: u64,
    },
    SendFiles {
        request: RequestId,
        channel: ChannelId,
        nonce: u64,
    },
    Edit {
        channel: ChannelId,
        message: MessageId,
    },
    Delete {
        channel: ChannelId,
        message: MessageId,
    },
    React {
        channel: ChannelId,
        message: MessageId,
        emoji: String,
    },
    Vote {
        channel: ChannelId,
        message: MessageId,
    },
    History {
        request: RequestId,
        channel: ChannelId,
    },
    Context {
        request: RequestId,
        channel: ChannelId,
    },
    ReadState {
        request: RequestId,
        channel: ChannelId,
    },
    Voice {
        channel: Option<ChannelId>,
    },
    /// Watching someone's stream.
    Stream {
        channel: ChannelId,
        owner: UserId,
    },
    /// Sharing the account's screen.
    Share {
        channel: ChannelId,
    },
    /// Turning the account's camera on or off in its call.
    Camera {
        channel: ChannelId,
    },
    Status,
    /// Creating, changing or leaving a server, or loading its bans.
    Server {
        server: Option<ServerId>,
    },
    Channel {
        channel: Option<ChannelId>,
    },
    Role {
        server: ServerId,
    },
    /// Roles, name, kick or ban of a member.
    Member {
        server: ServerId,
        user: UserId,
    },
    /// Revoking an invitation.
    Invite {
        server: ServerId,
        code: String,
    },
    /// Renaming or deleting a custom expression.
    Expression {
        server: ServerId,
        expression: ExpressionId,
    },
    Profile,
    /// Loading someone's profile.
    ProfileLoad {
        request: RequestId,
        user: UserId,
    },
    /// Opening the link of an activity's button.
    ActivityButton {
        request: RequestId,
    },
    /// Pinning or unpinning a message.
    Pin {
        channel: ChannelId,
        message: MessageId,
    },
    /// Loading a channel's pinned messages.
    Pins {
        request: RequestId,
        channel: ChannelId,
    },
    /// A friend request, an answer to one, a removal or a block; `None` for
    /// a request sent by handle.
    Relationship {
        user: Option<UserId>,
    },
    /// Opening a direct conversation with `user`.
    Direct {
        request: RequestId,
        user: UserId,
    },
    /// Renaming, changing the people or the picture of, or leaving a group
    /// conversation.
    Group {
        channel: ChannelId,
    },
    Other,
    /// Making a group conversation.
    NewGroup {
        request: RequestId,
    },
    /// Taking a direct conversation off the list.
    Close {
        channel: ChannelId,
    },
    // Threads and forums.
    /// Starting a thread or a forum post.
    NewThread {
        request: RequestId,
    },
    /// Following, changing or deleting a thread.
    Thread {
        thread: ChannelId,
    },
    /// Reading a page of a channel's threads.
    ThreadList {
        request: RequestId,
        parent: ChannelId,
    },
    /// Raising a hand, inviting to speak or going to the audience on a stage.
    Stage {
        channel: ChannelId,
    },
    // Profiles.
    /// Changing the account's profile in a server.
    ServerProfile {
        server: ServerId,
    },
    // Administration (19).
    /// Making an invitation.
    NewInvite {
        request: RequestId,
        server: ServerId,
    },
    /// Adding an emoji or a sticker.
    NewExpression {
        request: RequestId,
        server: ServerId,
    },
    /// Counting or removing inactive members.
    Prune {
        request: RequestId,
        server: ServerId,
    },
    /// Making, changing or deleting a webhook; `None` while making one.
    Webhook {
        server: ServerId,
        webhook: Option<WebhookId>,
    },
    /// Removing an integration.
    Integration {
        server: ServerId,
        integration: IntegrationId,
    },
    /// Reading or changing the GIF favorites the provider keeps.
    GifFavorites,
    // The account's synced settings (P1).
    /// Reading the account's status, folders and privacy again.
    AccountSettings,
    /// Changing how a server, or the direct conversations, notify.
    Notifications {
        server: Option<ServerId>,
    },
    Folders,
    Privacy,
    /// Reading or keeping the account's private note on someone.
    Note {
        user: UserId,
    },
    // Reads, search and remote members (P2).
    /// Holding a conversation unread at the provider.
    Unread {
        channel: ChannelId,
    },
    /// Marking a whole server read.
    ServerRead {
        server: ServerId,
    },
    /// A search at the provider.
    Search {
        request: RequestId,
    },
    // Commands and interactions (P3).
    /// Reading the commands a conversation offers.
    AppCommands {
        request: RequestId,
        channel: ChannelId,
    },
    /// Using an app's button, menu, modal or command.
    Interaction {
        request: RequestId,
        channel: ChannelId,
    },
    // Message actions and moderation (P4).
    /// Forwarding a message to `to`.
    Forward {
        to: ChannelId,
        nonce: u64,
    },
    /// Starting a poll.
    NewPoll {
        channel: ChannelId,
        nonce: u64,
    },
    /// Ending a poll early.
    EndPoll {
        channel: ChannelId,
        message: MessageId,
    },
    /// Reading who reacted or voted.
    People {
        request: RequestId,
    },
    /// Reading a conversation a link names.
    Place {
        request: RequestId,
    },
    // Presence, identity and expressions (P5).
    /// Reading the standard sticker packs.
    StickerPacks,
    /// A search of the service's GIFs.
    Gifs {
        request: RequestId,
    },
    // Server administration, rest (P7).
    /// Reading a server's profile.
    ServerLoad {
        request: RequestId,
        server: ServerId,
    },
    /// Reordering a server's channels.
    ChannelOrder {
        server: ServerId,
    },
    // Private calls.
    /// Ringing someone to a call, or stopping it.
    Ring {
        channel: ChannelId,
    },
    // Calls and the voice panel (llamadas-y-panel-de-voz).
    /// Asking whether a direct call may ring. `refused`: the service
    /// answered that it may not be made; otherwise its answer did not come
    /// (no connection, a limit, a failure of its own).
    CallCheck {
        channel: ChannelId,
        refused: bool,
    },
    // Rules and onboarding (2026-10-18).
    /// Reading or accepting a server's rules.
    Rules {
        request: RequestId,
        server: ServerId,
    },
    /// Reading or answering a server's onboarding.
    Onboarding {
        request: RequestId,
        server: ServerId,
    },
    /// Saving a server's onboarding.
    OnboardingEdit {
        request: RequestId,
        server: ServerId,
    },
    // Desktop session and games (escritorio-sesion-y-juegos).
    /// Changing whether the account shows its activity.
    ActivitySharing,
    // Scheduled events (eventos-programados).
    /// Reading a server's events or who an event interests.
    ScheduledEvents {
        request: RequestId,
        server: ServerId,
    },
    /// Planning, changing, starting, ending or deleting an event.
    ScheduledEvent {
        request: RequestId,
        server: ServerId,
    },
    /// Saying whether an event interests the account.
    EventInterest {
        request: RequestId,
        server: ServerId,
    },
    // AutoMod (automod).
    /// Reading a server's AutoMod rules.
    AutoModRules {
        request: RequestId,
        server: ServerId,
    },
    /// Saving an AutoMod rule, new or changed.
    AutoModSave {
        request: RequestId,
        server: ServerId,
    },
    /// Turning an AutoMod rule on or off, or deleting it.
    AutoModRule {
        server: ServerId,
        rule: rune_model::AutoModRuleId,
    },
}

impl Failure {
    /// The failure of `command`: for one that could not be queued, or that
    /// never got an answer. `None` for commands nobody waits on.
    pub fn of(command: &Command) -> Option<Self> {
        Some(match command {
            Command::BeginSignIn
            | Command::CancelSignIn
            | Command::SignOut
            | Command::ForgetSession
            | Command::UseAccount(_)
            | Command::ForgetAccount(_) => Self::Other,
            Command::AcceptInvitation {
                request,
                destination,
                ..
            }
            | Command::VerifyInvitation {
                request,
                destination,
                ..
            } => Self::Invitation {
                request: *request,
                destination: *destination,
            },
            Command::SendMessage { channel, nonce, .. }
            | Command::SendSticker { channel, nonce, .. } => Self::Send {
                channel: *channel,
                nonce: *nonce,
            },
            Command::SendFiles { scope, .. } => Self::SendFiles {
                request: scope.request,
                channel: scope.channel,
                nonce: scope.nonce,
            },
            Command::EditMessage {
                channel, message, ..
            } => Self::Edit {
                channel: *channel,
                message: *message,
            },
            Command::DeleteMessage { channel, message } => Self::Delete {
                channel: *channel,
                message: *message,
            },
            Command::ToggleReaction {
                channel,
                message,
                emoji,
                ..
            } => Self::React {
                channel: *channel,
                message: *message,
                emoji: emoji.clone(),
            },
            Command::VotePoll {
                channel, message, ..
            } => Self::Vote {
                channel: *channel,
                message: *message,
            },
            Command::LoadHistory {
                request, channel, ..
            } => Self::History {
                request: *request,
                channel: *channel,
            },
            Command::LoadContext {
                request, channel, ..
            } => Self::Context {
                request: *request,
                channel: *channel,
            },
            Command::MarkRead {
                request, channel, ..
            } => Self::ReadState {
                request: *request,
                channel: *channel,
            },
            Command::JoinVoice { channel, .. } => Self::Voice {
                channel: Some(*channel),
            },
            Command::LeaveVoice | Command::SetVoiceSelf { .. } => Self::Voice { channel: None },
            Command::WatchStream { channel, owner } | Command::StopWatching { channel, owner } => {
                Self::Stream {
                    channel: *channel,
                    owner: *owner,
                }
            }
            Command::StartStream { channel, .. }
            | Command::StopStream { channel }
            | Command::PauseStream { channel, .. } => Self::Share { channel: *channel },
            Command::SetCamera { channel, .. } => Self::Camera { channel: *channel },
            Command::SetStatus(_) | Command::SetCustomStatus { .. } => Self::Status,
            Command::CreateServer { .. } => Self::Server { server: None },
            Command::EditServer { server, .. }
            | Command::DeleteServer { server }
            | Command::LeaveServer { server }
            | Command::LoadBans { server, .. }
            | Command::LoadInvites { server, .. }
            | Command::LoadAudit { server, .. } => Self::Server {
                server: Some(*server),
            },
            Command::CreateChannel { .. } => Self::Channel { channel: None },
            Command::EditChannel { channel, .. }
            | Command::DeleteChannel { channel }
            | Command::SetOverwrite { channel, .. }
            | Command::RemoveOverwrite { channel, .. } => Self::Channel {
                channel: Some(*channel),
            },
            Command::CreateRole { server, .. }
            | Command::EditRole { server, .. }
            | Command::DeleteRole { server, .. } => Self::Role { server: *server },
            Command::SetMemberRoles { server, user, .. }
            | Command::SetNick { server, user, .. }
            | Command::Kick { server, user, .. }
            | Command::Ban { server, user, .. }
            | Command::Unban { server, user } => Self::Member {
                server: *server,
                user: *user,
            },
            Command::RevokeInvite { server, code } => Self::Invite {
                server: *server,
                code: code.clone(),
            },
            Command::RenameExpression {
                server, expression, ..
            }
            | Command::DeleteExpression { server, expression } => Self::Expression {
                server: *server,
                expression: *expression,
            },
            Command::EditProfile(_) => Self::Profile,
            Command::LoadProfile { request, user, .. } => Self::ProfileLoad {
                request: *request,
                user: *user,
            },
            Command::OpenActivityButton { request, .. } => Self::ActivityButton {
                request: *request,
            },
            Command::PinMessage { channel, message }
            | Command::UnpinMessage { channel, message } => Self::Pin {
                channel: *channel,
                message: *message,
            },
            Command::LoadPins { request, channel } => Self::Pins {
                request: *request,
                channel: *channel,
            },
            Command::SendFriendRequest { .. } => Self::Relationship { user: None },
            Command::AcceptFriendRequest { user }
            | Command::RemoveRelationship { user }
            | Command::BlockUser { user }
            | Command::IgnoreUser { user }
            | Command::UnignoreUser { user } => Self::Relationship { user: Some(*user) },
            Command::OpenDirect { request, user } => Self::Direct {
                request: *request,
                user: *user,
            },
            Command::RenameGroup { channel, .. }
            | Command::AddGroupMember { channel, .. }
            | Command::RemoveGroupMember { channel, .. }
            | Command::LeaveGroup { channel }
            | Command::SetGroupIcon { channel, .. }
            | Command::SetGroupOwner { channel, .. } => Self::Group { channel: *channel },
            Command::CreateGroup { request, .. } => Self::NewGroup { request: *request },
            Command::CloseDirect { channel } => Self::Close { channel: *channel },
            Command::SetFriendNickname { user, .. } => Self::Relationship { user: Some(*user) },
            Command::CreateThread { request, .. } | Command::CreatePost { request, .. } => {
                Self::NewThread { request: *request }
            }
            Command::FollowThread { thread, .. }
            | Command::EditThread { thread, .. }
            | Command::DeleteThread { thread }
            // Threads, forums and stages (2026-10-18).
            | Command::MuteThread { thread, .. } => Self::Thread { thread: *thread },
            Command::LoadThreads {
                request, parent, ..
            } => Self::ThreadList {
                request: *request,
                parent: *parent,
            },
            Command::RaiseHand { channel, .. } | Command::SetSpeaker { channel, .. } => {
                Self::Stage { channel: *channel }
            }
            Command::EditServerProfile { server, .. } => Self::ServerProfile { server: *server },
            Command::TimeoutMember { server, user, .. } => Self::Member {
                server: *server,
                user: *user,
            },
            Command::CreateInvite {
                request, server, ..
            } => Self::NewInvite {
                request: *request,
                server: *server,
            },
            Command::CreateExpression {
                request, server, ..
            } => Self::NewExpression {
                request: *request,
                server: *server,
            },
            Command::PruneMembers {
                request, server, ..
            } => Self::Prune {
                request: *request,
                server: *server,
            },
            Command::LoadMembers { server, .. }
            | Command::LoadWebhooks { server, .. }
            | Command::LoadIntegrations { server, .. } => Self::Server {
                server: Some(*server),
            },
            Command::CreateWebhook { server, .. } => Self::Webhook {
                server: *server,
                webhook: None,
            },
            Command::EditWebhook {
                server, webhook, ..
            }
            | Command::DeleteWebhook { server, webhook } => Self::Webhook {
                server: *server,
                webhook: Some(*webhook),
            },
            Command::DeleteIntegration {
                server,
                integration,
            } => Self::Integration {
                server: *server,
                integration: *integration,
            },
            Command::LoadGifFavorites | Command::SetGifFavorite { .. } => Self::GifFavorites,
            Command::LoadAccountSettings => Self::AccountSettings,
            Command::SetNotifications { server, .. } => Self::Notifications { server: *server },
            Command::SetServerFolders(_) => Self::Folders,
            Command::SetMessagingPrivacy(_) => Self::Privacy,
            Command::LoadNote { user } | Command::SetNote { user, .. } => {
                Self::Note { user: *user }
            }
            Command::MarkUnread { channel, .. } => Self::Unread { channel: *channel },
            Command::MarkServerRead { server, .. } => Self::ServerRead { server: *server },
            Command::SearchMessages { request, .. } => Self::Search { request: *request },
            Command::LoadAppCommands { request, channel } => Self::AppCommands {
                request: *request,
                channel: *channel,
            },
            Command::Interact {
                request, channel, ..
            } => Self::Interaction {
                request: *request,
                channel: *channel,
            },
            Command::ForwardMessage { to, nonce, .. } => Self::Forward {
                to: *to,
                nonce: *nonce,
            },
            Command::CreatePoll { channel, nonce, .. } => Self::NewPoll {
                channel: *channel,
                nonce: *nonce,
            },
            Command::EndPoll { channel, message } => Self::EndPoll {
                channel: *channel,
                message: *message,
            },
            Command::LoadReactions { request, .. } | Command::LoadPollVoters { request, .. } => {
                Self::People { request: *request }
            }
            Command::LoadChannel { request, .. } => Self::Place { request: *request },
            Command::ModerateVoice { server, user, .. } => Self::Member {
                server: *server,
                user: *user,
            },
            Command::LoadStickerPacks => Self::StickerPacks,
            Command::SearchGifs { request, .. } => Self::Gifs { request: *request },
            // Server administration, rest (P7).
            Command::LoadServerProfile { request, server } => Self::ServerLoad {
                request: *request,
                server: *server,
            },
            Command::ReorderChannels { server, .. } => Self::ChannelOrder { server: *server },
            // Private calls.
            Command::RingCall { channel, .. } | Command::StopRinging { channel, .. } => {
                Self::Ring { channel: *channel }
            }
            // Calls and the voice panel (llamadas-y-panel-de-voz).
            Command::CheckCall { channel } => Self::CallCheck {
                channel: *channel,
                refused: false,
            },
            // Rules and onboarding (2026-10-18).
            Command::LoadServerRules { request, server }
            | Command::AnswerServerRules {
                request, server, ..
            } => Self::Rules {
                request: *request,
                server: *server,
            },
            Command::LoadOnboarding { request, server }
            | Command::AnswerOnboarding {
                request, server, ..
            } => Self::Onboarding {
                request: *request,
                server: *server,
            },
            Command::SaveOnboarding {
                request, server, ..
            } => Self::OnboardingEdit {
                request: *request,
                server: *server,
            },
            // Desktop session and games (escritorio-sesion-y-juegos).
            Command::SetActivitySharing(_) => Self::ActivitySharing,
            // Scheduled events (eventos-programados).
            Command::LoadScheduledEvents { request, server }
            | Command::LoadEventInterested {
                request, server, ..
            } => Self::ScheduledEvents {
                request: *request,
                server: *server,
            },
            Command::CreateScheduledEvent {
                request, server, ..
            }
            | Command::EditScheduledEvent {
                request, server, ..
            }
            | Command::SetScheduledEventStatus {
                request, server, ..
            }
            | Command::DeleteScheduledEvent {
                request, server, ..
            } => Self::ScheduledEvent {
                request: *request,
                server: *server,
            },
            Command::SetEventInterest {
                request, server, ..
            } => Self::EventInterest {
                request: *request,
                server: *server,
            },
            // AutoMod (automod).
            Command::LoadAutoModRules { request, server } => Self::AutoModRules {
                request: *request,
                server: *server,
            },
            Command::SaveAutoModRule {
                request, server, ..
            } => Self::AutoModSave {
                request: *request,
                server: *server,
            },
            Command::SetAutoModRuleEnabled { server, rule, .. }
            | Command::DeleteAutoModRule { server, rule } => Self::AutoModRule {
                server: *server,
                rule: *rule,
            },
            Command::SuggestChoices { .. }
            | Command::SuggestGifs { .. }
            | Command::Cancel(_)
            | Command::SearchMembers { .. }
            | Command::SolveChallenge { .. }
            | Command::StartTyping { .. }
            | Command::WatchMemberList { .. }
            | Command::UnwatchMemberList
            | Command::LoadStreamPreview { .. }
            // Streams (transmisiones).
            | Command::UploadStreamPreview { .. }
            // Desktop, language and local voice (P6).
            | Command::SetLocalActivity(_)
            | Command::LoadDetectableGames
            | Command::VoiceVerificationCode { .. }
            // Server administration, rest (P7).
            | Command::LoadRoleCounts { .. }
            // A real account's first cards (2026-10-17).
            | Command::WatchPeople { .. }
            // The opening screen (2026-10-18).
            | Command::ReconnectNow
            // Voice protocol (voz-protocolo).
            | Command::WatchServer(_) => {
                return None;
            }
        })
    }

    /// What was being done, as the interface says it: t("enviar el mensaje").
    pub fn action(&self) -> &'static str {
        match self {
            Self::Invitation { .. } => t("aceptar la invitación"),
            Self::Send { .. } | Self::SendFiles { .. } => t("enviar el mensaje"),
            Self::Edit { .. } => t("editar el mensaje"),
            Self::Delete { .. } => t("borrar el mensaje"),
            Self::React { .. } => t("reaccionar"),
            Self::Vote { .. } => t("votar en la encuesta"),
            Self::History { .. } => t("cargar el historial"),
            Self::Context { .. } => t("abrir el contexto del mensaje"),
            Self::ReadState { .. } => t("confirmar la lectura"),
            Self::Voice { .. } => t("conectar la voz"),
            Self::Stream { .. } => t("ver la transmisión"),
            Self::Share { .. } => t("compartir la pantalla"),
            Self::Camera { .. } => t("usar la cámara"),
            Self::Status => t("cambiar el estado"),
            Self::Server { .. } => t("cambiar el servidor"),
            Self::Channel { .. } => t("cambiar el canal"),
            Self::Role { .. } => t("cambiar los roles"),
            Self::Member { .. } => t("cambiar al miembro"),
            Self::Invite { .. } => t("revocar la invitación"),
            Self::Expression { .. } => t("cambiar la expresión"),
            Self::Profile => t("cambiar el perfil"),
            Self::ProfileLoad { .. } => t("cargar el perfil"),
            Self::ActivityButton { .. } => t("abrir el enlace de la actividad"),
            Self::Pin { .. } => t("cambiar los mensajes fijados"),
            Self::Pins { .. } => t("cargar los mensajes fijados"),
            Self::Relationship { .. } => t("cambiar la relación"),
            Self::Direct { .. } => t("abrir la conversación"),
            Self::Group { .. } => t("cambiar el grupo"),
            Self::Other => t("completar la acción"),
            Self::NewGroup { .. } => t("crear el grupo"),
            Self::Close { .. } => t("cerrar la conversación"),
            Self::NewThread { .. } => t("crear el hilo"),
            Self::Thread { .. } => t("cambiar el hilo"),
            Self::ThreadList { .. } => t("cargar los hilos"),
            Self::Stage { .. } => t("cambiar tu lugar en el escenario"),
            Self::ServerProfile { .. } => t("cambiar tu perfil del servidor"),
            Self::NewInvite { .. } => t("crear la invitación"),
            Self::NewExpression { .. } => t("agregar la expresión"),
            Self::Prune { .. } => t("podar a los miembros inactivos"),
            Self::Webhook { .. } => t("cambiar el webhook"),
            Self::Integration { .. } => t("quitar la integración"),
            Self::GifFavorites => t("guardar los GIF favoritos"),
            Self::AccountSettings => t("leer los ajustes de la cuenta"),
            Self::Notifications { .. } => t("cambiar las notificaciones"),
            Self::Folders => t("cambiar las carpetas de servidores"),
            Self::Privacy => t("cambiar la privacidad"),
            Self::Note { .. } => t("guardar la nota"),
            Self::Unread { .. } => t("marcar como no leído"),
            Self::ServerRead { .. } => t("marcar el servidor como leído"),
            Self::Search { .. } => t("buscar mensajes"),
            Self::AppCommands { .. } => t("cargar los comandos"),
            Self::Interaction { .. } => t("usar la aplicación"),
            Self::Forward { .. } => t("reenviar el mensaje"),
            Self::NewPoll { .. } => t("crear la encuesta"),
            Self::EndPoll { .. } => t("terminar la encuesta"),
            Self::People { .. } => t("cargar la lista de personas"),
            Self::Place { .. } => t("abrir el enlace"),
            Self::StickerPacks => t("cargar los paquetes de stickers"),
            Self::Gifs { .. } => t("buscar GIF"),
            Self::ServerLoad { .. } => t("cargar el perfil del servidor"),
            Self::ChannelOrder { .. } => t("ordenar los canales"),
            Self::Ring { .. } => t("hacer sonar la llamada"),
            Self::CallCheck { .. } => t("preparar la llamada"),
            // Rules and onboarding (2026-10-18).
            Self::Rules { .. } => t("completar las reglas del servidor"),
            Self::Onboarding { .. } => t("completar las preguntas del servidor"),
            Self::OnboardingEdit { .. } => t("guardar la incorporación"),
            // Desktop session and games (escritorio-sesion-y-juegos).
            Self::ActivitySharing => t("cambiar si se comparte tu actividad"),
            // Scheduled events (eventos-programados).
            Self::ScheduledEvents { .. } => t("cargar los eventos"),
            Self::ScheduledEvent { .. } => t("cambiar el evento"),
            Self::EventInterest { .. } => t("marcar tu interés en el evento"),
            // AutoMod (automod).
            Self::AutoModRules { .. } => t("cargar las reglas de AutoMod"),
            Self::AutoModSave { .. } => t("guardar la regla de AutoMod"),
            Self::AutoModRule { .. } => t("cambiar la regla de AutoMod"),
        }
    }
}

/// Something that happened at the service.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Authentication(AuthenticationState),
    /// Whether this device remembers the current session for the next start
    /// (for example, in the system keyring). Never carries the credential.
    SessionRemembered(bool),
    /// The accounts this device remembers, the most recently used first.
    /// Sent at start and after each change; never carries a credential.
    RememberedAccounts(Vec<RememberedAccount>),
    Ready(Box<Snapshot>),
    Connection(ConnectionState),
    /// Body consumption only; completion still requires MessageCreated.
    UploadProgress {
        scope: UploadSendScope,
        sent_bytes: u64,
        total_bytes: u64,
    },
    /// Provider-confirmed inclusive cursor. Versions are positive and increase
    /// per account/channel. A correlated answer must cover its requested target.
    /// `None` is an unsolicited update (for example, another device).
    ReadStateConfirmed {
        account: UserId,
        request: Option<RequestId>,
        channel: ChannelId,
        message: MessageId,
        version: u64,
    },
    /// The acceptance attempt finished, independently of membership delivery.
    /// Success never creates a server, channels, roles or permissions locally.
    /// A host deadline while the provider may still be working is instead
    /// reported as an ambiguous [`Event::Failed`].
    InvitationAcceptance {
        request: RequestId,
        destination: InvitationDestination,
        result: Result<(), InvitationJoinError>,
    },
    /// `nonce` is the one of the command that sent it, when it was sent from here.
    MessageCreated {
        message: Message,
        nonce: Option<u64>,
    },
    MessageUpdated(Message),
    /// Confirmed partial fields for an existing message. Consumers preserve
    /// absent fields and never reconstruct an unknown message from this event.
    MessagePatched(MessagePatch),
    MessageDeleted {
        channel: ChannelId,
        message: MessageId,
    },
    /// One account added (`added`) or took back `emoji` on a message. A
    /// change, not a total: a provider forwards each one once, and `me`
    /// marks the current account, whose own change applies only once.
    ReactionChanged {
        channel: ChannelId,
        message: MessageId,
        emoji: String,
        me: bool,
        added: bool,
    },
    /// Every reaction of a message, or only `emoji`, was taken away.
    ReactionsCleared {
        channel: ChannelId,
        message: MessageId,
        emoji: Option<String>,
    },
    /// One account added or took back its vote for `answer` of the poll of
    /// a message; a change applied like [`Event::ReactionChanged`].
    PollVoteChanged {
        channel: ChannelId,
        message: MessageId,
        answer: u32,
        me: bool,
        added: bool,
    },
    /// The answer to a `LoadHistory`, oldest first. `reached_start` says
    /// there is nothing older.
    HistoryLoaded {
        request: RequestId,
        channel: ChannelId,
        before: Option<MessageId>,
        messages: Vec<Message>,
        reached_start: bool,
    },
    Typing {
        channel: ChannelId,
        user: UserId,
    },
    PresenceChanged {
        user: UserId,
        presence: Presence,
    },
    VoiceChanged(VoiceState),
    VoiceLeft {
        user: UserId,
        channel: ChannelId,
    },
    /// Someone started or stopped speaking. Sent as a latest value: see
    /// [`EventSink::send_latest`].
    Speaking {
        channel: ChannelId,
        user: UserId,
        speaking: bool,
    },
    /// The account's call reached `phase`, with the connection's quality
    /// when the provider measures it.
    CallPhase {
        channel: ChannelId,
        phase: CallPhase,
        quality: Option<CallQuality>,
    },
    /// The account's call ended; `reason` when it was not the account's doing.
    CallEnded {
        channel: ChannelId,
        reason: Option<String>,
    },
    /// The stream of `owner` the account watches reached `phase`; with the
    /// account as `owner`, its own shared screen.
    StreamPhase {
        channel: ChannelId,
        owner: UserId,
        phase: StreamPhase,
    },
    /// The command `failure` names could not be carried out. `ambiguous`
    /// says it may have happened anyway: it left, but no answer came back.
    /// The core reports these too, for a command that could not be queued
    /// or whose answer never arrived.
    Failed {
        failure: Failure,
        reason: String,
        ambiguous: bool,
    },
    /// A server is new or changed, roles and all.
    ServerChanged(Server),
    /// The account is no longer in a server: it left, was removed, or the
    /// server was deleted.
    ServerRemoved {
        server: ServerId,
        reason: Option<String>,
    },
    /// A channel is new or changed, with what the account may do in it.
    ChannelChanged(Channel),
    ChannelRemoved {
        channel: ChannelId,
    },
    MemberChanged(Member),
    /// Someone left or was removed from a server.
    MemberRemoved {
        server: ServerId,
        user: UserId,
    },
    UserChanged(User),
    /// A page of bans, by ascending user id after `after`, as asked for.
    /// `reached_end` says there are no more.
    BansLoaded {
        request: RequestId,
        server: ServerId,
        after: Option<UserId>,
        bans: Vec<Ban>,
        reached_end: bool,
    },
    /// Someone was banned, here or elsewhere.
    BanAdded {
        server: ServerId,
        ban: Ban,
    },
    BanRemoved {
        server: ServerId,
        user: UserId,
    },
    /// A page of the audit log matching `query`, newest first, older than
    /// `before`. `reached_end` says there are no older entries.
    AuditLoaded {
        request: RequestId,
        server: ServerId,
        query: AuditQuery,
        before: Option<u64>,
        entries: Vec<AuditEntry>,
        reached_end: bool,
    },
    /// Every invitation of `server`.
    InvitesLoaded {
        request: RequestId,
        server: ServerId,
        invites: Vec<Invite>,
    },
    InviteRemoved {
        server: ServerId,
        code: String,
    },
    /// The answer to a create command, after the change events that made it.
    Created {
        request: RequestId,
        created: Created,
    },
    /// The answer to a `LoadProfile`. Its scope must match the request's.
    ProfileLoaded {
        request: RequestId,
        profile: rune_model::UserProfile,
    },
    /// The link of the button asked for with [`Command::OpenActivityButton`].
    /// It is the app's own address: the interface opens it only through the
    /// link flow, which asks first.
    ActivityButtonUrl {
        request: RequestId,
        url: String,
    },
    /// The watched member list as it is now. The people in it arrive first,
    /// as [`Event::UserChanged`] and [`Event::MemberChanged`].
    MemberListChanged(rune_model::MemberList),
    /// The answer to a `LoadPins`: the pinned messages of `channel`, the
    /// newest first.
    PinsLoaded {
        request: RequestId,
        channel: ChannelId,
        messages: Vec<Message>,
    },
    /// The exact anchor and its neighbors, oldest first. This is not a
    /// history page and carries no claim of continuity or latest position.
    ContextLoaded {
        request: RequestId,
        channel: ChannelId,
        anchor: MessageId,
        messages: Vec<Message>,
    },
    /// The account's relationship with someone is new or changed.
    RelationshipChanged(Relationship),
    /// The account no longer has a relationship with `user`.
    RelationshipRemoved {
        user: UserId,
    },
    // Video, phase 5.
    /// The latest picture of `owner`'s stream in `channel`; `None` when
    /// there is none to show (yet, or any more).
    StreamPreview {
        channel: ChannelId,
        owner: UserId,
        image: Option<RemoteImage>,
        /// The service asked not to be asked again for so long (a 429), when
        /// there is no picture because of it.
        retry_after: Option<std::time::Duration>,
    },
    // Attachments and platform (07/08).
    /// The largest file the account may attach in `server`'s conversations,
    /// or outside servers for `None`, as the service sets it for this
    /// account. A server without its own value takes the account's. Never
    /// above [`Provider::upload_limits`], which still bounds every message.
    UploadLimit {
        server: Option<ServerId>,
        max_file_bytes: u64,
    },
    // Threads and forums.
    /// The page of threads asked for with `request`. The threads
    /// it names that were not known yet arrive first, as
    /// [`Event::ChannelChanged`]; a thread known already keeps what was
    /// reported of it. `next` asks for the page after this one; `None` when
    /// `scope` has no more.
    ThreadsLoaded {
        request: RequestId,
        parent: ChannelId,
        scope: ThreadScope,
        threads: Vec<ChannelId>,
        next: Option<String>,
    },
    // Profiles.
    /// The account's own profile as saved, in its scope: replaces what was
    /// loaded of it there. A field `None` was not reported and keeps its value.
    ProfileChanged(rune_model::UserProfile),
    // Verification of writes.
    /// The service asked for a verification a person must solve before the
    /// action `failure` names can go out. The action waits, unanswered, for
    /// [`Command::SolveChallenge`] with `challenge`, up to
    /// [`CHALLENGE_LIFETIME`]; then it ends as any other: with its own
    /// answer, or [`Event::Failed`] for `failure`. One at a time.
    ChallengeRequired {
        challenge: RequestId,
        failure: Failure,
    },
    // Administration (19).
    /// The invitation made with `request`.
    InviteCreated {
        request: RequestId,
        server: ServerId,
        invite: Invite,
    },
    /// The page of `server`'s members asked for with `request`; their
    /// accounts arrive first, as [`Event::UserChanged`]. `next` asks for the
    /// page after this one; `None` at the end. `total` is how many members
    /// the server has, when the provider says.
    MembersLoaded {
        request: RequestId,
        server: ServerId,
        after: Option<String>,
        members: Vec<Member>,
        next: Option<String>,
        total: Option<u32>,
        /// As asked for; `members` come in its order.
        query: rune_model::MemberQuery,
        /// How each of them joined, when the provider says.
        joins: Vec<rune_model::JoinedBy>,
    },
    /// How many members a prune of `days` removes (`removed: false`), or
    /// removed (`true`); `None` when the provider did not count them.
    Pruned {
        request: RequestId,
        server: ServerId,
        days: u32,
        count: Option<u32>,
        removed: bool,
    },
    /// Every webhook of `server`.
    WebhooksLoaded {
        request: RequestId,
        server: ServerId,
        webhooks: Vec<Webhook>,
    },
    /// A webhook is new or changed.
    WebhookChanged {
        server: ServerId,
        webhook: Webhook,
    },
    WebhookRemoved {
        server: ServerId,
        webhook: WebhookId,
    },
    /// Every integration of `server`.
    IntegrationsLoaded {
        request: RequestId,
        server: ServerId,
        integrations: Vec<Integration>,
    },
    IntegrationRemoved {
        server: ServerId,
        integration: IntegrationId,
    },
    /// The GIF favorites the provider keeps for the account, as they are
    /// now, the first added first.
    GifFavorites(Vec<FavoriteGif>),
    // The account's synced settings (P1).
    /// How servers and the direct conversations (`server: None`) notify:
    /// with `replace`, all of them, and whatever is not listed is back to
    /// its default; else only those listed change.
    NotificationSettings {
        settings: Vec<Notifications>,
        replace: bool,
    },
    /// The rail's layout as the account keeps it. Servers it does not name
    /// go first, the newest first, as the service shows them.
    ServerFolders(Vec<ServerFolder>),
    MessagingPrivacy(MessagingPrivacy),
    /// The account's private note on `user`; empty when there is none.
    NoteChanged {
        user: UserId,
        note: String,
    },
    // Reads, search and remote members (P2).
    /// `channel` is held unread by hand from its first message after
    /// `through` (from its start with `None`), here or on another of the
    /// account's devices; `anchor` is that first message when the provider
    /// knows it, and `mentions` the account's mentions left unread.
    /// `version` as in [`Event::ReadStateConfirmed`].
    MarkedUnread {
        account: UserId,
        channel: ChannelId,
        through: Option<MessageId>,
        anchor: Option<MessageId>,
        mentions: u32,
        version: u64,
    },
    /// The newest message of channels the provider only summarizes, such
    /// as a large server's: they have something new to read, not what.
    LatestMessages(Vec<(ChannelId, MessageId)>),
    /// The page asked for with `request`; its authors arrive first, as
    /// [`Event::UserChanged`]. Its messages are not history: they join no
    /// conversation.
    SearchResults {
        request: RequestId,
        result: Result<rune_model::RemotePage, rune_model::RemoteSearchError>,
    },
    /// Members of `server` named like `query`; their accounts arrive first,
    /// as [`Event::UserChanged`].
    MembersFound {
        server: ServerId,
        query: String,
        members: Vec<Member>,
    },
    // Commands and interactions (P3).
    /// The apps' commands `channel` offers, asked for with `request`.
    AppCommands {
        request: RequestId,
        channel: ChannelId,
        commands: Vec<rune_model::interactions::AppCommand>,
    },
    /// The app got the interaction sent with `request` (`done: false`), or
    /// answered it (`done: true`). A refusal is [`Event::Failed`].
    Interaction {
        request: RequestId,
        done: bool,
    },
    /// The app opened `modal` in `channel`, in answer to the interaction
    /// sent with `request`.
    ModalOpened {
        request: RequestId,
        channel: ChannelId,
        modal: rune_model::interactions::Modal,
    },
    /// What the app suggests for the option being written, for `request`.
    CommandChoices {
        request: RequestId,
        choices: Vec<rune_model::interactions::OptionChoice>,
    },
    /// The commands of `server`, or of every conversation (`None`), changed
    /// at the service: a list kept of them is old.
    AppCommandsChanged {
        server: Option<ServerId>,
    },
    // Message actions and moderation (P4).
    /// The page of who reacted or voted asked for with `request`, by
    /// ascending id; their accounts arrive first, as [`Event::UserChanged`].
    /// `reached_end` says there are no more.
    PeopleLoaded {
        request: RequestId,
        people: Vec<UserId>,
        reached_end: bool,
    },
    /// The conversation asked for with `request`, reported first as
    /// [`Event::ChannelChanged`].
    ChannelLoaded {
        request: RequestId,
        channel: ChannelId,
    },
    // Desktop, language and local voice (P6).
    /// The games the provider can recognize, as asked for with
    /// [`Command::LoadDetectableGames`].
    DetectableGames(Vec<rune_model::DetectableGame>),
    /// The privacy code of the account's call in `channel`: the same for
    /// everyone in it while nobody else holds its keys. It changes when
    /// someone joins or leaves; `None` while it is not encrypted yet.
    CallPrivacyCode {
        channel: ChannelId,
        code: Option<String>,
    },
    /// The code the account and `user` compare, asked for with
    /// [`Command::VoiceVerificationCode`]; `None` when it cannot be known,
    /// such as when they left.
    VoiceVerification {
        channel: ChannelId,
        user: UserId,
        code: Option<String>,
    },
    // Presence, identity and expressions (P5).
    /// What the account may use beyond each conversation's own server, as
    /// its plan allows now. Sent after [`Event::Ready`] and when it changes.
    Perks(rune_model::Perks),
    /// The sticker packs the service offers everyone.
    StickerPacks(Vec<rune_model::StickerPack>),
    /// A page of the service's own GIFs, or why not.
    GifResults(GifResult),
    /// What to search for GIFs, for `query`.
    GifSuggestions {
        request: RequestId,
        query: String,
        terms: Vec<String>,
    },
    // Server administration, rest (P7).
    /// How many members have each role of `server` now; a role left out
    /// has none, or the provider did not count it.
    RoleCounts {
        server: ServerId,
        counts: Vec<(RoleId, u32)>,
    },
    // Private calls.
    /// A direct or group conversation has a call, ringing whom it names:
    /// replaces what was known of it. Who is in it arrives as
    /// [`Event::VoiceChanged`].
    PrivateCall(rune_model::PrivateCall),
    /// The call of the conversation `channel` is over, for everyone.
    PrivateCallEnded {
        channel: ChannelId,
    },
    // Sends and uploads (envios-y-subidas).
    /// The bytes of the files of the new post `request` handed over so far,
    /// as [`Event::UploadProgress`] counts a message's; the post is made
    /// only with [`Event::Created`].
    PostProgress {
        request: RequestId,
        sent_bytes: u64,
        total_bytes: u64,
    },
    // Encrypted local cache (2026-10-18).
    /// The account the local store may keep data of from now on, for a
    /// provider whose account is known only once it is verified: sent after
    /// the verification, before the session's [`Event::Ready`]. It lasts
    /// until the session ends; a `Ready` of another account still turns the
    /// store off. The core consumes it.
    StorageIdentity(StorageIdentity),
    /// The person forgot this account on this device: what the local store
    /// keeps of it, and its key, are deleted. Never sent when only switching
    /// accounts. The core consumes it.
    ForgetStorage(StorageIdentity),
    // Reactions, pins and search (2026-10-18).
    /// One account added or took back a super reaction with `emoji`, with
    /// the colors the service gave it; applied like
    /// [`Event::ReactionChanged`], but counted apart.
    BurstReactionChanged {
        channel: ChannelId,
        message: MessageId,
        emoji: String,
        me: bool,
        added: bool,
        colors: Vec<rune_model::Rgb>,
    },
    // Rail and notifications.
    /// An app launched in a voice channel and who uses it now: it replaces
    /// what was known of the same launch, and with nobody it ended.
    VoiceActivity(rune_model::VoiceActivity),
    // Voice protocol (voz-protocolo).
    /// Since when voice channels of `server` have had someone in them, as
    /// the service tells it; `None` for one nobody is in now. With `whole`
    /// these are all of the server's: any other has nobody.
    VoiceStartTimes {
        server: ServerId,
        times: Vec<(ChannelId, Option<Timestamp>)>,
        whole: bool,
    },
    // Calls and the voice panel (llamadas-y-panel-de-voz).
    /// The answer to [`Command::CheckCall`]: the call of the direct
    /// conversation `channel` may be made, and whether it may ring.
    CallChecked {
        channel: ChannelId,
        ringable: bool,
    },
    // Rules and onboarding (2026-10-18).
    /// `server`'s rules, asked for with `request`; `None` when it asks
    /// nothing before taking part.
    ServerRules {
        request: RequestId,
        server: ServerId,
        rules: Option<rune_model::ServerRules>,
    },
    /// What came of accepting `server`'s rules. Approved, the account takes
    /// part from now on, as its membership will also say.
    RulesAnswered {
        request: RequestId,
        server: ServerId,
        outcome: rune_model::JoinOutcome,
    },
    /// `server`'s onboarding, asked for or saved with `request`; `None`
    /// when it has none.
    OnboardingLoaded {
        request: RequestId,
        server: ServerId,
        onboarding: Option<rune_model::Onboarding>,
    },
    /// The account finished `server`'s onboarding choosing `chosen`.
    OnboardingAnswered {
        request: RequestId,
        server: ServerId,
        chosen: Vec<rune_model::ChoiceId>,
    },
    // Desktop session and games (escritorio-sesion-y-juegos).
    /// Whether the account shows its activity to others, as read or saved.
    ActivitySharing(bool),
    /// Whether others see the activity this device publishes, as the
    /// account's sessions say it now.
    ActivityVisibility(rune_model::ActivityVisibility),
    // Scheduled events (eventos-programados).
    /// Every event of `server`, asked for with `request`.
    ScheduledEvents {
        request: RequestId,
        server: ServerId,
        events: Vec<rune_model::ScheduledEvent>,
    },
    /// An event planned or changed: by the account with `request`, or as
    /// the service reported it (`None`).
    ScheduledEventChanged {
        request: Option<RequestId>,
        event: rune_model::ScheduledEvent,
    },
    /// An event deleted, as [`Event::ScheduledEventChanged`].
    ScheduledEventDeleted {
        request: Option<RequestId>,
        server: ServerId,
        event: rune_model::ScheduledEventId,
    },
    /// `event` interests `user` now, or no longer: the account's own answer
    /// to `request`, or as the service reported it (`None`).
    EventInterest {
        request: Option<RequestId>,
        server: ServerId,
        event: rune_model::ScheduledEventId,
        user: UserId,
        interested: bool,
    },
    /// Who said `event` interests them, asked for with `request`.
    EventInterested {
        request: RequestId,
        server: ServerId,
        event: rune_model::ScheduledEventId,
        users: Vec<User>,
    },
    // AutoMod (automod).
    /// Every AutoMod rule of `server`, asked for with `request`.
    AutoModRules {
        request: RequestId,
        server: ServerId,
        rules: Vec<rune_model::AutoModRule>,
    },
    /// An AutoMod rule is new or changed; `request` is the save that made
    /// it, when it was asked for here.
    AutoModRuleChanged {
        server: ServerId,
        rule: rune_model::AutoModRule,
        request: Option<RequestId>,
    },
    AutoModRuleRemoved {
        server: ServerId,
        rule: rune_model::AutoModRuleId,
    },
    /// AutoMod acted on someone in a server the account moderates.
    AutoModExecuted(rune_model::AutoModExecution),
    /// A server's AutoMod refused what `failure` names (a message, an edit,
    /// a title); `notice` is what the server tells whoever wrote it, when
    /// it says something. It ends that command as [`Event::Failed`] would:
    /// the core reports it as one, with the reason worded for the person.
    AutoModBlocked {
        failure: Failure,
        notice: Option<String>,
    },
}

/// How long an action waits for its verification to be solved, as the
/// official client's widget allows. A deadline kept for that action is
/// pushed back by this much when [`Event::ChallengeRequired`] names it.
pub const CHALLENGE_LIFETIME: std::time::Duration = std::time::Duration::from_secs(300);

/// What the provider can do. The interface hides or disables the rest and
/// says why.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Capabilities {
    /// Public invitation metadata can be inspected without accepting it.
    pub invitation_preview: bool,
    /// An explicitly requested invitation can be accepted by the current account.
    /// Independent of preview support and server administration.
    pub invitation_accept: bool,
    /// An account can be authorized interactively and signed out.
    pub authentication: bool,
    /// Several accounts can be remembered on this device and switched
    /// between with [`Command::UseAccount`].
    pub saved_accounts: bool,
    /// The account's basic server listing is available independently of chat access.
    pub account_servers: bool,
    pub send_messages: bool,
    pub edit_messages: bool,
    pub delete_messages: bool,
    pub reactions: bool,
    /// The account can vote on polls and take its vote back.
    pub poll_votes: bool,
    pub history: bool,
    /// A message can be opened by its exact ID with surrounding messages.
    pub message_context: bool,
    /// Inclusive read-through progress can receive account-bound, versioned
    /// confirmations. Manual unread reminders always remain local to RUNE.
    pub read_state: bool,
    pub typing: bool,
    pub direct_messages: bool,
    pub member_list: bool,
    pub presence: bool,
    pub voice: VoiceSupport,
    /// The account's camera reaches the others in its call
    /// ([`Command::SetCamera`]). Without it RUNE still previews it
    /// locally, and says it is not sent.
    pub video: bool,
    /// The account's screen reaches the others in its call
    /// ([`Command::StartStream`]); without it, it is only previewed.
    pub share_screen: bool,
    /// Someone's shared screen in the account's call can be watched
    /// ([`Command::WatchStream`]).
    pub watch_streams: bool,
    /// The cameras of the others in the account's call show on their tiles
    /// ([`video::VideoRequest::Wants`]).
    pub watch_cameras: bool,
    /// Voice channels hold a chat of their own, read and written like any
    /// conversation of the server.
    pub voice_text: bool,
    /// Files can be attached to messages.
    pub attachments: bool,
    /// Available server stickers can be sent as their own message.
    pub stickers: bool,
    /// Sending again with the same nonce cannot make a second message, so a
    /// send whose fate is unknown can be retried without repeating it.
    pub idempotent_send: bool,
    /// Servers and their roles can be created, changed and deleted.
    pub manage_servers: bool,
    /// Channels, categories and their overwrites can be managed.
    pub manage_channels: bool,
    /// Members can be given roles, renamed, kicked and banned.
    pub moderation: bool,
    /// The account's profile can be changed.
    pub edit_profile: bool,
    /// Someone's profile can be loaded on demand with `LoadProfile`.
    pub profiles: bool,
    /// The account can show a line of text beside its status.
    pub custom_status: bool,
    /// Servers keep a log of the changes made to them, read a page at a time.
    pub audit_log: bool,
    /// A server's invitations can be listed and revoked.
    pub manage_invites: bool,
    /// A server's custom emojis and stickers can be renamed and deleted.
    pub manage_expressions: bool,
    /// Messages can be pinned to their channel.
    pub pins: bool,
    /// Friends, requests and blocks can be read. Without it the interface
    /// says the list is not offered, never that it is empty.
    pub relationships: bool,
    /// Friend requests can be sent, accepted, ignored and cancelled, and
    /// friends removed.
    pub friend_requests: bool,
    /// People can be blocked and unblocked.
    pub block_users: bool,
    /// People can be ignored and unignored ([`Command::IgnoreUser`]).
    pub ignore_users: bool,
    /// A direct conversation can be started with someone who has none yet.
    pub open_direct: bool,
    /// Group conversations can be renamed, given a picture and left; adding
    /// and taking out people also needs `group_members`.
    pub manage_groups: bool,
    /// Messages can be sent inside threads and forum posts. Reading them
    /// needs no capability: they arrive as channels like any other.
    pub thread_replies: bool,
    // Video, phase 5.
    /// The picture of a stream shows under its owner in the column
    /// ([`Command::LoadStreamPreview`]).
    pub stream_previews: bool,
    /// Group conversations can be made with several friends at once.
    pub create_groups: bool,
    /// People can be added to and taken out of group conversations. Without
    /// it, `manage_groups` covers the name, the picture and leaving only.
    pub group_members: bool,
    /// A direct conversation can be taken off the list.
    pub close_direct: bool,
    /// Friends can be given a private name.
    pub friend_nicknames: bool,
    /// Friends' presence is reported, even without `presence`, which also
    /// covers the account's own status.
    pub friend_presence: bool,
    // Threads and forums.
    /// Threads can be started in channels, and posts in forums.
    pub create_threads: bool,
    /// A channel's threads, open and archived, can be read a page at a time.
    pub thread_pages: bool,
    /// Threads can be followed and unfollowed.
    pub follow_threads: bool,
    /// Threads can be renamed, tagged, closed, locked and deleted, as the
    /// account's permissions allow.
    pub manage_threads: bool,
    /// Where a link to a channel starts: the link is this, then
    /// `/{server}/{channel}`, with `@me` for the server outside servers,
    /// and `/{message}` after it for a message. `None` when the service
    /// has no such links.
    pub channel_links: Option<&'static str>,
    // Stages.
    /// Stages can be joined: listening in the audience, raising a hand,
    /// speaking when invited, and moderation inviting and stepping down.
    pub stages: bool,
    // Profiles.
    /// The account's picture and banner can be changed with
    /// [`Command::EditProfile`]; without it, only its text and color.
    pub profile_images: bool,
    /// The account's profile in each server (name, picture, banner, «Sobre
    /// mí») can be changed with [`Command::EditServerProfile`].
    pub server_profiles: bool,
    // Groups (11.3).
    /// The owner of a group can make someone else in it the owner.
    pub group_owner: bool,
    // Administration (19).
    /// Members can be timed out and the timeout lifted
    /// ([`Command::TimeoutMember`]).
    pub timeouts: bool,
    /// Invitations can be made ([`Command::CreateInvite`]).
    pub create_invites: bool,
    /// Where an invitation's link starts: the link is this, then its code.
    /// `None` when the service has no such links: only the code is shown.
    pub invite_links: Option<&'static str>,
    /// New emojis and stickers can be added ([`Command::CreateExpression`]).
    pub upload_expressions: bool,
    /// A server's members can be read a page at a time for its
    /// administration ([`Command::LoadMembers`]).
    pub member_pages: bool,
    /// Inactive members without a role can be counted and removed
    /// ([`Command::PruneMembers`]).
    pub prune: bool,
    /// A server's webhooks can be listed, made, renamed, moved and deleted.
    pub webhooks: bool,
    /// A server's integrations can be listed and removed.
    pub integrations: bool,
    /// The provider keeps the account's GIF favorites, across its devices
    /// ([`Command::LoadGifFavorites`]); without it RUNE keeps them on this
    /// device only, and says so.
    pub synced_gif_favorites: bool,
    // The account's synced settings (P1).
    /// How each server and channel notifies is kept with the account and
    /// read from it, instead of on this machine.
    pub synced_notifications: bool,
    /// The rail is laid out in the account's folders, which can be changed.
    pub server_folders: bool,
    /// Who may write to the account, and what is filtered, can be read and
    /// changed.
    pub messaging_privacy: bool,
    /// The account keeps a private note on each person.
    pub notes: bool,
    // Reads, search and remote members (P2).
    /// A whole server can be marked read at once ([`Command::MarkServerRead`]).
    pub server_read: bool,
    /// Holding a conversation unread reaches the provider and the account's
    /// other devices ([`Command::MarkUnread`]), and theirs arrive here.
    /// Without it a reminder stays in RUNE.
    pub remote_unread: bool,
    /// Messages can be searched at the provider, through all of a
    /// conversation's or a server's history ([`Command::SearchMessages`]).
    pub message_search: bool,
    /// A server's members can be looked up by name at the provider, beyond
    /// the ones already known ([`Command::SearchMembers`]).
    pub member_search: bool,
    // Commands and interactions (P3).
    /// Apps' buttons and menus tell their app what was done, and the
    /// modals apps open can be filled ([`Command::Interact`]).
    pub interactions: bool,
    /// Apps' commands can be listed, suggested and run
    /// ([`Command::LoadAppCommands`]).
    pub app_commands: bool,
    // Message actions and moderation (P4).
    /// Messages can be forwarded to other conversations
    /// ([`Command::ForwardMessage`]).
    pub forward_messages: bool,
    /// Polls can be started ([`Command::CreatePoll`]), and the account's
    /// own ended early ([`Command::EndPoll`]).
    pub create_polls: bool,
    /// Who reacted with each emoji can be read ([`Command::LoadReactions`]).
    pub reaction_lists: bool,
    /// Who voted for each answer can be read ([`Command::LoadPollVoters`]).
    pub poll_voters: bool,
    /// A conversation a link names can be read by its id
    /// ([`Command::LoadChannel`]).
    pub load_channels: bool,
    /// A message that starts with [`SILENT`] notifies nobody.
    pub silent_messages: bool,
    /// A reply can leave its author unnotified
    /// (`mention_reply` of [`Command::SendMessage`]).
    pub reply_mentions: bool,
    /// The moderation can mute, deafen, move and disconnect people in
    /// voice ([`Command::ModerateVoice`]).
    pub voice_moderation: bool,
    /// The account can leave servers ([`Command::LeaveServer`]), whatever
    /// else it may do with them.
    pub leave_servers: bool,
    /// Hosts whose links open without asking first, with their subdomains:
    /// the service's own. Other links ask, unless the person trusts them.
    pub trusted_links: &'static [&'static str],
    // Desktop, language and local voice (P6).
    /// What this device says the person is doing (another program's Rich
    /// Presence, a game) can be published as the account's activity
    /// ([`Command::SetLocalActivity`]).
    pub local_activity: bool,
    /// The provider lists the games it recognizes by their programs
    /// ([`Command::LoadDetectableGames`]).
    pub detectable_games: bool,
    /// Calls are end-to-end encrypted and say their privacy code
    /// ([`Event::CallPrivacyCode`]); each person's can be compared
    /// ([`Command::VoiceVerificationCode`]).
    pub voice_privacy_code: bool,
    // Presence, identity and expressions (P5).
    /// The service offers sticker packs to everyone
    /// ([`Command::LoadStickerPacks`]).
    pub sticker_packs: bool,
    /// The service searches GIFs itself ([`Command::SearchGifs`]), beside
    /// the optional GIPHY search of this device.
    pub gif_search: bool,
    // Server administration, rest (P7).
    /// A server's picture and the traits of its profile can be changed
    /// ([`ServerEdit::icon`], [`ServerEdit::traits`]) and the traits read
    /// ([`Command::LoadServerProfile`]).
    pub server_identity: bool,
    /// A server's system channel and messages, default notifications, idle
    /// channel, verification and media scan can be changed.
    pub server_safety: bool,
    /// New invitations to a server can be paused and resumed
    /// ([`ServerEdit::invites_paused`]).
    pub pause_invites: bool,
    /// A server's channels can be reordered and moved between categories
    /// at once ([`Command::ReorderChannels`]).
    pub reorder_channels: bool,
    /// A forum's tags, default reaction, layout and order can be changed
    /// ([`ChannelEdit::tags`], [`ChannelEdit::forum`]).
    pub forum_settings: bool,
    /// Roles can be given a picture or an emoji and a gradient, as each
    /// server allows ([`RoleEdit::icon`], [`RoleEdit::gradient`]).
    pub role_styles: bool,
    /// How many members have each role can be read ([`Command::LoadRoleCounts`]).
    pub role_counts: bool,
    /// The member pages can be sorted and kept to recent joins, and say
    /// how each one joined ([`Command::LoadMembers`]'s query).
    pub member_queries: bool,
    // Private calls.
    /// Direct and group conversations have calls: the account starts or
    /// answers one with [`Command::JoinVoice`], rings people with
    /// [`Command::RingCall`] and declines with [`Command::StopRinging`].
    /// Screens and cameras there follow `share_screen`, `watch_streams`,
    /// `video` and `watch_cameras`, as in voice channels.
    pub private_calls: bool,
    // Invitations of every kind.
    /// Besides `invitation_accept`: invitations to a group conversation
    /// can be accepted.
    pub group_invitations: bool,
    /// Besides `invitation_accept`: friend invitations can be accepted,
    /// which makes the account friends with whoever made them.
    pub friend_invitations: bool,
    /// Besides `invitation_accept`: guest invitations can be accepted, a
    /// way into one voice channel of a server without joining it.
    pub guest_invitations: bool,
    // The opening screen (2026-10-18).
    /// A connection waiting to try again after a loss can try at once
    /// ([`Command::ReconnectNow`]), without starting the provider again.
    pub reconnect_now: bool,
    // Calls and the voice panel (llamadas-y-panel-de-voz).
    /// Besides `private_calls`: a direct call is asked about before it
    /// starts ([`Command::CheckCall`]), and is not made when it may not be.
    pub call_check: bool,
    // Rules and onboarding (2026-10-18).
    /// A server's rules can be read and accepted
    /// ([`Command::LoadServerRules`], [`Command::AnswerServerRules`]).
    pub server_rules: bool,
    /// A server's onboarding can be read and answered
    /// ([`Command::LoadOnboarding`], [`Command::AnswerOnboarding`]).
    pub onboarding: bool,
    /// A server's onboarding can be changed ([`Command::SaveOnboarding`]).
    pub manage_onboarding: bool,
    // Threads, forums and stages (2026-10-18).
    /// A thread the account follows can be silenced for a while or for
    /// good ([`Command::MuteThread`]).
    pub mute_threads: bool,
    // Desktop session and games (escritorio-sesion-y-juegos).
    /// The account says whether it shows its activity, it can be changed
    /// ([`Command::SetActivitySharing`]), and its sessions say whether the
    /// activity published is seen ([`Event::ActivityVisibility`]).
    pub activity_sharing: bool,
    /// An activity's rich-presence buttons open their links, asked of the
    /// service ([`Command::OpenActivityButton`]).
    pub activity_buttons: bool,
    // Icon, banner and community (2026-10-18).
    /// A server's banner can be changed ([`ServerEdit::banner`]).
    pub server_banner: bool,
    /// A server can become a community and choose its rules and updates
    /// channels ([`ServerEdit::community`]).
    pub community: bool,
    // Scheduled events (eventos-programados).
    /// A server's events and who they interest can be read, and the
    /// account can say one interests it ([`Command::LoadScheduledEvents`],
    /// [`Command::LoadEventInterested`], [`Command::SetEventInterest`]).
    pub scheduled_events: bool,
    /// Events can be planned, changed, started, ended and deleted, as the
    /// account's permissions allow ([`Command::CreateScheduledEvent`] and
    /// the rest).
    pub manage_scheduled_events: bool,
    // AutoMod (automod).
    /// A server's AutoMod rules can be read, made, changed, turned on and
    /// off and deleted ([`Command::LoadAutoModRules`] and the rest).
    pub automod: bool,
    // Streams (transmisiones).
    /// The account's own stream gets a picture others see before watching
    /// it ([`Command::UploadStreamPreview`]).
    pub upload_stream_previews: bool,
}

impl Capabilities {
    /// Whether an invitation leading to `destination` can be accepted.
    pub fn accepts(&self, destination: InvitationDestination) -> bool {
        self.invitation_accept
            && match destination {
                InvitationDestination::Server(_) => true,
                InvitationDestination::Guest { .. } => self.guest_invitations,
                InvitationDestination::Group(_) => self.group_invitations,
                InvitationDestination::Friend(_) => self.friend_invitations,
            }
    }
}

/// How far voice goes with a provider.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VoiceSupport {
    #[default]
    None,
    /// Joining, muting and who speaks are simulated; no audio is carried.
    Simulated,
    /// Audio is carried.
    Real,
}

/// What kind of service a provider connects to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderKind {
    /// No service: data made up on this machine.
    Simulated,
    /// A service reached through an interface it publishes and authorizes.
    Official,
    /// An explicitly labeled integration outside the service's authorized application APIs.
    Unofficial,
}

/// An account this device remembers, as a list of them shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RememberedAccount {
    pub id: UserId,
    /// Its shown name when it was last used; may be empty.
    pub name: String,
    /// Its picture when it was last used, when it had one.
    pub avatar: Option<RemoteImage>,
}

/// What the interface says about the connection.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderInfo {
    pub name: String,
    pub kind: ProviderKind,
    pub capabilities: Capabilities,
    /// One sentence on what this provider is and is not, shown to the user.
    pub notice: String,
}

/// An already identified account that may use the local store. The provider
/// key is stable across display-name changes and distinct between adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageIdentity {
    pub provider: &'static str,
    pub account: UserId,
}

/// Why a session ended.
#[derive(Clone, Debug, PartialEq)]
pub enum ProviderError {
    /// Worth connecting again.
    Transient(String),
    /// Connecting again would fail the same way.
    Fatal(String),
}

/// An event and the session that produced it. Sessions are numbered as
/// they start; an event of an earlier one is stale and the core drops it.
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    pub generation: u64,
    pub event: Event,
}

/// What a latest value is about. Only its newest event matters.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Latest {
    Presence(UserId),
    Speaking(UserId),
}

/// The newest event of each [`Latest`] key not taken yet, shared between a
/// session's sink and the core.
pub type LatestValues = Arc<Mutex<HashMap<Latest, Envelope>>>;

/// Where a provider reports. Every event wakes the interface, which drains
/// it on its next frame. Three kinds of delivery:
///
/// - [`send`](Self::send): never lost. The queue is bounded, so the provider
///   waits while it is full instead of growing it without limit.
/// - [`send_latest`](Self::send_latest): state where only the newest value
///   matters, such as presence or who speaks. A newer value replaces an
///   older one not taken yet; the last one is never lost.
/// - [`send_lossy`](Self::send_lossy): signals that expire on their own,
///   such as someone typing. Dropped when the queue is full.
#[derive(Clone)]
pub struct EventSink {
    events: mpsc::Sender<Envelope>,
    latest: LatestValues,
    wake: Arc<dyn Fn() + Send + Sync>,
    generation: u64,
}

/// The interface is gone: the session should end.
#[derive(Debug)]
pub struct Closed;

impl EventSink {
    /// A sink into `events` and `latest` that calls `wake` after each event,
    /// for session 0.
    pub fn new(
        events: mpsc::Sender<Envelope>,
        latest: LatestValues,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            events,
            latest,
            wake,
            generation: 0,
        }
    }

    /// The same sink, stamping its events with session `generation`.
    pub fn for_session(&self, generation: u64) -> Self {
        Self {
            generation,
            ..self.clone()
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn envelope(&self, event: Event) -> Envelope {
        Envelope {
            generation: self.generation,
            event,
        }
    }

    /// Report `event`, waiting for room in the queue.
    pub async fn send(&self, event: Event) -> Result<(), Closed> {
        self.events
            .send(self.envelope(event))
            .await
            .map_err(|_| Closed)?;
        (self.wake)();
        Ok(())
    }

    /// Report the newest state `key` names. It never waits and is never lost:
    /// it replaces the previous value of `key` if that was not taken yet.
    pub fn send_latest(&self, key: Latest, event: Event) {
        let envelope = self.envelope(event);
        if let Ok(mut latest) = self.latest.lock() {
            latest.insert(key, envelope);
        }
        (self.wake)();
    }

    /// Report `event` if there is room now, else drop it: for what loses
    /// nothing when dropped, such as someone typing.
    pub fn send_lossy(&self, event: Event) {
        if self.events.try_send(self.envelope(event)).is_ok() {
            (self.wake)();
        }
    }
}

/// A boxed future, as a trait object can return one.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A service RUNE connects to.
pub trait Provider: Send + Sync + 'static {
    /// What it is and what it can do.
    fn info(&self) -> ProviderInfo;

    /// Verified outgoing attachment policy. Absence keeps upload support off.
    fn upload_limits(&self) -> Option<UploadLimits> {
        None
    }

    /// Opt in only when the account is already known and its persistence
    /// policy is implemented. Interactive adapters default to memory only,
    /// or name their verified account later with [`Event::StorageIdentity`].
    fn storage_identity(&self) -> Option<StorageIdentity> {
        None
    }

    /// A former single-account namespace, for an atomic one-time migration.
    /// The core checks the stored snapshot's account before moving any rows.
    fn legacy_storage_namespace(&self) -> Option<&'static str> {
        None
    }

    /// HTTPS origins for public images. A separate credential-free client
    /// validates references against this list and never follows redirects.
    fn image_origins(&self) -> &'static [&'static str] {
        &[]
    }

    /// The address of the picture at `url` for a screen of `density`
    /// pixels per logical one (1, 1.5 or 2), when the service serves it at
    /// a size asked for; `None` fetches `url` itself. On the same origin
    /// only; the core validates it as it validates `url`.
    fn image_at(&self, _url: &str, _density: f32) -> Option<String> {
        None
    }

    /// Recognize an invitation without I/O. Protocol-specific URL rules stay here.
    fn invitation_target(&self, _url: &str) -> Option<InvitationTarget> {
        None
    }

    /// The host's end of the call audio, for a provider whose voice is
    /// [`VoiceSupport::Real`]. Taken once; later calls return `None`.
    fn take_voice_link(&self) -> Option<voice::VoiceLink> {
        None
    }

    /// The host's end of watched streams, for a provider that carries them
    /// for real. Taken once; later calls return `None`.
    fn take_video_link(&self) -> Option<video::VideoLink> {
        None
    }

    /// Inspect public metadata without account credentials or membership changes.
    fn inspect_invitation(
        &self,
        _target: InvitationTarget,
    ) -> BoxFuture<'_, Result<InvitationPreview, InvitationError>> {
        Box::pin(async { Err(InvitationError::Unsupported) })
    }

    /// Run one session: report `Ready`, then carry out `commands` and report
    /// what happens until the commands end (`Ok`) or the session is lost
    /// (`Err`). The core connects again after a transient error.
    fn connect<'a>(
        &'a self,
        commands: &'a mut mpsc::Receiver<Command>,
        events: &'a EventSink,
    ) -> BoxFuture<'a, Result<(), ProviderError>>;

    // Profile editing (edicion-de-perfil).
    /// The file of a picture this provider keeps itself instead of on one
    /// of its [`image_origins`](Self::image_origins): the simulation's saved
    /// profile pictures. Only bytes made on this device, never fetched, and
    /// at most [`ProfileImage::MAX_ANIMATED_BYTES`]: the core drops larger
    /// ones. It answers at once, without waiting on anything (the core asks
    /// from the interface's thread), and is read as a download would be.
    fn hosted_image(&self, _url: &str) -> Option<bytes::Bytes> {
        None
    }
}
