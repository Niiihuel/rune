//! No command the session declares an ability for ends in the generic
//! «Esta acción todavía no está disponible»: one of each, sent through the
//! whole provider to a session with a server it owns, every request it
//! makes refused. Local fixtures only; nothing here reached Discord.

use super::*;
use bytes::Bytes;
use rune_model::{
    interactions::{CommandUse, Interaction},
    AuditQuery, ChannelKind, ChannelMove, EmojiId, ExpressionId, IntegrationId, LocalActivity,
    MemberQuery, NotificationChange, Overwrite, OverwriteTarget, Permissions, PrivacyChange,
    RemoteSearch, RoleId, ServerFolder, ServerId, Status, StickerId, UserId, WebhookId,
};
use rune_provider::{
    ChannelEdit, ExpressionImage, ExpressionKind, Failure, FavoriteGif, GifQuery, NewExpression,
    NewPoll, NewPollAnswer, ProfileEdit, RoleEdit, ServerEdit, ServerProfileEdit, ThreadEdit,
    ThreadScope, VoiceModeration,
};

/// What the generic rejection says, in both of its forms.
const GENERIC: &str = "Esta acción todavía no está disponible";
const ME: UserId = UserId(42);
const PEER: UserId = UserId(43);
const SERVER: ServerId = ServerId(10);
const DIRECT: ChannelId = ChannelId(20);
const GROUP: ChannelId = ChannelId(21);
const TEXT: ChannelId = ChannelId(30);
const VOICE: ChannelId = ChannelId(31);
const STAGE: ChannelId = ChannelId(32);
const FORUM: ChannelId = ChannelId(33);
const THREAD: ChannelId = ChannelId(35);
const MESSAGE: MessageId = MessageId(100);

/// A group and a server the account owns: text with a thread, voice, a
/// stage it sits in, a forum, a peer, an emoji and a sticker.
async fn owned(app: &mut Harness) {
    app.dispatch
        .send((
            "CHANNEL_CREATE",
            json!({"id":"21","type":3,"name":"grupo","owner_id":"42",
                "recipients":[{"id":"43","username":"Peer"}]}),
        ))
        .await
        .unwrap();
    app.dispatch
        .send((
            "GUILD_CREATE",
            json!({"id":"10","name":"Taller","owner_id":"42",
                "roles":[{"id":"10","name":"@everyone","permissions":"0","position":0},
                    {"id":"11","name":"Gente","permissions":"0","position":1}],
                "members":[{"user":{"id":"42","username":"Owner"},"roles":[]},
                    {"user":{"id":"43","username":"Peer"},"roles":["11"]}],
                "channels":[
                    {"id":"30","type":0,"name":"general","permission_overwrites":[]},
                    {"id":"31","type":2,"name":"sala","permission_overwrites":[]},
                    {"id":"32","type":13,"name":"escenario","permission_overwrites":[]},
                    {"id":"33","type":15,"name":"foro","permission_overwrites":[],
                        "available_tags":[{"id":"60","name":"Duda"}]},
                    {"id":"34","type":4,"name":"Texto","permission_overwrites":[]}],
                "threads":[{"id":"35","type":11,"name":"hilo","parent_id":"30",
                    "thread_metadata":{"archived":false,"locked":false},
                    "member":{"flags":0,"muted":false}}],
                "emojis":[{"id":"70","name":"ferris"}],
                "stickers":[{"id":"80","name":"Cangrejo","format_type":1}]}),
        ))
        .await
        .unwrap();
    app.dispatch
        .send((
            "VOICE_STATE_UPDATE",
            json!({"guild_id":"10","channel_id":"32","user_id":"42",
                "session_id":"elsewhere","suppress":true}),
        ))
        .await
        .unwrap();
    // The Gateway's dispatches go in order: once this message is in, so
    // is everything above, before any command can race it.
    let mut message = rest::message(MESSAGE.0, "hola");
    message["channel_id"] = json!("30");
    message["author"] = json!({"id":"43","username":"Peer"});
    app.dispatch
        .send(("MESSAGE_CREATE", message))
        .await
        .unwrap();
    loop {
        if matches!(app.running.next().await,
            Event::MessageCreated { message, .. } if message.id == MESSAGE)
        {
            break;
        }
    }
}

/// [`skipped`] and how many commands it keeps for [`samples`]: the
/// `None` arm is the list given, so a command put there without a
/// sample fails the test.
macro_rules! classified {
    ($($name:ident),+ $(,)?) => {
        /// How many commands [`samples`] has one of.
        const SAMPLED: usize = [$(stringify!($name)),+].len();

        /// Why a command is not among [`samples`]; `None` for the ones that
        /// are. No catch-all: a new command must say which it is.
        fn skipped(command: &Command) -> Option<&'static str> {
            match command {
                Command::BeginSignIn
                | Command::CancelSignIn
                | Command::SignOut
                | Command::ForgetSession
                | Command::UseAccount(_)
                | Command::ForgetAccount(_) => Some("the session's own lifetime: keyring tests"),
                Command::AcceptInvitation { .. } | Command::VerifyInvitation { .. } => {
                    Some("its own errors, never the generic one: invitation tests")
                }
                Command::SendFiles { .. } => Some("prepared files: upload tests"),
                Command::Cancel(_) | Command::SolveChallenge { .. } | Command::ReconnectNow => {
                    Some("about work in flight; nothing to refuse")
                }
                Command::JoinVoice { .. }
                | Command::LeaveVoice
                | Command::SetVoiceSelf { .. }
                | Command::WatchStream { .. }
                | Command::StopWatching { .. }
                | Command::StartStream { .. }
                | Command::StopStream { .. }
                | Command::PauseStream { .. }
                | Command::SetCamera { .. }
                | Command::VoiceVerificationCode { .. } => {
                    Some("the call's own connection, not a request: voice tests")
                }
                $(Command::$name { .. })|+ => None,
            }
        }
    };
}

classified! {
    LoadHistory, LoadContext, SendMessage, SendSticker, EditMessage, DeleteMessage,
    ToggleReaction, VotePoll, StartTyping, MarkRead, SetStatus, SetCustomStatus, CreateServer,
    EditServer, DeleteServer, LeaveServer, CreateChannel, EditChannel, DeleteChannel,
    SetOverwrite, RemoveOverwrite, CreateRole, EditRole, DeleteRole, SetMemberRoles, SetNick,
    Kick, Ban, Unban, LoadBans, LoadInvites, RevokeInvite, RenameExpression, DeleteExpression,
    EditProfile, LoadAudit, PinMessage, UnpinMessage, LoadProfile, WatchMemberList,
    UnwatchMemberList, LoadPins, SendFriendRequest, AcceptFriendRequest, RemoveRelationship,
    BlockUser, OpenDirect, RenameGroup, AddGroupMember, RemoveGroupMember, LeaveGroup,
    LoadStreamPreview, CreateGroup, SetGroupIcon, CloseDirect, SetFriendNickname, CreateThread,
    CreatePost, LoadThreads, FollowThread, EditThread, DeleteThread, RaiseHand, SetSpeaker,
    EditServerProfile, SetGroupOwner, TimeoutMember, CreateInvite, CreateExpression,
    LoadMembers, PruneMembers, LoadWebhooks, CreateWebhook, EditWebhook, DeleteWebhook,
    LoadIntegrations, DeleteIntegration, LoadGifFavorites, SetGifFavorite, LoadAccountSettings,
    SetNotifications, SetServerFolders, SetMessagingPrivacy, LoadNote, SetNote, MarkUnread,
    MarkServerRead, SearchMessages, SearchMembers, LoadAppCommands, Interact, SuggestChoices,
    ForwardMessage, CreatePoll, EndPoll, LoadReactions, LoadPollVoters, LoadChannel,
    ModerateVoice, SetLocalActivity, LoadDetectableGames, LoadStickerPacks, SearchGifs,
    SuggestGifs, LoadServerProfile, ReorderChannels, LoadRoleCounts, WatchPeople, RingCall,
    StopRinging, WatchServer, CheckCall, LoadServerRules, AnswerServerRules, LoadOnboarding,
    AnswerOnboarding, SaveOnboarding, MuteThread, SetActivitySharing,
    // Scheduled events (eventos-programados).
    LoadScheduledEvents, LoadEventInterested, SetEventInterest, CreateScheduledEvent,
    EditScheduledEvent, SetScheduledEventStatus, DeleteScheduledEvent,
    // AutoMod (automod).
    LoadAutoModRules, SaveAutoModRule, SetAutoModRuleEnabled, DeleteAutoModRule,
}

/// One of each command [`skipped`] does not leave out, aimed at what
/// [`owned`] has.
fn samples() -> Vec<Command> {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(64u32.to_be_bytes());
    png.extend(64u32.to_be_bytes());
    png.extend([8, 6, 0, 0, 0]);
    let answer = |text: &str| NewPollAnswer {
        text: text.into(),
        emoji: None,
    };
    let used = CommandUse {
        command: 1,
        version: 1,
        app: UserId(9),
        name: "dado".into(),
        server: Some(SERVER),
        path: Vec::new(),
        values: Vec::new(),
        focused: Some(("caras".into(), "6".into())),
    };
    vec![
        Command::LoadHistory {
            request: 1,
            channel: TEXT,
            before: None,
            limit: 50,
        },
        Command::LoadContext {
            request: 2,
            channel: TEXT,
            anchor: MESSAGE,
            limit: 50,
        },
        Command::SendMessage {
            channel: TEXT,
            content: "hola".into(),
            reply_to: None,
            nonce: 3,
            mention_reply: false,
        },
        Command::SendSticker {
            channel: TEXT,
            sticker: StickerId(80),
            nonce: 4,
        },
        Command::EditMessage {
            channel: TEXT,
            message: MESSAGE,
            content: "chau".into(),
        },
        Command::DeleteMessage {
            channel: TEXT,
            message: MESSAGE,
        },
        Command::ToggleReaction {
            channel: TEXT,
            message: MESSAGE,
            emoji: "👍".into(),
            add: true,
        },
        Command::VotePoll {
            channel: TEXT,
            message: MESSAGE,
            answers: vec![1],
        },
        Command::StartTyping { channel: TEXT },
        Command::MarkRead {
            request: 5,
            account: ME,
            channel: TEXT,
            message: MESSAGE,
        },
        Command::SetStatus(Status::Idle),
        Command::SetCustomStatus {
            text: Some("hola".into()),
            emoji: None,
            until: None,
        },
        Command::CreateServer {
            request: 6,
            name: "Nuevo".into(),
        },
        Command::EditServer {
            server: SERVER,
            edit: ServerEdit {
                name: Some("Otro".into()),
                ..Default::default()
            },
        },
        Command::DeleteServer { server: SERVER },
        Command::LeaveServer { server: SERVER },
        Command::CreateChannel {
            request: 7,
            server: SERVER,
            kind: ChannelKind::Text,
            name: "nuevo".into(),
            parent: None,
            private: false,
        },
        Command::EditChannel {
            channel: TEXT,
            edit: ChannelEdit {
                name: Some("otro".into()),
                ..Default::default()
            },
        },
        Command::DeleteChannel { channel: TEXT },
        Command::SetOverwrite {
            channel: TEXT,
            overwrite: Overwrite {
                target: OverwriteTarget::Member(PEER),
                allow: Permissions::SEND,
                deny: Permissions::NONE,
            },
        },
        Command::RemoveOverwrite {
            channel: TEXT,
            target: OverwriteTarget::Member(PEER),
        },
        Command::CreateRole {
            request: 8,
            server: SERVER,
            name: "Nuevo".into(),
        },
        Command::EditRole {
            server: SERVER,
            role: RoleId(11),
            edit: RoleEdit {
                name: Some("Otra".into()),
                ..Default::default()
            },
        },
        Command::DeleteRole {
            server: SERVER,
            role: RoleId(11),
        },
        Command::SetMemberRoles {
            server: SERVER,
            user: PEER,
            roles: Vec::new(),
        },
        Command::SetNick {
            server: SERVER,
            user: PEER,
            nick: Some("p".into()),
        },
        Command::Kick {
            server: SERVER,
            user: PEER,
            reason: None,
        },
        Command::Ban {
            server: SERVER,
            user: PEER,
            reason: None,
        },
        Command::Unban {
            server: SERVER,
            user: PEER,
        },
        Command::LoadBans {
            request: 9,
            server: SERVER,
            after: None,
            limit: 50,
        },
        Command::LoadInvites {
            request: 10,
            server: SERVER,
        },
        Command::RevokeInvite {
            server: SERVER,
            code: "abc".into(),
        },
        Command::RenameExpression {
            server: SERVER,
            expression: ExpressionId::Emoji(EmojiId(70)),
            name: "crab".into(),
        },
        Command::DeleteExpression {
            server: SERVER,
            expression: ExpressionId::Sticker(StickerId(80)),
        },
        Command::EditProfile(ProfileEdit {
            pronouns: Some(Some("elle".into())),
            ..Default::default()
        }),
        Command::LoadAudit {
            request: 11,
            server: SERVER,
            query: AuditQuery::default(),
            before: None,
            limit: 50,
        },
        Command::PinMessage {
            channel: TEXT,
            message: MESSAGE,
        },
        Command::UnpinMessage {
            channel: TEXT,
            message: MESSAGE,
        },
        Command::LoadProfile {
            request: 12,
            user: PEER,
            server: Some(SERVER),
            effect: true,
        },
        Command::WatchMemberList {
            server: SERVER,
            channel: TEXT,
            first: 0,
            last: 99,
        },
        Command::UnwatchMemberList,
        Command::LoadPins {
            request: 13,
            channel: TEXT,
        },
        Command::SendFriendRequest {
            username: "peer".into(),
        },
        Command::AcceptFriendRequest { user: PEER },
        Command::RemoveRelationship { user: PEER },
        Command::BlockUser { user: PEER },
        Command::OpenDirect {
            request: 14,
            user: PEER,
        },
        Command::RenameGroup {
            channel: GROUP,
            name: "otro".into(),
        },
        Command::AddGroupMember {
            channel: GROUP,
            user: UserId(44),
        },
        Command::RemoveGroupMember {
            channel: GROUP,
            user: PEER,
        },
        Command::LeaveGroup { channel: GROUP },
        Command::LoadStreamPreview {
            server: SERVER,
            channel: VOICE,
            owner: PEER,
        },
        Command::CreateGroup {
            request: 15,
            users: vec![PEER, UserId(44)],
        },
        Command::SetGroupIcon {
            channel: GROUP,
            icon: None,
        },
        Command::CloseDirect { channel: DIRECT },
        Command::SetFriendNickname {
            user: PEER,
            nickname: Some("p".into()),
        },
        Command::CreateThread {
            request: 16,
            channel: TEXT,
            message: None,
            name: "otro hilo".into(),
            private: false,
        },
        Command::CreatePost {
            request: 17,
            forum: FORUM,
            title: "Duda".into(),
            tags: Vec::new(),
            content: "¿Cómo?".into(),
            files: Vec::new(),
        },
        Command::LoadThreads {
            request: 18,
            parent: FORUM,
            scope: ThreadScope::default(),
            before: None,
        },
        Command::FollowThread {
            thread: THREAD,
            follow: true,
        },
        Command::EditThread {
            thread: THREAD,
            edit: ThreadEdit {
                name: Some("renombrado".into()),
                ..Default::default()
            },
        },
        Command::DeleteThread { thread: THREAD },
        Command::RaiseHand {
            channel: STAGE,
            raised: true,
        },
        Command::SetSpeaker {
            channel: STAGE,
            user: ME,
            speaker: false,
        },
        Command::EditServerProfile {
            server: SERVER,
            edit: ServerProfileEdit {
                nick: Some(Some("yo".into())),
                ..Default::default()
            },
        },
        Command::SetGroupOwner {
            channel: GROUP,
            user: PEER,
        },
        Command::TimeoutMember {
            server: SERVER,
            user: PEER,
            until: None,
            reason: None,
        },
        Command::CreateInvite {
            request: 19,
            server: SERVER,
            channel: TEXT,
            max_age_secs: 3600,
            max_uses: 0,
            temporary: false,
        },
        Command::CreateExpression {
            request: 20,
            server: SERVER,
            expression: NewExpression {
                kind: ExpressionKind::Emoji,
                name: "crab".into(),
                image: ExpressionImage::png(ExpressionKind::Emoji, Bytes::from(png)).unwrap(),
                tags: String::new(),
                description: String::new(),
            },
        },
        Command::LoadMembers {
            request: 21,
            server: SERVER,
            after: None,
            query: MemberQuery::default(),
        },
        Command::PruneMembers {
            request: 22,
            server: SERVER,
            days: 7,
            remove: false,
        },
        Command::LoadWebhooks {
            request: 23,
            server: SERVER,
        },
        Command::CreateWebhook {
            server: SERVER,
            channel: TEXT,
            name: "Gancho".into(),
        },
        Command::EditWebhook {
            server: SERVER,
            webhook: WebhookId(90),
            name: Some("Otro".into()),
            channel: None,
        },
        Command::DeleteWebhook {
            server: SERVER,
            webhook: WebhookId(90),
        },
        Command::LoadIntegrations {
            request: 24,
            server: SERVER,
        },
        Command::DeleteIntegration {
            server: SERVER,
            integration: IntegrationId(91),
        },
        Command::LoadGifFavorites,
        Command::SetGifFavorite {
            gif: FavoriteGif {
                url: "https://media.tenor.com/gato.gif".into(),
                preview: None,
                width: 0,
                height: 0,
            },
            favorite: true,
        },
        Command::LoadAccountSettings,
        Command::SetNotifications {
            server: Some(SERVER),
            change: NotificationChange::SuppressEveryone(true),
        },
        Command::SetServerFolders(vec![ServerFolder {
            servers: vec![SERVER],
            ..Default::default()
        }]),
        Command::SetMessagingPrivacy(PrivacyChange::ServerDms(false)),
        Command::LoadNote { user: PEER },
        Command::SetNote {
            user: PEER,
            note: "nota".into(),
        },
        Command::MarkUnread {
            account: ME,
            channel: TEXT,
            message: MESSAGE,
            mentions: 0,
        },
        Command::MarkServerRead {
            account: ME,
            server: SERVER,
        },
        Command::SearchMessages {
            request: 25,
            search: RemoteSearch {
                channel: TEXT,
                server: Some(SERVER),
                text: "hola".into(),
                ..Default::default()
            },
            offset: 0,
        },
        Command::SearchMembers {
            server: SERVER,
            query: "pe".into(),
        },
        Command::LoadAppCommands {
            request: 26,
            channel: TEXT,
        },
        Command::Interact {
            request: 27,
            channel: TEXT,
            interaction: Interaction::Part {
                message: MESSAGE,
                custom_id: "boton".into(),
                values: Vec::new(),
            },
            files: Vec::new(),
        },
        Command::SuggestChoices {
            request: 28,
            channel: TEXT,
            command: used,
        },
        Command::ForwardMessage {
            channel: TEXT,
            message: MESSAGE,
            to: DIRECT,
            nonce: 29,
        },
        Command::CreatePoll {
            channel: TEXT,
            poll: NewPoll {
                question: "¿Sí?".into(),
                answers: vec![answer("Sí"), answer("No")],
                hours: 24,
                multiselect: false,
            },
            nonce: 30,
        },
        Command::EndPoll {
            channel: TEXT,
            message: MESSAGE,
        },
        Command::LoadReactions {
            request: 31,
            channel: TEXT,
            message: MESSAGE,
            emoji: "👍".into(),
            after: None,
        },
        Command::LoadPollVoters {
            request: 32,
            channel: TEXT,
            message: MESSAGE,
            answer: 1,
            after: None,
        },
        Command::LoadChannel {
            request: 33,
            channel: TEXT,
        },
        Command::ModerateVoice {
            server: SERVER,
            user: PEER,
            change: VoiceModeration::Mute(true),
        },
        Command::SetLocalActivity(Some(LocalActivity {
            name: Some("Juego".into()),
            ..Default::default()
        })),
        Command::LoadDetectableGames,
        Command::LoadStickerPacks,
        Command::SearchGifs {
            request: 34,
            query: GifQuery::Search("gato".into()),
            offset: 0,
        },
        Command::SuggestGifs {
            request: 35,
            query: "ga".into(),
        },
        Command::LoadServerProfile {
            request: 36,
            server: SERVER,
        },
        Command::ReorderChannels {
            server: SERVER,
            moves: vec![ChannelMove {
                channel: TEXT,
                parent: None,
                position: 1,
                lock_permissions: false,
            }],
        },
        Command::LoadRoleCounts {
            request: 37,
            server: SERVER,
        },
        Command::WatchPeople {
            server: SERVER,
            people: vec![PEER],
        },
        Command::RingCall {
            channel: DIRECT,
            users: Vec::new(),
        },
        Command::StopRinging {
            channel: DIRECT,
            users: Vec::new(),
        },
        // Voice protocol (voz-protocolo).
        Command::WatchServer(Some(SERVER)),
        // Calls and the voice panel (llamadas-y-panel-de-voz).
        Command::CheckCall { channel: DIRECT },
        // Rules and onboarding (2026-10-18): the account owns the server,
        // so it answers nothing (refused by its own reason) and edits.
        Command::LoadServerRules {
            request: 38,
            server: SERVER,
        },
        Command::AnswerServerRules {
            request: 39,
            server: SERVER,
            rules: rune_model::ServerRules::default(),
            answers: Vec::new(),
        },
        Command::LoadOnboarding {
            request: 40,
            server: SERVER,
        },
        Command::AnswerOnboarding {
            request: 41,
            server: SERVER,
            onboarding: rune_model::Onboarding::default(),
            chosen: Vec::new(),
        },
        Command::SaveOnboarding {
            request: 42,
            server: SERVER,
            onboarding: rune_model::Onboarding::default(),
        },
        Command::MuteThread {
            thread: THREAD,
            until: Some(rune_model::MUTED_FOREVER),
        },
        // Desktop session and games (escritorio-sesion-y-juegos).
        Command::SetActivitySharing(true),
        // Scheduled events (eventos-programados).
        Command::LoadScheduledEvents {
            request: 43,
            server: SERVER,
        },
        Command::LoadEventInterested {
            request: 44,
            server: SERVER,
            event: rune_model::ScheduledEventId(90),
        },
        Command::SetEventInterest {
            request: 45,
            server: SERVER,
            event: rune_model::ScheduledEventId(90),
            interested: true,
        },
        Command::CreateScheduledEvent {
            request: 46,
            server: SERVER,
            draft: event_draft(),
        },
        Command::EditScheduledEvent {
            request: 47,
            server: SERVER,
            event: rune_model::ScheduledEventId(90),
            draft: event_draft(),
        },
        Command::SetScheduledEventStatus {
            request: 48,
            server: SERVER,
            event: rune_model::ScheduledEventId(90),
            status: rune_model::EventStatus::Active,
        },
        Command::DeleteScheduledEvent {
            request: 49,
            server: SERVER,
            event: rune_model::ScheduledEventId(90),
        },
        // AutoMod (automod).
        Command::LoadAutoModRules {
            request: 50,
            server: SERVER,
        },
        Command::SaveAutoModRule {
            request: 51,
            server: SERVER,
            rule: rune_model::AutoModRule {
                trigger: rune_model::AutoModTrigger::Keywords(rune_model::WordFilter {
                    keywords: vec!["hola".into()],
                    ..Default::default()
                }),
                ..rune_model::AutoModRule::fresh(rune_model::AutoModKind::Keywords)
            },
        },
        Command::SetAutoModRuleEnabled {
            server: SERVER,
            rule: rune_model::AutoModRuleId(90),
            enabled: false,
        },
        Command::DeleteAutoModRule {
            server: SERVER,
            rule: rune_model::AutoModRuleId(90),
        },
    ]
}

// Scheduled events (eventos-programados).
/// An event in the voice room, a day from now.
fn event_draft() -> rune_model::EventDraft {
    let start = rune_model::Timestamp(chrono::Utc::now().timestamp_millis() + 86_400_000);
    rune_model::EventDraft {
        name: "Repaso".into(),
        description: None,
        start,
        end: None,
        place: rune_model::EventPlace::Voice(VOICE),
    }
}

/// A command nobody waits on is refused without a word: it shows it got
/// past every check by its request, unless it makes none here.
fn shows_by_request(command: &Command) -> bool {
    Failure::of(command).is_none()
        && !matches!(
            command,
            // The Gateway's.
            Command::WatchMemberList { .. }
                | Command::UnwatchMemberList
                | Command::SearchMembers { .. }
                | Command::WatchPeople { .. }
                | Command::WatchServer(_)
                // Published with the presence; looked up only for an app.
                | Command::SetLocalActivity(_)
                // Needs the app's commands, which the fixture refuses.
                | Command::SuggestChoices { .. }
                // Asked only of someone streaming there.
                | Command::LoadStreamPreview { .. }
        )
}

/// Answer every request with a refusal and note each failure with the
/// generic reason, until nothing happens for `quiet` (and, for
/// `request`, one came or a while passed). How many requests came.
async fn settle(
    app: &mut Harness,
    sent: &Command,
    quiet: Duration,
    request: bool,
    generic: &mut Vec<String>,
) -> usize {
    let (mut answered, until) = (0, tokio::time::Instant::now() + Duration::from_secs(5));
    loop {
        tokio::select! {
            asked = app.requests.recv() => {
                let Some((_, reply)) = asked else { return answered };
                answered += 1;
                let _ = reply.send(response(400, r#"{"code":50035,"message":"SYNTHETIC"}"#));
            }
            event = app.running.events.recv() => {
                let Some(envelope) = event else { return answered };
                if let Event::Failed { failure, reason, .. } = envelope.event {
                    if reason.starts_with(GENERIC) {
                        generic.push(format!("{failure:?} (after {sent:?}): {reason}"));
                    }
                }
            }
            () = tokio::time::sleep(quiet) => {
                if !request || answered > 0 || tokio::time::Instant::now() >= until {
                    return answered;
                }
            }
        }
    }
}

#[tokio::test]
async fn no_command_the_session_declares_falls_through_to_not_available_yet() {
    let (login, _) = login();
    let capabilities = DiscordSessionProvider::new(login)
        .unwrap()
        .info()
        .capabilities;
    // Every ability but idempotent sends, how a send may be repeated and no
    // command of its own: each command below is one the session says it does.
    assert!(!capabilities.idempotent_send && capabilities.private_calls);
    let mut app = Harness::new().await;
    owned(&mut app).await;
    let mut generic = Vec::new();
    let (samples, mut requests) = (samples(), 0);
    let quiet = Duration::from_millis(60);
    // One of each command `skipped` keeps, none twice.
    let kinds: std::collections::HashSet<_> = samples.iter().map(std::mem::discriminant).collect();
    assert_eq!((kinds.len(), samples.len()), (SAMPLED, SAMPLED));
    for command in &samples {
        assert_eq!(skipped(command), None, "{command:?}");
        app.running.commands.send(command.clone()).await.unwrap();
        let silent = shows_by_request(command);
        let answered = settle(&mut app, command, quiet, silent, &mut generic).await;
        assert!(!silent || answered > 0, "{command:?} made no request");
        requests += answered;
    }
    let last = Command::LoadStickerPacks;
    requests += settle(&mut app, &last, quiet * 8, false, &mut generic).await;
    assert!(generic.is_empty(), "{generic:#?}");
    // Most got as far as their request: a fixture that turned them away
    // before it would prove nothing (88 requests for 110 commands when
    // written; the rest stop at a local check of their own).
    assert!(requests >= 80, "{requests} for {}", samples.len());
}
