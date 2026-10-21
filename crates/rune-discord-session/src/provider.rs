use rune_i18n::{es, t, tr};
use std::{
    collections::VecDeque,
    future::pending,
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

use rune_model::{ChannelKind, User, UserId};
use rune_platform::secrets::{Entry, Saved, SecretStore};
use rune_provider::{
    AuthenticationState, BoxFuture, Capabilities, Command, ConnectionState, Event, EventSink,
    Failure, InvitationDestination, InvitationJoinError, Provider, ProviderError, ProviderInfo,
    ProviderKind, RememberedAccount, Snapshot, VoiceSupport, CHALLENGE_LIFETIME,
};
use tokio::{
    sync::{mpsc, watch},
    time::Instant,
};

use crate::{
    bootstrap,
    challenges::{self, Challenges},
    gateway::{Gateway, GatewayEvent},
    http::{Answer, Http, Issued, RestError, Solved, Upload},
    members::{Lists, Watch},
    messages::{send_nonce, write_target, Messages},
    voice::Calls,
    wire, ChallengeAnswer, LoginError, SessionLogin, SessionSecret,
};
use zeroize::Zeroizing;

const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);
const COMMAND_QUEUE: usize = 16;
// Leave room for two 15-second HTTP phases before core's 60-second deadline.
const INVITATION_QUEUE_TTL: Duration = Duration::from_secs(20);
// Time left after solving for the one retry, within core's deadline.
const RETRY_RESERVE: Duration = Duration::from_secs(45);
// Discord shows an indicator for ten seconds; past half of that, skip it.
const TYPING_QUEUE_TTL: Duration = Duration::from_secs(5);
/// The keyring entries of this provider, one per account.
const SERVICE: &str = "discord-session";
/// This provider in the local store's names (encrypted cache, 2026-10-18).
pub(crate) const STORAGE: &str = "rune.discord-session.v1";

/// What the local store keeps of `account`: named once it is verified.
fn storage(account: UserId) -> rune_provider::StorageIdentity {
    rune_provider::StorageIdentity {
        provider: STORAGE,
        account,
    }
}
// Bounds waiting on the keyring where its answer must come before going on.
const VAULT_TIMEOUT: Duration = Duration::from_secs(15);

struct Queued {
    command: Command,
    at: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RestPhase {
    Standard,
    ContextRead {
        account: UserId,
    },
    MessagePreflight {
        account: UserId,
    },
    MessageWrite {
        account: UserId,
        author: Option<UserId>,
    },
    InvitationPreflight,
    InvitationPost,
}

impl RestPhase {
    fn ambiguous(self, command: &Command) -> bool {
        !matches!(
            self,
            Self::InvitationPreflight | Self::MessagePreflight { .. } | Self::ContextRead { .. }
        ) && !matches!(
            command,
            Command::LoadHistory { .. }
                | Command::LoadContext { .. }
                | Command::LoadPins { .. }
                | Command::LoadProfile { .. }
                | Command::LoadStreamPreview { .. }
                | Command::LoadThreads { .. }
                | Command::SearchMessages { .. }
                | Command::CheckCall { .. }
        ) && !crate::admin::is_read(command)
            && !crate::message_actions::is_read(command)
            && !crate::onboarding::is_read(command)
            // Scheduled events (eventos-programados).
            && !crate::events::is_read(command)
            // AutoMod (automod).
            && !crate::automod::is_read(command)
    }
}

enum Step {
    Acquired(SessionSecret),
    /// `restored`: it came from the keyring entry of this same account.
    Verified {
        secret: SessionSecret,
        user: Box<User>,
        restored: bool,
    },
    /// The saved session was rejected; its entry is already deleted. Carries
    /// the accounts still remembered, when the keyring could list them.
    Revoked(Option<Vec<Entry>>),
    /// `account`'s saved session could not be checked now; its entry stays.
    Unrestored {
        account: String,
        reason: LoginError,
    },
}

/// What a keyring operation left: whether the open session is remembered
/// (`None`: the operation was about another account), the accounts now
/// remembered, and a notice when it did not do what it should have.
struct VaultDone {
    remembered: Option<bool>,
    accounts: Option<Vec<Entry>>,
    notice: Option<String>,
}

type VaultOperation = BoxFuture<'static, VaultDone>;

type Operation = BoxFuture<'static, Result<Step, LoginError>>;
type GatewayOperation = BoxFuture<'static, Result<(), LoginError>>;

struct RestOperation {
    command: Command,
    phase: RestPhase,
    admitted: Instant,
    future: BoxFuture<'static, Result<Vec<Event>, RestError>>,
}

/// The hCaptcha that rejected one acceptance, while the person may solve it.
struct Held {
    command: Command,
    issued: Issued,
    until: Instant,
}

/// The host's window showing a held challenge.
struct Verifying {
    command: Command,
    rqtoken: Option<Zeroizing<String>>,
    session: Option<Zeroizing<String>>,
    future: BoxFuture<'static, Result<ChallengeAnswer, LoginError>>,
}

struct Session {
    secret: Arc<SessionSecret>,
    snapshot: Snapshot,
    messages: Messages,
    nonce_prefix: u32,
    online: bool,
    /// The member list watched, and the subscription the Gateway keeps.
    lists: Lists,
    members: watch::Sender<Option<Watch>>,
    /// Direct conversations closed while it runs, to list them again when
    /// someone writes in one.
    closed: Vec<rune_model::Channel>,
    /// What the account may attach, by plan and server boosts.
    limits: crate::uploads::Limits,
    // The account's synced settings (P1).
    account: crate::account::Account,
    /// The presence the Gateway tells the service.
    presence: watch::Sender<Option<crate::settings_proto::Own>>,
    // Reads, search and remote members (P2).
    reads: crate::reads::Reads,
    lookups: crate::member_search::Lookups,
    /// Apps' interactions on their way and the commands listed (P3).
    apps: crate::interactions::Apps,
    // Desktop, language and local voice (P6).
    /// The local activity the Gateway publishes with the presence.
    activity: watch::Sender<Option<serde_json::Value>>,
    /// What publishing a program's activity looked up, and its result.
    local: crate::desktop::LocalPresence,
    /// The icons of applications whose activities bring no picture.
    icons: crate::app_icons::AppIcons,
    // The opening screen (2026-10-18).
    /// Cuts the Gateway's wait between attempts short.
    retry: Arc<tokio::sync::Notify>,
    // Voice protocol (voz-protocolo).
    /// The server whose channels show, asked when its voice rooms went live.
    showing: watch::Sender<Option<u64>>,
}

/// Explicit normal-account access, outside Discord's authorized application
/// APIs. Commands and events contain domain data; credentials stay in RAM.
pub struct DiscordSessionProvider {
    pub(crate) login: Arc<dyn SessionLogin>,
    /// Owner-authorized (2026-10-16) storage of each remembered account's
    /// credential and shown name, to restore a session at the next start or
    /// switch to it. Never SQLite or files.
    pub(crate) secrets: Arc<dyn SecretStore>,
    pub(crate) http: Http,
    pub(crate) gateway: Gateway,
    /// The call audio: this end for the voice connection, the other for the host.
    pub(crate) voice: rune_provider::voice::VoicePort,
    pub(crate) voice_link: std::sync::Mutex<Option<rune_provider::voice::VoiceLink>>,
    /// Watched streams' frames, likewise.
    pub(crate) video: rune_provider::video::VideoPort,
    pub(crate) video_link: std::sync::Mutex<Option<rune_provider::video::VideoLink>>,
    /// Fixtures: the voice server address every call uses.
    #[cfg(test)]
    pub(crate) voice_local: Option<String>,
    // Reads, search and remote members (P2).
    /// The versions of confirmed read cursors, growing across sessions.
    pub(crate) read_versions: Arc<AtomicU64>,
}

/// Whether accounts are remembered in the system keyring. Blocking, for the
/// host's start before any window exists; an unusable keyring reads as none.
pub fn has_remembered_accounts() -> bool {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return false;
    };
    runtime.block_on(async {
        let secrets = rune_platform::secrets::system();
        matches!(
            tokio::time::timeout(VAULT_TIMEOUT, secrets.list(SERVICE)).await,
            Ok(Ok(entries)) if !entries.is_empty()
        )
    })
}

impl DiscordSessionProvider {
    /// Remembers the session in this system's keyring, if it has one. Its
    /// REST requests go as `proxy` says (escritorio-sesion-y-juegos). The
    /// Gateway and voice go straight.
    pub fn with_proxy(
        login: Arc<dyn SessionLogin>,
        proxy: &rune_provider::ApiProxy,
    ) -> Result<Self, LoginError> {
        let (voice, voice_link) = rune_provider::voice::voice_link();
        let (video, video_link) = rune_provider::video::video_link();
        Ok(Self {
            voice,
            voice_link: std::sync::Mutex::new(Some(voice_link)),
            video,
            video_link: std::sync::Mutex::new(Some(video_link)),
            #[cfg(test)]
            voice_local: None,
            login,
            // Tests never reach the person's keyring.
            #[cfg(not(test))]
            secrets: rune_platform::secrets::system(),
            #[cfg(test)]
            secrets: Arc::new(rune_platform::secrets::Unavailable),
            http: Http::with_proxy(proxy)?,
            gateway: Gateway::new(),
            read_versions: Arc::default(),
        })
    }

    async fn run(
        &self,
        commands: &mut mpsc::Receiver<Command>,
        events: &EventSink,
    ) -> Result<(), ProviderError> {
        let mut operation: Option<Operation> = None;
        // Browser identity (2026-10-19): the engine of the login windows
        // (Windows' WebView2, windows-login) and the web client's current
        // build, learnt before anything else goes out, so every request and
        // Identify name the same. Without the engine nothing goes out: the
        // saved session's check or the login asks again and says why.
        // Fixtures keep the fallback build: they never reach the page.
        if crate::browser::learn(&*self.login).await.is_ok() && !cfg!(test) {
            if let Some(build) = self.http.current_build().await {
                crate::browser::set_build(build);
            }
        }
        // Look for a saved session before announcing SignedOut, which opens
        // the login window. A failing keyring means memory only. Only the
        // last used account is verified; the others wait to be chosen.
        let listed = tokio::time::timeout(VAULT_TIMEOUT, self.secrets.list(SERVICE))
            .await
            .ok()
            .and_then(Result::ok);
        let saved = match listed.as_ref().and_then(|entries| entries.first()) {
            Some(last) => self.load(&last.account).await,
            None => None,
        };
        if let Some(entries) = &listed {
            emit(
                events,
                Event::RememberedAccounts(remembered_accounts(entries)),
            )
            .await?;
        }
        // The saved account being checked, or whose check could not finish:
        // signing out forgets it although no session holds it yet.
        let mut restoring: Option<String> = None;
        if let Some(saved) = saved {
            emit(
                events,
                Event::Authentication(AuthenticationState::Exchanging),
            )
            .await?;
            restoring = Some(saved.account.clone());
            operation = Some(self.restore(saved));
        } else {
            emit(
                events,
                Event::Authentication(AuthenticationState::SignedOut),
            )
            .await?;
        }
        let mut vault: Option<VaultOperation> = None;
        let mut remembered = false;
        // A saved session that could not be checked is retried once per click.
        let mut retry_restore: Option<String> = None;
        let mut session: Option<Session> = None;
        let mut gateway: Option<GatewayOperation> = None;
        let mut incoming: Option<mpsc::Receiver<GatewayEvent>> = None;
        let mut queue = VecDeque::<Queued>::new();
        let mut rest: Option<RestOperation> = None;
        // One file send at a time, beside the other requests: it can take
        // minutes, and reading or writing text must not wait for it.
        let mut upload: Option<Upload> = None;
        // The challenge that rejected an acceptance, the window showing it, and
        // the person's answer waiting for the one retry it allows.
        let mut held: Option<Held> = None;
        let mut verifying: Option<Verifying> = None;
        let mut solved: Option<(Command, Solved)> = None;
        // Any other write a challenge stopped, the answer for its one retry
        // until that goes out, and the write retried until it is answered.
        let mut challenges = Challenges::default();
        let mut retry: Option<(Command, Answer)> = None;
        let mut retrying: Option<Command> = None;
        // The icon of an application being read, beside the other requests.
        let mut icon: Option<BoxFuture<'static, (u64, Option<rune_model::RemoteImage>)>> = None;
        let mut calls = Calls::new(Some(self.voice.clone()));
        calls.video = Some(self.video.clone());
        #[cfg(test)]
        {
            calls.local = self.voice_local.clone();
        }
        loop {
            // A challenge belongs to one connected account and lifetime; its
            // window and answer never outlive that connection.
            if session.as_ref().is_none_or(|active| !active.online) {
                // An icon's read ends with its connection: its credential with
                // it. Back online, it is asked again.
                if icon.take().is_some() {
                    if let Some(active) = session.as_mut() {
                        active.icons.interrupted();
                    }
                }
                held = None;
                if let Some(work) = verifying.take() {
                    fail_invitation(events, &work.command, InvitationJoinError::Unavailable)
                        .await?;
                }
                if let Some((command, _)) = solved.take() {
                    fail_invitation(events, &command, InvitationJoinError::Unavailable).await?;
                }
                if let Some(waiting) = challenges.waiting.take() {
                    fail(events, &waiting.command, challenges::OFFLINE, false).await?;
                }
                retry = None;
                retrying = None;
            }
            if let Some((command, answer, left)) = challenges.take_solved() {
                // The one retry an answer allows, rechecked like the first
                // attempt; whatever its outcome, it is not repeated.
                if matches!(command, Command::SendFiles { .. }) {
                    let active = session.as_ref().expect("checked online above");
                    // Checked against the retry's own time, not the first attempt's.
                    let mut check = command.clone();
                    if let Command::SendFiles { scope, .. } = &mut check {
                        scope.expires_at = std::time::Instant::now() + left;
                    }
                    let admitted = crate::uploads::admit(
                        &active.snapshot,
                        &active.messages,
                        &active.limits,
                        events.generation(),
                        &check,
                    )
                    .and_then(|()| {
                        if upload.is_some() {
                            Err(rune_provider::UploadError::Busy)
                        } else {
                            Ok(())
                        }
                    });
                    match admitted {
                        Err(error) => {
                            fail(events, &command, LoginError::new(error.label()), false).await?
                        }
                        Ok(()) => {
                            let Command::SendFiles { scope, .. } = &command else {
                                unreachable!()
                            };
                            let nonce = send_nonce(active.nonce_prefix, scope.nonce);
                            retrying = Some(command.clone());
                            upload = Some(Upload::start(
                                self.http.answering(answer),
                                active.secret.clone(),
                                events.clone(),
                                command,
                                nonce,
                                Instant::now() + left,
                            ));
                        }
                    }
                } else {
                    retrying = Some(command.clone());
                    retry = Some((command.clone(), answer));
                    queue.push_front(Queued {
                        command,
                        at: Instant::now(),
                    });
                }
            }
            if held
                .as_ref()
                .is_some_and(|held| held.until <= Instant::now())
            {
                held = None;
            }
            if let Some((command, answer)) = solved.take().filter(|_| rest.is_none()) {
                // The one retry a solved challenge allows, rechecked like the
                // first attempt; it is never repeated, whatever the outcome.
                match invitation_allowed(session.as_ref(), &command) {
                    Err(error) => fail_invitation(events, &command, error).await?,
                    Ok(()) => {
                        let active = session.as_ref().expect("validated session");
                        let http = self.http.clone();
                        let secret = active.secret.clone();
                        let Command::AcceptInvitation {
                            request,
                            ref target,
                            destination,
                            expires_at,
                            ..
                        } = command
                        else {
                            unreachable!()
                        };
                        let target = target.clone();
                        let expires_at = Instant::from_std(expires_at);
                        rest = Some(RestOperation {
                            command,
                            phase: RestPhase::InvitationPost,
                            admitted: Instant::now(),
                            future: Box::pin(async move {
                                if expires_at.saturating_duration_since(Instant::now())
                                    < Duration::from_secs(15)
                                {
                                    return Err(RestError::invitation(
                                        InvitationJoinError::Unavailable,
                                    ));
                                }
                                tokio::time::timeout_at(
                                    expires_at,
                                    http.accept_invitation(
                                        &secret,
                                        &target,
                                        destination,
                                        request,
                                        Some(&answer),
                                    ),
                                )
                                .await
                                .unwrap_or_else(|_| {
                                    Err(RestError::invitation(InvitationJoinError::Ambiguous))
                                })
                            }),
                        });
                    }
                }
            }
            // The account's settings are read first: once per connection,
            // when another device changes them and after a refused write.
            if let Some(active) = session
                .as_mut()
                .filter(|active| active.online && active.account.stale)
            {
                active.account.stale = false;
                let reading = |command: &Command| matches!(command, Command::LoadAccountSettings);
                if !queue.iter().any(|queued| reading(&queued.command))
                    && !rest.as_ref().is_some_and(|work| reading(&work.command))
                {
                    queue.push_front(Queued {
                        command: Command::LoadAccountSettings,
                        at: Instant::now(),
                    });
                }
            }
            while rest.is_none() && !queue.is_empty() {
                if let Some(queued) = queue.pop_front() {
                    let command = queued.command;
                    // A late indicator would show typing after the message.
                    if matches!(command, Command::StartTyping { .. })
                        && queued.at.elapsed() >= TYPING_QUEUE_TTL
                    {
                        continue;
                    }
                    if matches!(command, Command::AcceptInvitation { .. }) {
                        let result = if queued.at.elapsed() >= INVITATION_QUEUE_TTL {
                            Err(InvitationJoinError::Unavailable)
                        } else {
                            invitation_allowed(session.as_ref(), &command)
                        };
                        if let Err(error) = result {
                            fail_invitation(events, &command, error).await?;
                            continue;
                        }
                        let http = self.http.clone();
                        let Command::AcceptInvitation {
                            ref target,
                            destination,
                            expires_at,
                            ..
                        } = command
                        else {
                            unreachable!()
                        };
                        let target = target.clone();
                        let expires_at = Instant::from_std(expires_at);
                        let admitted = queued.at;
                        rest = Some(RestOperation {
                            command,
                            phase: RestPhase::InvitationPreflight,
                            admitted,
                            future: Box::pin(async move {
                                if admitted.elapsed() >= INVITATION_QUEUE_TTL
                                    || expires_at.saturating_duration_since(Instant::now())
                                        < Duration::from_secs(30)
                                {
                                    return Err(RestError::invitation(
                                        InvitationJoinError::Unavailable,
                                    ));
                                }
                                http.inspect_acceptance(&target, destination).await
                            }),
                        });
                        continue;
                    }
                    match session.as_ref().filter(|session| session.online) {
                        Some(active) => match active
                            .messages
                            .needs_preflight(&active.snapshot, &command)
                        {
                            Ok(preflight) => {
                                let phase = if write_target(&command).is_some()
                                    || matches!(command, Command::LoadContext { .. })
                                {
                                    let Some(account) = active.snapshot.me.as_ref().map(|me| me.id)
                                    else {
                                        fail(events, &command, wire::invalid(), false).await?;
                                        continue;
                                    };
                                    if matches!(command, Command::LoadContext { .. }) {
                                        RestPhase::ContextRead { account }
                                    } else if preflight {
                                        RestPhase::MessagePreflight { account }
                                    } else {
                                        RestPhase::MessageWrite {
                                            account,
                                            author: active.messages.author(&command),
                                        }
                                    }
                                } else {
                                    RestPhase::Standard
                                };
                                let nonce = match &command {
                                    Command::SendMessage { nonce, .. }
                                    | Command::SendSticker { nonce, .. }
                                    | Command::ForwardMessage { nonce, .. }
                                    | Command::CreatePoll { nonce, .. } => {
                                        send_nonce(active.nonce_prefix, *nonce)
                                    }
                                    _ => String::new(),
                                };
                                // A preflight's write takes the answer later.
                                let http = match retry.take_if(|(retried, _)| {
                                    *retried == command
                                        && !matches!(phase, RestPhase::MessagePreflight { .. })
                                }) {
                                    Some((_, answer)) => self.http.answering(answer),
                                    None => self.http.clone(),
                                };
                                let secret = active.secret.clone();
                                let request = command.clone();
                                let me = active.snapshot.me.as_ref().map(|me| me.id);
                                let future: BoxFuture<'static, _> = if matches!(
                                    phase,
                                    RestPhase::MessagePreflight { .. }
                                ) {
                                    Box::pin(async move {
                                        let (channel, message) = write_target(&request)
                                            .expect("preflight targets a message");
                                        http.message_preflight(&secret, channel, message).await
                                    })
                                } else if let Some((server, me)) =
                                    crate::stage::target(&active.snapshot, &request)
                                {
                                    // A stage's requests go through its guild.
                                    Box::pin(async move {
                                        http.stage(&secret, server, me, &request).await
                                    })
                                } else if let Some((server, forum)) =
                                    open_threads(&active.snapshot, &request)
                                {
                                    // Open threads are read through the guild.
                                    let Command::LoadThreads {
                                        request,
                                        parent,
                                        before,
                                        ..
                                    } = request
                                    else {
                                        unreachable!("open_threads checked the command")
                                    };
                                    Box::pin(async move {
                                        http.active_threads(
                                            &secret,
                                            request,
                                            parent,
                                            server,
                                            forum,
                                            before.as_deref(),
                                        )
                                        .await
                                    })
                                } else if let (Command::EditProfile(edit), Some(account)) =
                                    (&request, active.snapshot.me.clone())
                                {
                                    let edit = edit.clone();
                                    Box::pin(async move {
                                        http.edit_profile(&secret, &account, &edit).await
                                    })
                                } else if let (
                                    Command::EditServerProfile { server, edit },
                                    Some(me),
                                ) = (&request, me)
                                {
                                    let (server, edit) = (*server, edit.clone());
                                    Box::pin(async move {
                                        http.edit_server_profile(&secret, me, server, &edit).await
                                    })
                                } else if let (true, Some(me)) =
                                    (crate::account::is_account_command(&request), me)
                                {
                                    let known = crate::http::Known {
                                        me,
                                        folders: active.account.folders.clone(),
                                        own: active.account.read.clone(),
                                    };
                                    Box::pin(async move {
                                        http.account(&secret, &request, &known).await
                                    })
                                } else if let Command::LoadProfile {
                                    request,
                                    user,
                                    server,
                                    effect,
                                } = request
                                {
                                    Box::pin(async move {
                                        http.profile(
                                            &secret,
                                            request,
                                            user,
                                            server,
                                            me != Some(user),
                                            effect,
                                        )
                                        .await
                                    })
                                } else if crate::identity::is_identity(&request) {
                                    // Packs and GIFs (P5): see `http::identity`.
                                    Box::pin(
                                        async move { http.expressions(&secret, &request).await },
                                    )
                                } else if crate::reads::is_read(&request) {
                                    // Reads and searches (P2): see `http::reads`.
                                    let token = active.reads.token.clone();
                                    Box::pin(
                                        async move { http.reads(&secret, &request, &token).await },
                                    )
                                } else if let Some(work) =
                                    crate::interactions::is_app_command(&request)
                                        .then(|| active.apps.work(&active.snapshot, &request))
                                        .flatten()
                                {
                                    // Apps (P3): the index, or the interaction admitted.
                                    Box::pin(async move { http.apps(&secret, work).await })
                                } else if crate::message_actions::is_action(&request) {
                                    // Message actions (P4): see `http::message_actions`.
                                    let source = match &request {
                                        Command::ForwardMessage { channel, .. } => active
                                            .snapshot
                                            .channels
                                            .iter()
                                            .find(|known| known.id == *channel)
                                            .and_then(|known| known.server),
                                        _ => None,
                                    };
                                    Box::pin(async move {
                                        http.message_action(&secret, &request, &nonce, source).await
                                    })
                                } else if let Command::SetLocalActivity(Some(activity)) = &request {
                                    // A program's activity, its name and pictures looked up (P6).
                                    let (activity, local) =
                                        (activity.clone(), active.local.clone());
                                    Box::pin(async move {
                                        http.local_activity(&secret, activity, local).await
                                    })
                                } else if matches!(request, Command::LoadDetectableGames) {
                                    Box::pin(async move { http.detectable_games().await })
                                } else if crate::admin::is_admin(&request) {
                                    // What the request needs of the session, read as it leaves.
                                    let context = crate::admin::context(&active.snapshot, &request);
                                    Box::pin(
                                        async move { http.admin(&secret, &request, context).await },
                                    )
                                } else if matches!(&request, Command::CreatePost { files, .. } if !files.is_empty())
                                {
                                    // A new post's files report their progress (envios-y-subidas).
                                    let events = events.clone();
                                    Box::pin(async move {
                                        http.create_post(&secret, &request, Some(&events)).await
                                    })
                                } else {
                                    write(http, secret, request, nonce)
                                };
                                rest = Some(RestOperation {
                                    command,
                                    phase,
                                    admitted: queued.at,
                                    future,
                                });
                            }
                            Err(reason) => fail(events, &command, reason, false).await?,
                        },
                        None => {
                            fail(
                                events,
                                &command,
                                LoginError::new(es("La conexión todavía no está disponible.")),
                                false,
                            )
                            .await?
                        }
                    }
                }
            }
            // An answer whose write was refused before it went out goes with
            // it: it never rides on another request.
            if retry.as_ref().is_some_and(|(retried, _)| {
                rest.as_ref().is_none_or(|work| work.command != *retried)
                    && queue
                        .front()
                        .is_none_or(|queued| queued.command != *retried)
            }) {
                retry = None;
                retrying = None;
            }
            if icon.is_none() {
                if let Some((active, application)) = session
                    .as_mut()
                    .filter(|active| active.online)
                    .and_then(|active| active.icons.next().map(|next| (active, next)))
                {
                    let (http, secret) = (self.http.clone(), active.secret.clone());
                    icon = Some(Box::pin(async move {
                        (
                            application,
                            http.application_icon(&secret, application).await,
                        )
                    }));
                }
            }
            let lapses = challenges.deadline();
            tokio::select! {
                // Cancellation wins over a concurrently completed login, REST
                // request or Gateway event; no detached task owns credentials.
                biased;
                command = commands.recv() => {
                    let Some(command) = command else {
                        // Let a pending deletion finish before the provider goes away.
                        settle(&mut vault, &mut remembered, events, false).await?;
                        return Ok(());
                    };
                    match command {
                        Command::BeginSignIn if session.is_none() => {
                            drop(operation.take());
                            let saved = match retry_restore.take() {
                                Some(account) => self.load(&account).await,
                                None => None,
                            };
                            restoring = saved.as_ref().map(|saved| saved.account.clone());
                            operation = Some(self.enter(saved, events).await?);
                        }
                        Command::BeginSignIn => {}
                        Command::CancelSignIn => {
                            operation = None;
                            retry_restore = None;
                            restoring = None;
                            if session.is_none() {
                                emit(events, Event::Authentication(AuthenticationState::SignedOut)).await?;
                            }
                        }
                        // Switching ends the session as signing out does, but
                        // keeps its entry for the next switch or start.
                        command @ (Command::SignOut | Command::UseAccount(_)) => {
                            let switching = matches!(command, Command::UseAccount(_));
                            let account = session.as_ref().and_then(account);
                            // A saved account still being checked has no session yet.
                            let checking = restoring.take();
                            let open = session.is_some();
                            // Signing out forgets the account here, also one still
                            // being checked: what the local store keeps of it goes
                            // too; switching keeps it.
                            let leaving = session.as_ref().and_then(|active| active.snapshot.me.as_ref()).map(|me| me.id)
                                .or_else(|| checking.as_deref().and_then(|id| id.parse().ok()).map(UserId));
                            if let Some(id) = leaving.filter(|_| !switching) {
                                emit(events, Event::ForgetStorage(storage(id))).await?;
                            }
                            operation = None;
                            retry_restore = None;
                            for event in calls.leave() { emit(events, event).await?; }
                            calls.end();
                            gateway = None;
                            incoming = None;
                            rest = None;
                            upload = None;
                            queue.clear();
                            session = None;
                            // The entry is gone before SignedOut: quitting right
                            // after signing out cannot restore this session.
                            settle(&mut vault, &mut remembered, events, false).await?;
                            let mut notice = None;
                            if let Some(account) = account.filter(|_| remembered).or(checking).filter(|_| !switching) {
                                let done = tokio::time::timeout(VAULT_TIMEOUT, self.forget(account, true)).await
                                    .unwrap_or_else(|_| VaultDone { remembered: Some(true), accounts: None, notice: Some(t(SLOW_KEYRING).into()) });
                                if let Some(entries) = done.accounts {
                                    emit(events, Event::RememberedAccounts(remembered_accounts(&entries))).await?;
                                }
                                notice = done.notice;
                            }
                            remembered = false;
                            if open || !switching {
                                emit(events, Event::Authentication(AuthenticationState::SignedOut)).await?;
                                emit(events, Event::Connection(ConnectionState::Disconnected {
                                    reason: if switching { t("Cambiando de cuenta.") } else { t("Sesión cerrada en este equipo.") }.into(),
                                })).await?;
                            }
                            if let Some(reason) = notice {
                                notify(events, reason).await?;
                            }
                            if let Command::UseAccount(target) = command {
                                let saved = match target {
                                    Some(id) => match self.load(&id.0.to_string()).await {
                                        Some(saved) => Some(saved),
                                        None => {
                                            emit(events, Event::Authentication(AuthenticationState::SignedOut)).await?;
                                            emit(events, Event::Connection(ConnectionState::Disconnected {
                                                reason: t("No se encontró la sesión guardada de esa cuenta. Entrá con «Usar otra cuenta» u olvidala.").into(),
                                            })).await?;
                                            continue;
                                        }
                                    },
                                    None => None,
                                };
                                restoring = saved.as_ref().map(|saved| saved.account.clone());
                                operation = Some(self.enter(saved, events).await?);
                            }
                        }
                        Command::ForgetSession => {
                            let account = session.as_ref().and_then(account);
                            self.drop_saved(&mut vault, &mut remembered, account, events, session.is_some()).await?;
                        }
                        Command::ForgetAccount(id) => {
                            // Forgotten here: its local data goes with its entry.
                            emit(events, Event::ForgetStorage(storage(id))).await?;
                            let target = id.0.to_string();
                            let current = session.as_ref().and_then(account);
                            if current.as_deref() == Some(target.as_str()) {
                                self.drop_saved(&mut vault, &mut remembered, current, events, true).await?;
                            } else {
                                settle(&mut vault, &mut remembered, events, session.is_some()).await?;
                                vault = Some(self.forget(target, false));
                            }
                        }
                        Command::Cancel(request) => {
                            queue.retain(|queued| !history_request(&queued.command, request));
                            if rest.as_ref().is_some_and(|work| history_request(&work.command, request)) {
                                rest = None;
                            }
                        }
                        command @ Command::AcceptInvitation { .. } => {
                            let allowed = invitation_allowed(session.as_ref(), &command);
                            match allowed {
                                Err(error) => fail_invitation(events, &command, error).await?,
                                Ok(()) if queue.len() >= COMMAND_QUEUE || invitation_pending(&queue, &rest)
                                    || verifying.is_some() || solved.is_some() =>
                                    fail_invitation(events, &command, InvitationJoinError::Unavailable).await?,
                                Ok(()) => {
                                    held = None;
                                    queue.push_back(Queued { command, at: Instant::now() });
                                }
                            }
                        }
                        Command::SolveChallenge { challenge, solve } => {
                            // One window at a time, the invitation's included.
                            let busy = solve && verifying.is_some();
                            if let Some((command, reason)) = challenges.answer(challenge, solve && !busy, &self.login) {
                                fail(events, &command, if busy { challenges::BUSY } else { reason }, false).await?;
                            }
                        }
                        command @ Command::VerifyInvitation { .. } => {
                            let Command::VerifyInvitation { request, destination, account, expires_at } = command else { unreachable!() };
                            // Only the challenge of this same request, account and
                            // invitation; any other one is discarded.
                            let found = held.take().filter(|held| matches!(held.command,
                                Command::AcceptInvitation { request: r, destination: s, account: a, .. } if r == request && s == destination && a == account));
                            let deadline = Instant::from_std(expires_at).checked_sub(RETRY_RESERVE);
                            let retry = found.map(|found| {
                                let mut retry = found.command;
                                if let Command::AcceptInvitation { expires_at: until, .. } = &mut retry {
                                    *until = expires_at;
                                }
                                (retry, found.issued, deadline.map(|at| at.min(found.until)))
                            });
                            let allowed = match &retry {
                                None => Err(InvitationJoinError::Unavailable),
                                Some((retry, ..)) => invitation_allowed(session.as_ref(), retry),
                            };
                            match (allowed, retry) {
                                (Ok(()), Some((retry, issued, Some(deadline))))
                                    if deadline > Instant::now() && !invitation_pending(&queue, &rest)
                                        && verifying.is_none() && solved.is_none() && !challenges.showing() =>
                                {
                                    let Issued { challenge, rqtoken, session: id, .. } = issued;
                                    let login = self.login.clone();
                                    verifying = Some(Verifying { command: retry, rqtoken, session: id,
                                        future: Box::pin(async move {
                                            tokio::time::timeout_at(deadline, login.verify(challenge)).await
                                                .unwrap_or_else(|_| Err(LoginError::new(es("Se agotó el tiempo de la verificación."))))
                                        }),
                                    });
                                }
                                (allowed, _) => fail_invitation(events, &command,
                                    allowed.err().unwrap_or(InvitationJoinError::Unavailable)).await?,
                            }
                        }
                        command @ (Command::LoadHistory { .. } | Command::LoadContext { .. } | Command::SendMessage { .. } | Command::SendSticker { .. }
                            | Command::EditMessage { .. } | Command::DeleteMessage { .. } | Command::VotePoll { .. }
                            | Command::ToggleReaction { .. } | Command::StartTyping { .. } | Command::PinMessage { .. }
                            | Command::UnpinMessage { .. } | Command::LoadPins { .. }
                            // People and conversations outside servers: `social::allowed`.
                            | Command::SendFriendRequest { .. } | Command::AcceptFriendRequest { .. }
                            | Command::RemoveRelationship { .. } | Command::BlockUser { .. }
                            | Command::SetFriendNickname { .. } | Command::OpenDirect { .. }
                            | Command::CreateGroup { .. } | Command::RenameGroup { .. }
                            | Command::SetGroupIcon { .. } | Command::AddGroupMember { .. }
                            | Command::RemoveGroupMember { .. } | Command::LeaveGroup { .. }
                            | Command::CloseDirect { .. } | Command::SetGroupOwner { .. }
                            // Threads and posts: `bootstrap::thread_allowed`.
                            | Command::CreateThread { .. } | Command::CreatePost { .. }
                            | Command::LoadThreads { .. } | Command::FollowThread { .. }
                            | Command::EditThread { .. } | Command::DeleteThread { .. }
                            | Command::MuteThread { .. }
                            // Stages: `stage::allowed`.
                            | Command::RaiseHand { .. } | Command::SetSpeaker { .. }
                            // Server administration and GIF favorites: `admin::allowed`.
                            | Command::CreateServer { .. } | Command::EditServer { .. } | Command::DeleteServer { .. }
                            | Command::LeaveServer { .. } | Command::CreateChannel { .. } | Command::EditChannel { .. }
                            | Command::DeleteChannel { .. } | Command::SetOverwrite { .. } | Command::RemoveOverwrite { .. }
                            | Command::CreateRole { .. } | Command::EditRole { .. } | Command::DeleteRole { .. }
                            | Command::SetMemberRoles { .. } | Command::SetNick { .. } | Command::Kick { .. }
                            | Command::Ban { .. } | Command::Unban { .. } | Command::LoadBans { .. }
                            | Command::LoadInvites { .. } | Command::RevokeInvite { .. } | Command::RenameExpression { .. }
                            | Command::DeleteExpression { .. } | Command::LoadAudit { .. } | Command::TimeoutMember { .. }
                            | Command::CreateInvite { .. } | Command::CreateExpression { .. } | Command::LoadMembers { .. }
                            | Command::PruneMembers { .. } | Command::LoadWebhooks { .. } | Command::CreateWebhook { .. }
                            | Command::EditWebhook { .. } | Command::DeleteWebhook { .. } | Command::LoadIntegrations { .. }
                            | Command::DeleteIntegration { .. } | Command::LoadGifFavorites | Command::SetGifFavorite { .. }
                            // The account's profile: `profiles::edit_allowed`.
                            | Command::EditProfile(_) | Command::EditServerProfile { .. }
                            // What the account keeps with Discord: `account::allowed`.
                            | Command::LoadAccountSettings | Command::SetStatus(_)
                            | Command::SetCustomStatus { .. } | Command::SetNotifications { .. }
                            | Command::SetServerFolders(_) | Command::SetMessagingPrivacy(_)
                            | Command::LoadNote { .. } | Command::SetNote { .. }
                            // Message actions and voice moderation (P4):
                            // `message_actions::allowed` and `admin::allowed`.
                            | Command::ForwardMessage { .. } | Command::CreatePoll { .. }
                            | Command::EndPoll { .. } | Command::LoadReactions { .. }
                            | Command::LoadPollVoters { .. } | Command::LoadChannel { .. }
                            | Command::ModerateVoice { .. }
                            // Server administration, rest (P7): `admin::allowed`.
                            | Command::LoadServerProfile { .. } | Command::ReorderChannels { .. }
                            | Command::LoadRoleCounts { .. }
                            // Calls and the voice panel: `voice::private::allowed`.
                            | Command::CheckCall { .. }
                            // Rules and onboarding: `onboarding::allowed`.
                            | Command::LoadServerRules { .. } | Command::AnswerServerRules { .. }
                            | Command::LoadOnboarding { .. } | Command::AnswerOnboarding { .. }
                            | Command::SaveOnboarding { .. }
                            // Desktop session and games: `account::allowed`.
                            | Command::SetActivitySharing(_)
                            // Scheduled events: `events::allowed`.
                            | Command::LoadScheduledEvents { .. } | Command::LoadEventInterested { .. }
                            | Command::SetEventInterest { .. } | Command::CreateScheduledEvent { .. }
                            | Command::EditScheduledEvent { .. } | Command::SetScheduledEventStatus { .. }
                            | Command::DeleteScheduledEvent { .. }
                            // AutoMod: `automod::allowed`.
                            | Command::LoadAutoModRules { .. } | Command::SaveAutoModRule { .. }
                            | Command::SetAutoModRuleEnabled { .. } | Command::DeleteAutoModRule { .. }) => {
                            let allowed = session.as_ref().filter(|session| session.online)
                                .ok_or_else(|| LoginError::new(es("La conexión todavía no está disponible.")))
                                .and_then(|session| session.messages.needs_preflight(&session.snapshot, &command).map(|_| ()));
                            match allowed {
                                Err(reason) => fail(events, &command, reason, false).await?,
                                Ok(()) if queue.len() >= COMMAND_QUEUE => fail(events, &command,
                                    LoginError::new(es("Hay demasiadas acciones pendientes. Volvé a intentarlo.")), false).await?,
                                Ok(()) => {
                                    // The Gateway tells the new status at once;
                                    // the account keeps it once Discord answers.
                                    // Private calls: a conversation read is asked about its call (op 13).
                                    if let (Some(active), Command::LoadHistory { channel, .. }) = (session.as_ref(), &command) {
                                        calls.opened(&active.snapshot, *channel);
                                    }
                                    if let (Some(active), Command::SetStatus(_) | Command::SetCustomStatus { .. })
                                        = (session.as_ref(), &command) {
                                        let wanted = crate::account::wanted(&active.account.told(), &command);
                                        active.presence.send_replace(Some(wanted));
                                    }
                                    queue.push_back(Queued { command, at: Instant::now() });
                                }
                            }
                        }
                        // Apps (P3): prepared with the session's state, then queued.
                        command @ (Command::LoadAppCommands { .. } | Command::Interact { .. }
                            | Command::SuggestChoices { .. }) => {
                            let admitted = match session.as_mut().filter(|session| session.online) {
                                None => Err(LoginError::new(es("La conexión todavía no está disponible."))),
                                Some(active) => active.apps.admit(&active.snapshot, &active.messages, &command),
                            };
                            match admitted {
                                Err(reason) => fail(events, &command, reason, false).await?,
                                Ok(()) if queue.len() >= COMMAND_QUEUE => fail(events, &command,
                                    LoginError::new(es("Hay demasiadas acciones pendientes. Volvé a intentarlo.")), false).await?,
                                Ok(()) => queue.push_back(Queued { command, at: Instant::now() }),
                            }
                        }
                        command @ Command::SendFiles { .. } => {
                            let admitted = session.as_ref().filter(|session| session.online)
                                .ok_or(rune_provider::UploadError::Unavailable)
                                .and_then(|active| crate::uploads::admit(&active.snapshot, &active.messages,
                                    &active.limits, events.generation(), &command))
                                .and_then(|()| if upload.is_some() { Err(rune_provider::UploadError::Busy) } else { Ok(()) });
                            match admitted {
                                Err(error) => fail(events, &command, LoginError::new(error.label()), false).await?,
                                Ok(()) => {
                                    let active = session.as_ref().expect("admitted in a session");
                                    let Command::SendFiles { scope, .. } = &command else { unreachable!() };
                                    let nonce = send_nonce(active.nonce_prefix, scope.nonce);
                                    let deadline = Instant::from_std(scope.expires_at);
                                    upload = Some(Upload::start(self.http.clone(), active.secret.clone(),
                                        events.clone(), command, nonce, deadline));
                                }
                            }
                        }
                        // One card at a time: a newer read replaces the one queued or in flight.
                        command @ Command::LoadProfile { .. } => {
                            let profile = |command: &Command| matches!(command, Command::LoadProfile { .. });
                            queue.retain(|queued| !profile(&queued.command));
                            if rest.as_ref().is_some_and(|work| profile(&work.command)) {
                                rest = None;
                            }
                            let allowed = session.as_ref().filter(|session| session.online)
                                .ok_or_else(|| LoginError::new(es("La conexión todavía no está disponible.")))
                                .and_then(|session| session.messages.allowed(&session.snapshot, &command));
                            match allowed {
                                Err(reason) => fail(events, &command, reason, false).await?,
                                Ok(()) if queue.len() >= COMMAND_QUEUE => fail(events, &command,
                                    LoginError::new(es("Hay demasiadas acciones pendientes. Volvé a intentarlo.")), false).await?,
                                Ok(()) => queue.push_back(Queued { command, at: Instant::now() }),
                            }
                        }
                        // Presence, identity and expressions (P5): reads of their
                        // own; a newer GIF search or suggestion replaces the one
                        // queued or in flight.
                        command @ (Command::LoadStickerPacks | Command::SearchGifs { .. }
                            | Command::SuggestGifs { .. }) => {
                            let same = |queued: &Command| std::mem::discriminant(queued) == std::mem::discriminant(&command);
                            if !matches!(command, Command::LoadStickerPacks) {
                                queue.retain(|queued| !same(&queued.command));
                                if rest.as_ref().is_some_and(|work| same(&work.command)) {
                                    rest = None;
                                }
                            }
                            let allowed = session.as_ref().filter(|session| session.online)
                                .ok_or_else(|| LoginError::new(es("La conexión todavía no está disponible.")))
                                .and_then(|_| if crate::identity::valid_query(&command) { Ok(()) }
                                    else { Err(LoginError::new(es("Buscá con un texto de hasta 50 caracteres."))) });
                            match allowed {
                                // A suggestion nobody waits on fails quietly.
                                Err(_) if matches!(command, Command::SuggestGifs { .. }) => {}
                                Err(reason) => fail(events, &command, reason, false).await?,
                                Ok(()) if queue.len() >= COMMAND_QUEUE => fail(events, &command,
                                    LoginError::new(es("Hay demasiadas acciones pendientes. Volvé a intentarlo.")), false).await?,
                                Ok(()) => queue.push_back(Queued { command, at: Instant::now() }),
                            }
                        }
                        // A stream's picture: a GET of its own, one at a time,
                        // only of a voice channel of that server.
                        command @ Command::LoadStreamPreview { server, channel, owner } => {
                            let preview = |queued: &Command| matches!(queued, Command::LoadStreamPreview { .. });
                            let known = session.as_ref().filter(|session| session.online).is_some_and(|session| {
                                owner.0 != 0 && session.snapshot.channels.iter().any(|known| {
                                    known.id == channel && known.server == Some(server) && known.kind == ChannelKind::Voice
                                })
                            });
                            if known && !queue.iter().any(|queued| preview(&queued.command)) && queue.len() < COMMAND_QUEUE {
                                queue.push_back(Queued { command, at: Instant::now() });
                            }
                        }
                        // Streams (transmisiones): the picture of the account's own
                        // stream, only while it shares there; a newer one replaces the
                        // one queued.
                        command @ Command::UploadStreamPreview { server, channel, owner, .. } => {
                            let upload = |queued: &Command| matches!(queued, Command::UploadStreamPreview { .. });
                            queue.retain(|queued| !upload(&queued.command));
                            let own = session.as_ref().filter(|session| session.online)
                                .and_then(|session| session.snapshot.me.as_ref())
                                .is_some_and(|me| me.id == owner)
                                && calls.own_stream_key() == Some(crate::http::stream_key(server, channel, owner));
                            if own && crate::http::preview_fits(&command) && queue.len() < COMMAND_QUEUE {
                                queue.push_back(Queued { command, at: Instant::now() });
                            }
                        }
                        // Desktop (P6): the newest activity wins over one queued or
                        // looking up; one that needs no lookup goes to the Gateway at once.
                        Command::SetLocalActivity(wanted) => {
                            let local = |queued: &Command| matches!(queued, Command::SetLocalActivity(_));
                            queue.retain(|queued| !local(&queued.command));
                            if rest.as_ref().is_some_and(|work| local(&work.command)) {
                                rest = None;
                            }
                            if let Some(active) = session.as_mut().filter(|session| session.online) {
                                active.local.take();
                                match wanted.as_ref().map(|wanted| (crate::desktop::ready(wanted), wanted.name.is_none() && wanted.application.is_none())) {
                                    None => { active.activity.send_replace(None); }
                                    Some((Some(value), _)) => { active.activity.send_replace(Some(value)); }
                                    // Nothing names it: there is nothing to publish.
                                    Some((None, true)) => { active.activity.send_replace(None); }
                                    Some((None, false)) if queue.len() < COMMAND_QUEUE => {
                                        queue.push_back(Queued { command: Command::SetLocalActivity(wanted), at: Instant::now() });
                                    }
                                    Some((None, false)) => {}
                                }
                            }
                        }
                        // One read of the list at a time.
                        Command::LoadDetectableGames => {
                            let reading = |queued: &Command| matches!(queued, Command::LoadDetectableGames);
                            let online = session.as_ref().is_some_and(|session| session.online);
                            if online && queue.len() < COMMAND_QUEUE && !queue.iter().any(|queued| reading(&queued.command))
                                && !rest.as_ref().is_some_and(|work| reading(&work.command)) {
                                queue.push_back(Queued { command: Command::LoadDetectableGames, at: Instant::now() });
                            }
                        }
                        Command::VoiceVerificationCode { channel, user } => {
                            for event in calls.verify(channel, user) { emit(events, event).await?; }
                        }
                        // Paced and sent by the Gateway; the list answers as it changes.
                        // A real account's first cards (2026-10-17): with the list watched.
                        Command::WatchPeople { server, people } => {
                            if let Some(active) = session.as_mut() {
                                let want = active.lists.people(server, &people);
                                active.members.send_if_modified(|kept| {
                                    let changed = *kept != want;
                                    *kept = want;
                                    changed
                                });
                            }
                        }
                        // Voice protocol (voz-protocolo): asked by the Gateway once a connection.
                        Command::WatchServer(server) => {
                            if let Some(active) = session.as_mut() {
                                active.showing.send_replace(server.map(|server| server.0));
                            }
                        }
                        command @ (Command::WatchMemberList { .. } | Command::UnwatchMemberList) => {
                            if let Some(active) = session.as_mut() {
                                let want = active.lists.watch(&active.snapshot, &command);
                                active.members.send_if_modified(|kept| {
                                    let changed = *kept != want;
                                    *kept = want;
                                    changed
                                });
                            }
                        }
                        Command::JoinVoice { channel, muted, deafened } => {
                            let joined = session.as_ref().filter(|session| session.online)
                                .ok_or_else(|| LoginError::new(es("La conexión todavía no está disponible.")))
                                .and_then(|active| calls.join(&active.snapshot, channel, muted, deafened));
                            match joined {
                                Ok(updates) => for event in updates { emit(events, event).await?; },
                                Err(reason) => fail(events, &Command::JoinVoice { channel, muted, deafened }, reason, false).await?,
                            }
                        }
                        Command::LeaveVoice => for event in calls.leave() { emit(events, event).await?; },
                        // Private calls: a ring waits for its call at the service.
                        command @ (Command::RingCall { .. } | Command::StopRinging { .. }) => {
                            let admitted = session.as_ref().filter(|session| session.online)
                                .ok_or_else(|| LoginError::new(es("La conexión todavía no está disponible.")))
                                .and_then(|active| crate::voice::private::allowed(&active.snapshot, &command))
                                .and_then(|()| calls.ring(command.clone()));
                            match admitted {
                                Err(reason) => fail(events, &command, reason, false).await?,
                                Ok(None) => {}
                                Ok(Some(_)) if queue.len() >= COMMAND_QUEUE => fail(events, &command,
                                    LoginError::new(es("Hay demasiadas acciones pendientes. Volvé a intentarlo.")), false).await?,
                                Ok(Some(command)) => queue.push_back(Queued { command, at: Instant::now() }),
                            }
                        }
                        Command::SetVoiceSelf { muted, deafened } => calls.set_self(muted, deafened),
                        Command::WatchStream { channel, owner } => {
                            let watched = session.as_ref().filter(|session| session.online)
                                .ok_or_else(|| LoginError::new(es("La conexión todavía no está disponible.")))
                                .and_then(|active| calls.watch(&active.snapshot, channel, owner));
                            match watched {
                                Ok(updates) => for event in updates { emit(events, event).await?; },
                                Err(reason) => fail(events, &Command::WatchStream { channel, owner }, reason, false).await?,
                            }
                        }
                        Command::StopWatching { channel, owner } => calls.stop_watching(channel, owner),
                        Command::StartStream { channel, quality } => {
                            let shared = session.as_ref().filter(|session| session.online)
                                .ok_or_else(|| LoginError::new(es("La conexión todavía no está disponible.")))
                                .and_then(|_| calls.share(channel, quality));
                            match shared {
                                Ok(updates) => for event in updates { emit(events, event).await?; },
                                Err(reason) => fail(events, &Command::StartStream { channel, quality }, reason, false).await?,
                            }
                        }
                        Command::StopStream { channel } => calls.stop_share(channel),
                        Command::PauseStream { channel, paused } => {
                            for event in calls.pause(channel, paused) { emit(events, event).await?; }
                        }
                        Command::SetCamera { channel, on } => {
                            if let Err(reason) = calls.set_camera(channel, on) {
                                fail(events, &Command::SetCamera { channel, on }, reason, false).await?;
                            }
                        }
                        // Reads, search and remote members (P2): `reads::allowed`.
                        // An acknowledgement replaces the one of its kind queued
                        // for its channel; a search, the one queued or in flight.
                        command @ (Command::MarkRead { .. } | Command::MarkUnread { .. }
                            | Command::MarkServerRead { .. } | Command::SearchMessages { .. }) => {
                            let same = |queued: &Command| crate::reads::ack_channel(queued).is_some()
                                && crate::reads::ack_channel(queued) == crate::reads::ack_channel(&command)
                                && std::mem::discriminant(queued) == std::mem::discriminant(&command);
                            queue.retain(|queued| !same(&queued.command));
                            if matches!(command, Command::SearchMessages { .. }) {
                                let search = |queued: &Command| matches!(queued, Command::SearchMessages { .. });
                                queue.retain(|queued| !search(&queued.command));
                                if rest.as_ref().is_some_and(|work| search(&work.command)) {
                                    rest = None;
                                }
                            }
                            let ack = crate::reads::ack_channel(&command).is_some();
                            let full = if ack {
                                queue.iter().filter(|queued| crate::reads::ack_channel(&queued.command).is_some()).count() >= crate::reads::MAX_QUEUED
                            } else {
                                queue.len() >= COMMAND_QUEUE
                            };
                            let allowed = session.as_ref().filter(|session| session.online)
                                .ok_or_else(|| LoginError::new(es("La conexión todavía no está disponible.")))
                                .and_then(|session| crate::reads::allowed(&session.snapshot, &command));
                            match allowed {
                                Err(reason) => fail(events, &command, reason, false).await?,
                                Ok(()) if full => fail(events, &command,
                                    LoginError::new(es("Hay demasiadas acciones pendientes. Volvé a intentarlo.")), false).await?,
                                Ok(()) => queue.push_back(Queued { command, at: Instant::now() }),
                            }
                        }
                        // The opening screen: the Gateway tries again now, if it waits to.
                        Command::ReconnectNow => {
                            if let Some(active) = session.as_ref() {
                                active.retry.notify_waiters();
                            }
                        }
                        // Sent by the Gateway, the newest only; nobody waits on it.
                        Command::SearchMembers { server, query } => {
                            if let Some(active) = session.as_mut().filter(|session| session.online) {
                                for event in active.lookups.ask(&active.snapshot, server, &query) {
                                    emit(events, event).await?;
                                }
                            }
                        }
                    }
                }
                result = async {
                    match operation.as_mut() { Some(next) => next.await, None => pending().await }
                } => {
                    operation = None;
                    restoring = None;
                    match result {
                        Ok(Step::Acquired(secret)) => {
                            emit(events, Event::Authentication(AuthenticationState::Exchanging)).await?;
                            let http = self.http.clone();
                            operation = Some(Box::pin(async move {
                                let user = Box::new(http.verify(&secret).await?);
                                Ok(Step::Verified { secret, user, restored: false })
                            }));
                        }
                        Ok(Step::Revoked(entries)) => {
                            // Before SignedOut: the interface lists what is left
                            // instead of opening the login window.
                            if let Some(entries) = entries {
                                emit(events, Event::RememberedAccounts(remembered_accounts(&entries))).await?;
                            }
                            emit(events, Event::Authentication(AuthenticationState::SignedOut)).await?;
                            emit(events, Event::Connection(ConnectionState::Disconnected {
                                reason: t("La sesión guardada ya no es válida. Iniciá sesión otra vez.").into(),
                            })).await?;
                        }
                        Ok(Step::Unrestored { account, reason }) => {
                            restoring = Some(account.clone());
                            retry_restore = Some(account);
                            emit(events, Event::Authentication(AuthenticationState::Failed {
                                reason: reason.to_string(),
                            })).await?;
                        }
                        Ok(Step::Verified { secret, user, restored }) => {
                            let mut random = [0; 4];
                            if getrandom::fill(&mut random).is_err() {
                                emit(events, Event::Authentication(AuthenticationState::Failed {
                                    reason: t("No se pudo preparar una sesión segura.").into(),
                                })).await?;
                                continue;
                            }
                            let account = user.id.0.to_string();
                            let name = user.shown_name().to_owned();
                            // The picture's public key, kept beside the name.
                            let avatar = user.avatar.as_ref().and_then(wire::image_key).unwrap_or_default().to_owned();
                            let id = user.id;
                            let snapshot = Snapshot { me: Some((*user).clone()), users: vec![*user], ..Default::default() };
                            // A restored session may show what this device kept
                            // of it until READY: no bare snapshot replaces that.
                            if !restored {
                                emit(events, Event::Ready(Box::new(snapshot.clone()))).await?;
                            }
                            // Verified: the local store may keep this account's data.
                            emit(events, Event::StorageIdentity(storage(id))).await?;
                            emit(events, Event::Authentication(AuthenticationState::SignedIn { scopes: vec![] })).await?;
                            emit(events, Event::Connection(ConnectionState::Connecting)).await?;
                            let secret = Arc::new(secret);
                            let transport = self.gateway.clone();
                            let shared = secret.clone();
                            let (sent, received) = mpsc::channel(32);
                            let (members, watched) = watch::channel(None);
                            let voice = calls.gateway();
                            let asked = calls.status_watch();
                            // Voice protocol (voz-protocolo): the server shown (op 43).
                            let (showing, shown) = watch::channel(None);
                            // Invisible until the account's status is read after READY.
                            let synced = crate::account::Account::default();
                            let (presence, told) = watch::channel(Some(synced.told()));
                            let (lookups, looked) = crate::member_search::Lookups::new(u32::from_le_bytes(random));
                            // Nothing of this device is published until the host says it again (P6).
                            let (activity, published) = watch::channel(None);
                            let retry = Arc::new(tokio::sync::Notify::new());
                            let hurry = retry.clone();
                            incoming = Some(received);
                            gateway = Some(Box::pin(async move { transport.run_watching(shared, sent, watched, voice, told, looked, published, asked, shown, hurry).await }));
                            // Saved again when restored too: the entry keeps its
                            // name current and becomes the last used. Best
                            // effort: without a keyring the session stays in RAM.
                            settle(&mut vault, &mut remembered, events, false).await?;
                            remembered = restored;
                            vault = Some(self.save(account, name, avatar, secret.clone(), restored));
                            session = Some(Session { secret, snapshot, messages: Messages::default(),
                                nonce_prefix: u32::from_le_bytes(random), online: false,
                                lists: Lists::default(), members, closed: Vec::new(),
                                limits: Default::default(), account: synced, presence,
                                reads: crate::reads::Reads::new(self.read_versions.clone()), lookups,
                                apps: Default::default(), activity, local: Default::default(),
                                icons: Default::default(), retry, showing });
                        }
                        Err(error) => emit(events, Event::Authentication(AuthenticationState::Failed {
                            reason: error.to_string(),
                        })).await?,
                    }
                }
                event = async {
                    match incoming.as_mut() { Some(events) => events.recv().await, None => pending().await }
                } => {
                    let Some(event) = event else { incoming = None; continue; };
                    let Some(active) = session.as_mut() else { continue; };
                    // Passive updates (P2): what is new to read, and who is in
                    // voice there, through the calls as their own dispatches.
                    if let GatewayEvent::Dispatch { kind, data } = &event {
                        if matches!(kind.as_str(), "PASSIVE_UPDATE_V1" | "PASSIVE_UPDATE_V2") {
                            let (latest, voice) = crate::reads::passive(&active.snapshot, data);
                            let mut updates: Vec<Event> = voice.iter()
                                .filter_map(|row| calls.dispatch(&mut active.snapshot, "VOICE_STATE_UPDATE", row))
                                .flatten().collect();
                            if !latest.is_empty() {
                                updates.push(Event::LatestMessages(latest));
                            }
                            for event in updates {
                                active.reads.observe(&event);
                                emit(events, event).await?;
                            }
                            continue;
                        }
                    }
                    // Whoever a voice state names is known before it (2026-10-17).
                    if let GatewayEvent::Dispatch { kind, data } = &event {
                        if kind.as_str() == "VOICE_STATE_UPDATE" {
                            let me = active.snapshot.me.as_ref().map(|me| me.id);
                            for event in crate::members::in_voice(me, data) { emit(events, event).await?; }
                        }
                    }
                    if let GatewayEvent::Dispatch { kind, data } = &event {
                        if let Some(updates) = calls.dispatch(&mut active.snapshot, kind.as_str(), data) {
                            for event in updates { emit(events, event).await?; }
                            // Private calls: a ring held for its call goes now.
                            if let Some(command) = calls.take_ring() {
                                if queue.len() >= COMMAND_QUEUE {
                                    fail(events, &command, LoginError::new(es("Hay demasiadas acciones pendientes. Volvé a intentarlo.")), false).await?;
                                } else {
                                    queue.push_back(Queued { command, at: Instant::now() });
                                }
                            }
                            continue;
                        }
                    }
                    // The Gateway's session tells this device's voice states
                    // from another device's (as the official client does).
                    if let GatewayEvent::Session(id) = &event {
                        calls.gateway_session(id.clone());
                    }
                    // Told after the new snapshot: a call that went to another device ends.
                    let call = match &event {
                        GatewayEvent::Ready(data) => calls.ready(data),
                        // A server after READY: a call held for it goes on (voz-protocolo).
                        GatewayEvent::Dispatch { kind, data } if kind.as_str() == "GUILD_CREATE" => calls.guild_created(data),
                        _ => Vec::new(),
                    };
                    let limits = active.limits.observe(&event);
                    match handle_gateway(active, event) {
                        Ok(updates) => {
                            // A confirmed acceptance queued behind another
                            // request is complete too; it must not be sent again.
                            queue.retain(|queued| !friend_request_confirmed(&queued.command, &updates));
                            retry.take_if(|(command, _)| friend_request_confirmed(command, &updates));
                            // A read loses its scope at revocation, even while
                            // another channel's HTTP request holds the worker.
                            // Confirmed deletion also invalidates queued writes.
                            for _ in 0..queue.len() {
                                let queued = queue.pop_front().expect("bounded queue entry");
                                let context_invalid = matches!(queued.command, Command::LoadContext { .. })
                                    && (!active.online || active.messages.allowed(&active.snapshot, &queued.command).is_err());
                                if message_changed(&queued.command, &updates) || context_invalid {
                                    fail(events, &queued.command, LoginError::new(es("El mensaje o la sesión cambió antes de completar la acción.")), false).await?;
                                } else { queue.push_back(queued); }
                            }
                            if !active.online {
                                // A queued invite was approved for the current
                                // connection. It cannot silently run on resume.
                                let cancelled: Vec<_> = queue.iter()
                                    .filter(|queued| matches!(queued.command, Command::AcceptInvitation { .. }))
                                    .map(|queued| queued.command.clone()).collect();
                                queue.retain(|queued| !matches!(queued.command, Command::AcceptInvitation { .. }));
                                for command in cancelled {
                                    fail_invitation(events, &command, InvitationJoinError::Unavailable).await?;
                                }
                            }
                            // The Gateway can confirm acceptance before PUT's
                            // reply. Its new Friend state is the success, not
                            // a lost precondition for the in-flight request.
                            if let Some(work) = rest.take_if(|work| friend_request_confirmed(&work.command, &updates)) {
                                retrying.take_if(|retried| *retried == work.command);
                            }
                            // Permission/removal dispatches can invalidate a
                            // request while HTTP is in flight. Drop it before
                            // it can repopulate a channel the UI just purged.
                            if let Some(reason) = rest.as_ref().and_then(|work| {
                                if message_changed(&work.command, &updates) {
                                    Some(LoginError::new(es("El mensaje o la sesión cambió antes de completar la acción.")))
                                } else { invalidated(active, work) }
                            }) {
                                let work = rest.take().expect("invalidated request exists");
                                let ambiguous = work.phase.ambiguous(&work.command);
                                drop(work.future);
                                fail(events, &work.command, reason, ambiguous).await?;
                            }
                            for mut event in updates {
                                active.messages.observe(&event);
                                active.reads.observe(&event);
                                active.icons.fill(&mut event);
                                emit(events, event).await?;
                            }
                            for event in call { emit(events, event).await?; }
                            for event in limits { emit(events, event).await?; }
                        },
                        Err(error) => {
                            active.online = false;
                            gateway = None;
                            incoming = None;
                            for event in calls.leave() { emit(events, event).await?; }
                            calls.end();
                            cancel_work(events, &mut rest, &mut queue, error).await?;
                            emit(events, Event::Connection(ConnectionState::Disconnected { reason: error.to_string() })).await?;
                        }
                    }
                }
                result = async {
                    match gateway.as_mut() { Some(next) => next.await, None => pending().await }
                } => {
                    gateway = None;
                    incoming = None;
                    for event in calls.leave() { emit(events, event).await?; }
                    calls.end();
                    if let Some(active) = session.as_mut() { active.online = false; }
                    let reason = result.err().unwrap_or(LoginError::new(es("La conexión con Discord terminó.")));
                    let rejected_session = reason.gateway_close_code() == Some(4004);
                    if rejected_session {
                        // A definitive Gateway authentication rejection has the
                        // same session-lifetime consequence as authenticated HTTP401.
                        operation = None;
                        upload = None;
                        let account = session.take().as_ref().and_then(account);
                        self.drop_saved(&mut vault, &mut remembered, account, events, false).await?;
                    }
                    cancel_work(events, &mut rest, &mut queue, reason).await?;
                    if rejected_session {
                        emit(events, Event::Authentication(AuthenticationState::Failed {
                            reason: reason.to_string(),
                        })).await?;
                    }
                    emit(events, Event::Connection(ConnectionState::Disconnected { reason: reason.to_string() })).await?;
                }
                updates = calls.next() => {
                    for event in updates { emit(events, event).await?; }
                }
                (application, found) = async {
                    match icon.as_mut() { Some(work) => work.await, None => pending().await }
                } => {
                    icon = None;
                    if let Some(active) = session.as_mut() {
                        for event in active.icons.arrived(application, found) { emit(events, event).await?; }
                    }
                }
                answer = challenges.shown() => {
                    if let Some((command, reason)) = challenges.solved(answer) {
                        fail(events, &command, reason, false).await?;
                    }
                }
                () = async {
                    match lapses { Some(until) => tokio::time::sleep_until(until).await, None => pending().await }
                } => {
                    if let Some(waiting) = challenges.waiting.take() {
                        fail(events, &waiting.command, challenges::EXPIRED, false).await?;
                    }
                }
                answer = async {
                    match verifying.as_mut() { Some(work) => work.future.as_mut().await, None => pending().await }
                } => {
                    let work = verifying.take().expect("a verification exists");
                    match answer {
                        Ok(answer) => solved = Some((work.command, Solved { answer, rqtoken: work.rqtoken, session: work.session })),
                        // Closed, expired or unavailable: nothing was sent.
                        Err(_) => fail_invitation(events, &work.command, InvitationJoinError::Unavailable).await?,
                    }
                }
                result = async {
                    match upload.as_mut() { Some(work) => work.future.as_mut().await, None => pending().await }
                } => {
                    let work = upload.take().expect("a finished upload exists");
                    let retried = retrying.take_if(|retried| same_upload(retried, &work.command)).is_some();
                    match result {
                        Ok(updates) => if let Some(active) = session.as_mut() {
                            if updates.iter().any(|event| matches!(event, Event::MessageCreated { message, .. }
                                if active.snapshot.me.as_ref().is_none_or(|me| message.author != me.id))) {
                                fail(events, &work.command, wire::invalid(), true).await?;
                                continue;
                            }
                            for event in updates {
                                active.messages.observe(&event);
                                emit(events, event).await?;
                            }
                        },
                        Err(mut error) => {
                            if let Some(issued) = error.challenge.take() {
                                let Command::SendFiles { scope, .. } = &work.command else { unreachable!() };
                                // The retry stages every file again: it gets this attempt's whole time.
                                let left = Instant::from_std(scope.expires_at).saturating_duration_since(work.started);
                                match (!retried).then(|| challenges.offer(&work.command, issued, left)).flatten() {
                                    Some(offer) => { emit(events, offer).await?; continue; }
                                    None => error.reason = if retried { challenges::AGAIN } else { challenges::BUSY },
                                }
                            }
                            fail_or_blocked(events, &work.command, error.blocked.take().map(|blocked| blocked.notice), error.reason, error.ambiguous && work.ambiguous()).await?;
                            if error.ends_session {
                                gateway = None;
                                incoming = None;
                                cancel_work(events, &mut rest, &mut queue, error.reason).await?;
                                let account = session.take().as_ref().and_then(account);
                                self.drop_saved(&mut vault, &mut remembered, account, events, false).await?;
                                emit(events, Event::Authentication(AuthenticationState::Failed {
                                    reason: error.reason.to_string(),
                                })).await?;
                            }
                        }
                    }
                }
                done = async {
                    match vault.as_mut() { Some(next) => next.await, None => pending().await }
                } => {
                    vault = None;
                    finish(done, &mut remembered, events, session.is_some()).await?;
                }
                result = async {
                    match rest.as_mut() { Some(work) => work.future.as_mut().await, None => pending().await }
                } => {
                    let work = rest.take().expect("a completed request exists");
                    if result.is_ok() && matches!(work.phase, RestPhase::ContextRead { .. }) {
                        let allowed = session.as_ref().ok_or_else(|| LoginError::new(es("La sesión ya no está disponible.")))
                            .and_then(|active| invalidated(active, &work).map_or(Ok(()), Err));
                        if let Err(reason) = allowed { fail(events, &work.command, reason, false).await?; continue; }
                    }
                    if let (RestPhase::MessagePreflight { account }, Ok(updates)) = (work.phase, &result) {
                        let Some(active) = session.as_mut().filter(|active| active.online) else {
                            fail(events, &work.command, LoginError::new(es("La conexión todavía no está disponible.")), false).await?;
                            continue;
                        };
                        let message = updates.iter().find_map(|event| match event {
                            Event::MessageUpdated(message) if write_target(&work.command) == Some((message.channel, message.id)) => Some(message),
                            _ => None,
                        });
                        let allowed = Messages::readable(&active.snapshot, &work.command, account)
                            .and_then(|_| message.ok_or_else(wire::invalid))
                            .and_then(|message| Messages::allowed_with_author(&active.snapshot, &work.command, account, Some(message.author)));
                        if let Err(reason) = allowed { fail(events, &work.command, reason, false).await?; continue; }
                        let author = message.expect("validated baseline").author;
                        for event in updates { active.messages.observe(event); }
                        let http = match retry.take_if(|(retried, _)| *retried == work.command) {
                            Some((_, answer)) => self.http.answering(answer),
                            None => self.http.clone(),
                        };
                        let secret = active.secret.clone();
                        let request = work.command.clone();
                        rest = Some(RestOperation {
                            command: work.command, phase: RestPhase::MessageWrite { account, author: Some(author) }, admitted: work.admitted,
                            future: write(http, secret, request, String::new()),
                        });
                        // Re-enter select before polling the write, so queued
                        // logout, Gateway revocation and deletion win over HTTP.
                        continue;
                    }
                    if work.phase == RestPhase::InvitationPreflight && result.is_ok() {
                        // Re-enter the provider loop before the write: account,
                        // membership and connectivity may have changed during GET.
                        if let Err(error) = invitation_allowed(session.as_ref(), &work.command) {
                            fail_invitation(events, &work.command, error).await?;
                            continue;
                        }
                        let active = session.as_ref().expect("validated session");
                        let http = self.http.clone();
                        let secret = active.secret.clone();
                        let Command::AcceptInvitation { request, ref target, destination, expires_at, .. } = work.command else { unreachable!() };
                        let target = target.clone();
                        let expires_at = Instant::from_std(expires_at);
                        let admitted = work.admitted;
                        rest = Some(RestOperation {
                            command: work.command, phase: RestPhase::InvitationPost, admitted,
                            future: Box::pin(async move {
                                // A busy event loop must never submit an old write
                                // after core has declared its 60-second deadline.
                                if admitted.elapsed() >= INVITATION_QUEUE_TTL + Duration::from_secs(15)
                                    || expires_at.saturating_duration_since(Instant::now()) < Duration::from_secs(15) {
                                    return Err(RestError::invitation(InvitationJoinError::Unavailable));
                                }
                                tokio::time::timeout_at((admitted + Duration::from_secs(50)).min(expires_at),
                                    http.accept_invitation(&secret, &target, destination, request, None)).await
                                    .unwrap_or_else(|_| Err(RestError::invitation(InvitationJoinError::Ambiguous)))
                            }),
                        });
                        continue;
                    }
                    let retried = retrying.take_if(|retried| *retried == work.command).is_some();
                    match result {
                        Ok(updates) => if let Some(active) = session.as_mut() {
                            // A thread is reported once it is kept beside its parent.
                            let updates = if crate::bootstrap::is_thread_command(&work.command) {
                                crate::bootstrap::thread_confirmed(&mut active.snapshot, &work.command, updates)
                            } else {
                                updates
                            };
                            // An administration answer changes the session as its dispatch would.
                            // So does a conversation of a server read by its id (P4).
                            let updates = if crate::admin::is_admin(&work.command)
                                || matches!(work.command, Command::LoadChannel { .. }) {
                                crate::admin::confirmed(&mut active.snapshot, self.http.take_echoes(), updates)
                            } else {
                                updates
                            };
                            // A read's answer confirms what it marked (P2).
                            let updates = if crate::reads::is_read(&work.command) {
                                let mut updates = updates;
                                updates.extend(active.reads.confirmed(&active.snapshot, &work.command));
                                updates
                            } else {
                                updates
                            };
                            // Rules accepted: the account takes part before the
                            // member's update arrives (2026-10-18).
                            if crate::onboarding::is_onboarding(&work.command) {
                                crate::onboarding::confirmed(&mut active.snapshot, &updates);
                            }
                            // Scheduled events: the account's own interest names it.
                            let updates = if crate::events::is_events(&work.command) {
                                crate::events::confirmed(&active.snapshot, updates)
                            } else {
                                updates
                            };
                            let own_write = matches!(work.command, Command::SendMessage { .. } | Command::SendSticker { .. } | Command::EditMessage { .. }
                                | Command::ForwardMessage { .. } | Command::CreatePoll { .. });
                            if own_write && updates.iter().any(|event| match event {
                                Event::MessageCreated { message, .. } | Event::MessageUpdated(message) =>
                                    active.snapshot.me.as_ref().is_none_or(|me| message.author != me.id),
                                _ => false,
                            }) {
                                fail(events, &work.command, wire::invalid(), true).await?;
                                continue;
                            }
                            if crate::account::is_account_command(&work.command) {
                                for event in &updates {
                                    active.account.observe(event);
                                }
                                active.presence.send_replace(Some(active.account.told()));
                            }
                            // Desktop (P6): a program's activity, now named, goes out.
                            if matches!(work.command, Command::SetLocalActivity(_)) {
                                if let Some(value) = active.local.take() {
                                    active.activity.send_replace(Some(value));
                                }
                            }
                            let account = active.snapshot.me.as_ref().map(|me| me.id);
                            let conversation = matches!(work.command, Command::OpenDirect { .. }
                                | Command::CreateGroup { .. } | Command::SetGroupOwner { .. }
                                | Command::RenameGroup { .. } | Command::SetGroupIcon { .. }
                                | Command::LeaveGroup { .. } | Command::CloseDirect { .. }
                                | Command::LoadChannel { .. });
                            for event in updates {
                                if conversation {
                                    crate::social::confirmed(&mut active.snapshot, &mut active.closed, &event);
                                }
                                if account.is_some_and(|account| !active.messages.first_confirmation(account, &event)) {
                                    continue;
                                }
                                active.messages.observe(&event);
                                active.reads.observe(&event);
                                emit(events, event).await?;
                            }
                        },
                        Err(mut error) => {
                            let challenge = error.challenge.take();
                            if error.invitation == Some(InvitationJoinError::ChallengeRequired) {
                                if let Some(issued) = challenge {
                                    held = Some(Held { command: work.command.clone(), issued: *issued, until: Instant::now() + CHALLENGE_LIFETIME });
                                }
                            } else if let Some(issued) = challenge.filter(|_| error.invitation.is_none()) {
                                // Any other write waits for the person, once.
                                match (!retried).then(|| challenges.offer(&work.command, issued, Duration::ZERO)).flatten() {
                                    Some(offer) => { emit(events, offer).await?; continue; }
                                    None => error.reason = if retried { challenges::AGAIN } else { challenges::BUSY },
                                }
                            }
                            if let Some(invitation) = error.invitation {
                                fail_invitation(events, &work.command, invitation).await?;
                            } else { fail_or_blocked(events, &work.command, error.blocked.take().map(|blocked| blocked.notice), error.reason, error.ambiguous).await?; }
                            if let Some(active) = session.as_mut() {
                                match work.command {
                                    // Back to what the account keeps.
                                    Command::SetStatus(_) | Command::SetCustomStatus { .. } => {
                                        active.presence.send_replace(Some(active.account.told()));
                                    }
                                    // Show them as they are now.
                                    Command::SetServerFolders(_) | Command::SetMessagingPrivacy(_)
                                    | Command::SetActivitySharing(_) => {
                                        active.account.stale = true;
                                    }
                                    _ => {}
                                }
                            }
                            if error.ends_session {
                                gateway = None;
                                incoming = None;
                                queue.clear();
                                upload = None;
                                let account = session.take().as_ref().and_then(account);
                                self.drop_saved(&mut vault, &mut remembered, account, events, false).await?;
                                emit(events, Event::Authentication(AuthenticationState::Failed {
                                    reason: error.reason.to_string(),
                                })).await?;
                            }
                        }
                    }
                }
            }
        }
    }
}

impl DiscordSessionProvider {
    /// Check a saved session with the same identity request as a new login.
    fn restore(&self, saved: Saved) -> Operation {
        let http = self.http.clone();
        let secrets = self.secrets.clone();
        let login = self.login.clone();
        Box::pin(async move {
            crate::browser::learn(&*login).await?;
            let Saved {
                account,
                mut secret,
            } = saved;
            let Ok(secret) = SessionSecret::try_from_local(std::mem::take(&mut *secret)) else {
                let _ = secrets.delete(SERVICE, &account).await;
                return Ok(Step::Revoked(secrets.list(SERVICE).await.ok()));
            };
            match http.identity(&secret).await {
                Ok(user) => {
                    let restored = user.id.0.to_string() == account;
                    if !restored {
                        // Saved under the wrong account: it is saved again
                        // under its own once verified.
                        let _ = secrets.delete(SERVICE, &account).await;
                    }
                    Ok(Step::Verified {
                        secret,
                        user: Box::new(user),
                        restored,
                    })
                }
                Err(error) if error.ends_session => {
                    let _ = secrets.delete(SERVICE, &account).await;
                    Ok(Step::Revoked(secrets.list(SERVICE).await.ok()))
                }
                Err(error) => Ok(Step::Unrestored {
                    account,
                    reason: error.reason,
                }),
            }
        })
    }

    /// The saved entry of `account`, bounded; a failing keyring has none.
    async fn load(&self, account: &str) -> Option<Saved> {
        tokio::time::timeout(VAULT_TIMEOUT, self.secrets.load(SERVICE, account))
            .await
            .ok()
            .and_then(Result::ok)
            .flatten()
    }

    /// Check `saved`, or start the login window without one.
    async fn enter(
        &self,
        saved: Option<Saved>,
        events: &EventSink,
    ) -> Result<Operation, ProviderError> {
        if let Some(saved) = saved {
            emit(
                events,
                Event::Authentication(AuthenticationState::Exchanging),
            )
            .await?;
            return Ok(self.restore(saved));
        }
        let login = self.login.clone();
        emit(
            events,
            Event::Authentication(AuthenticationState::AwaitingExternal),
        )
        .await?;
        Ok(Box::pin(async move {
            crate::browser::learn(&*login).await?;
            tokio::time::timeout(LOGIN_TIMEOUT, login.acquire())
                .await
                .map_err(|_| {
                    LoginError::new(es(
                        "El inicio de sesión venció. Podés iniciar otro intento.",
                    ))
                })?
                .map(Step::Acquired)
        }))
    }

    /// `restored`: the entry already exists, so a failed save leaves it.
    fn save(
        &self,
        account: String,
        name: String,
        avatar: String,
        secret: Arc<SessionSecret>,
        restored: bool,
    ) -> VaultOperation {
        let secrets = self.secrets.clone();
        Box::pin(async move {
            match secrets.save(SERVICE, &account, &name, &avatar, &secret.0).await {
                Ok(()) => VaultDone {
                    remembered: Some(true),
                    accounts: secrets.list(SERVICE).await.ok(),
                    notice: None,
                },
                Err(error) => VaultDone {
                    remembered: Some(restored),
                    accounts: None,
                    notice: (!restored).then(|| {
                        tr!(
                            "No se pudo recordar la sesión: {error} Sigue abierta hasta que cierres RUNE.",
                            error
                        )
                    }),
                },
            }
        })
    }

    /// `current`: `account` is the open session's.
    fn forget(&self, account: String, current: bool) -> VaultOperation {
        let secrets = self.secrets.clone();
        Box::pin(async move {
            let result = secrets.delete(SERVICE, &account).await;
            VaultDone {
                remembered: current.then_some(result.is_err()),
                accounts: secrets.list(SERVICE).await.ok(),
                notice: result
                    .err()
                    .map(|error| tr!("No se pudo borrar la sesión guardada: {error}", error)),
            }
        })
    }

    /// Stop remembering `account`, after the keyring operation in flight.
    async fn drop_saved(
        &self,
        vault: &mut Option<VaultOperation>,
        remembered: &mut bool,
        account: Option<String>,
        events: &EventSink,
        report: bool,
    ) -> Result<(), ProviderError> {
        settle(vault, remembered, events, report).await?;
        if let Some(account) = account.filter(|_| *remembered) {
            *vault = Some(self.forget(account, true));
        }
        Ok(())
    }
}

const SLOW_KEYRING: &str = es("El llavero del sistema no respondió a tiempo.");

fn account(session: &Session) -> Option<String> {
    session.snapshot.me.as_ref().map(|me| me.id.0.to_string())
}

/// The keyring's entries as the interface lists them; a foreign key is skipped.
fn remembered_accounts(entries: &[Entry]) -> Vec<RememberedAccount> {
    entries
        .iter()
        .filter_map(|entry| {
            let id = entry.account.parse().ok()?;
            Some(RememberedAccount {
                id: UserId(id),
                name: entry.name.clone(),
                // Rebuilt and checked like any avatar; without one, Discord's
                // default for a new-style name, as the account's own shows.
                avatar: wire::image("avatars", id, &serde_json::json!(entry.avatar))
                    .or_else(|| Some(wire::default_avatar(id, &serde_json::Value::Null, false))),
            })
        })
        .collect()
}

/// Wait, bounded, for the keyring operation in flight and report it.
async fn settle(
    vault: &mut Option<VaultOperation>,
    remembered: &mut bool,
    events: &EventSink,
    report: bool,
) -> Result<(), ProviderError> {
    let Some(pending) = vault.take() else {
        return Ok(());
    };
    let done = tokio::time::timeout(VAULT_TIMEOUT, pending)
        .await
        // Unknown outcome: assume it may be saved, so a deletion still runs.
        .unwrap_or_else(|_| VaultDone {
            remembered: Some(true),
            accounts: None,
            notice: Some(t(SLOW_KEYRING).into()),
        });
    finish(done, remembered, events, report).await
}

/// `report`: a session is open to say it of; notices always go out.
async fn finish(
    done: VaultDone,
    remembered: &mut bool,
    events: &EventSink,
    report: bool,
) -> Result<(), ProviderError> {
    if let Some(now) = done.remembered {
        *remembered = now;
        if report {
            emit(events, Event::SessionRemembered(now)).await?;
        }
    }
    if let Some(entries) = done.accounts {
        emit(
            events,
            Event::RememberedAccounts(remembered_accounts(&entries)),
        )
        .await?;
    }
    match done.notice {
        Some(reason) => notify(events, reason).await,
        None => Ok(()),
    }
}

/// A non-blocking notice that is not about any one command.
async fn notify(events: &EventSink, reason: String) -> Result<(), ProviderError> {
    emit(
        events,
        Event::Failed {
            failure: Failure::Other,
            reason,
            ambiguous: false,
        },
    )
    .await
}

fn handle_gateway(session: &mut Session, event: GatewayEvent) -> Result<Vec<Event>, LoginError> {
    match event {
        GatewayEvent::Session(id) => {
            session.apps.ready(id);
            Ok(Vec::new())
        }
        GatewayEvent::Ready(data) => {
            let me = session.snapshot.me.as_ref().ok_or_else(wire::invalid)?;
            if wire::id(&data["user"]["id"])? != me.id.0 {
                return Err(wire::invalid());
            }
            let id = me.id;
            let mut snapshot = bootstrap::ready(&data, me)?;
            // The threads read by page stay, as in the Store (2026-10-18).
            bootstrap::keep_paged_threads(
                &mut snapshot,
                std::mem::take(&mut session.snapshot.channels),
            );
            session.snapshot = snapshot;
            session.reads.ready(&data, &mut session.snapshot);
            session.messages = Messages::default();
            session.lists.restart();
            session.online = true;
            let mut events = vec![
                Event::Ready(Box::new(session.snapshot.clone())),
                Event::Connection(ConnectionState::Connected),
            ];
            events.extend(crate::account::ready(&data));
            // What the account's plan lets it use (P5).
            events.push(Event::Perks(crate::identity::perks(&data["user"])));
            // What the Gateway was told (invisible before the first read),
            // until the read says.
            events.push(crate::http::presence(id, &session.account.told()));
            session.account.stale = true;
            Ok(events)
        }
        GatewayEvent::Reconnecting {
            attempt,
            retry_in_ms,
        } => {
            session.online = false;
            Ok(vec![Event::Connection(ConnectionState::Reconnecting {
                attempt,
                retry_in_ms,
            })])
        }
        GatewayEvent::Resumed => {
            session.online = true;
            Ok(vec![Event::Connection(ConnectionState::Connected)])
        }
        GatewayEvent::Dispatch { kind, data } => {
            let kind = kind.as_str();
            let me = session.snapshot.me.as_ref().map(|me| me.id);
            if kind == "GUILD_MEMBER_LIST_UPDATE" {
                return Ok(session.lists.update(me, &data));
            }
            // Reads, search and remote members (P2).
            if kind == "MESSAGE_ACK" {
                return Ok(session.reads.dispatch(&session.snapshot, &data));
            }
            if kind == "GUILD_MEMBERS_CHUNK" {
                return Ok(session.lookups.chunk(&data));
            }
            if kind == "PRESENCE_UPDATE" {
                return Ok(session
                    .lists
                    .presence(me, &data)
                    .or_else(|| crate::social::presence(&session.snapshot, &data))
                    .into_iter()
                    .collect());
            }
            // Apps' answers to this session's interactions (P3).
            if let Some(events) = session.apps.dispatch(kind, &data) {
                return Ok(events);
            }
            // A closed direct conversation is listed again before its message.
            let reopened = (kind == "MESSAGE_CREATE")
                .then(|| crate::social::reopen(&mut session.snapshot, &mut session.closed, &data))
                .flatten();
            if let Some(events) =
                session
                    .messages
                    .dispatch(&session.snapshot, kind, &data, session.nonce_prefix)?
            {
                return Ok(reopened.into_iter().chain(events).collect());
            }
            if kind == "CHANNEL_DELETE" {
                if let Ok(channel) = wire::id(&data["id"]) {
                    crate::social::forget(
                        &mut session.snapshot,
                        &mut session.closed,
                        rune_model::ChannelId(channel),
                    );
                }
            }
            if kind == "READY_SUPPLEMENTAL" {
                let mut events = bootstrap::update(&mut session.snapshot, kind, &data)?;
                events.extend(crate::social::supplemental_presences(
                    &session.snapshot,
                    &data,
                ));
                // Rail and notifications.
                events.extend(crate::activities::supplemental(&session.snapshot, &data));
                return Ok(events);
            }
            // Rail and notifications.
            if kind == "EMBEDDED_ACTIVITY_UPDATE_V2" {
                return Ok(crate::activities::update(&session.snapshot, &data)
                    .into_iter()
                    .collect());
            }
            if kind == "TYPING_START" {
                return Ok(typing(&session.snapshot, &data).into_iter().collect());
            }
            if kind == "USER_UPDATE" {
                let user = wire::user(&data)?;
                if session
                    .snapshot
                    .me
                    .as_ref()
                    .is_some_and(|me| me.id == user.id)
                {
                    if user.bot {
                        return Err(wire::invalid());
                    }
                    session.snapshot.me = Some(user.clone());
                    // A change of plan changes what the account may use (P5).
                    if data.get("premium_type").is_some() {
                        let perks = Event::Perks(crate::identity::perks(&data));
                        return Ok(vec![Event::UserChanged(user), perks]);
                    }
                }
                return Ok(vec![Event::UserChanged(user)]);
            }
            if let Some(events) = crate::social::dispatch(&mut session.snapshot, kind, &data) {
                return Ok(events);
            }
            // AutoMod (automod).
            if let Some(events) = crate::automod::dispatch(&session.snapshot, kind, &data) {
                return Ok(events);
            }
            if let Some(events) = crate::admin::dispatch(&session.snapshot, kind, &data) {
                return Ok(events);
            }
            // Scheduled events (eventos-programados).
            if let Some(events) = crate::events::dispatch(&session.snapshot, kind, &data) {
                return Ok(events);
            }
            if let Some(mut events) =
                crate::account::dispatch(&mut session.account, &session.snapshot, kind, &data)
            {
                // Desktop session and games: whether what this device
                // publishes is seen.
                if kind == "SESSIONS_REPLACE" {
                    if let Some(published) = session.activity.borrow().as_ref() {
                        events.extend(
                            crate::desktop::visibility(&data, published)
                                .map(Event::ActivityVisibility),
                        );
                    }
                }
                return Ok(events);
            }
            let mut events = bootstrap::update(&mut session.snapshot, kind, &data)?;
            // Rail and notifications: the apps in its voice channels.
            if kind == "GUILD_CREATE" {
                events.extend(crate::activities::of_guild(&session.snapshot, &data));
            }
            Ok(events)
        }
    }
}

/// Someone else started typing in a conversation the account can read.
/// A malformed indicator is decoration: it is dropped, never a disconnect.
fn typing(snapshot: &Snapshot, data: &serde_json::Value) -> Option<Event> {
    let channel = rune_model::ChannelId(wire::id(&data["channel_id"]).ok()?);
    let user = UserId(wire::id(&data["user_id"]).ok()?);
    let readable = snapshot.channels.iter().any(|known| {
        known.id == channel
            && crate::messages::holds_chat(known.kind)
            && known.permissions.contains(rune_model::Permissions::VIEW)
    });
    (readable && snapshot.me.as_ref().is_some_and(|me| me.id != user))
        .then_some(Event::Typing { channel, user })
}

/// The one REST call of a command whose preflight, if any, is done. A
/// reaction goes the way the command asks, never the way a baseline that
/// may lag behind says: of two quick ones, the last wins.
fn write(
    http: Http,
    secret: Arc<SessionSecret>,
    request: Command,
    nonce: String,
) -> BoxFuture<'static, Result<Vec<Event>, RestError>> {
    Box::pin(async move {
        match &request {
            Command::ToggleReaction {
                channel,
                message,
                emoji,
                add,
            } => http.react(&secret, *channel, *message, emoji, *add).await,
            _ => http.execute(&secret, &request, &nonce).await,
        }
    })
}

fn invitation_allowed(
    session: Option<&Session>,
    command: &Command,
) -> Result<(), InvitationJoinError> {
    let Command::AcceptInvitation {
        target,
        destination,
        account,
        expires_at,
        ..
    } = command
    else {
        return Err(InvitationJoinError::Unsupported);
    };
    if Instant::from_std(*expires_at) <= Instant::now() {
        return Err(InvitationJoinError::Unavailable);
    }
    if !destination.valid() || crate::invitations::target(&target.url).as_ref() != Some(target) {
        return Err(InvitationJoinError::Invalid);
    }
    let active = session
        .filter(|active| active.online)
        .ok_or(InvitationJoinError::Unavailable)?;
    if active
        .snapshot
        .me
        .as_ref()
        .is_none_or(|me| me.id != *account)
    {
        return Err(InvitationJoinError::Denied);
    }
    // Already there: the server, the group, or a friend (or one's own link).
    let snapshot = &active.snapshot;
    let joined = match *destination {
        InvitationDestination::Server(server) | InvitationDestination::Guest { server, .. } => {
            snapshot.servers.iter().any(|known| known.id == server)
        }
        InvitationDestination::Group(channel) => {
            snapshot.channels.iter().any(|known| known.id == channel)
        }
        InvitationDestination::Friend(user) => {
            user == *account
                || snapshot.relationships.iter().flatten().any(|known| {
                    known.user == user && known.kind == rune_model::RelationshipKind::Friend
                })
        }
    };
    if joined {
        // One's own link is no invitation for oneself.
        return Err(if *destination == InvitationDestination::Friend(*account) {
            InvitationJoinError::Denied
        } else {
            InvitationJoinError::AlreadyJoined
        });
    }
    Ok(())
}

fn invitation_pending(queue: &VecDeque<Queued>, rest: &Option<RestOperation>) -> bool {
    queue
        .iter()
        .any(|queued| matches!(queued.command, Command::AcceptInvitation { .. }))
        || rest
            .as_ref()
            .is_some_and(|rest| matches!(rest.command, Command::AcceptInvitation { .. }))
}

fn friend_request_confirmed(command: &Command, updates: &[Event]) -> bool {
    let Command::AcceptFriendRequest { user } = command else {
        return false;
    };
    updates.iter().any(|event| {
        matches!(event, Event::RelationshipChanged(known)
            if known.user == *user && known.kind == rune_model::RelationshipKind::Friend)
    })
}

fn invalidated(active: &Session, work: &RestOperation) -> Option<LoginError> {
    match work.phase {
        RestPhase::ContextRead { account } => {
            if !active.online
                || active
                    .snapshot
                    .me
                    .as_ref()
                    .is_none_or(|me| me.id != account)
            {
                Some(LoginError::new(es(
                    "La sesión o la conexión ya no está disponible.",
                )))
            } else {
                active
                    .messages
                    .allowed(&active.snapshot, &work.command)
                    .err()
            }
        }
        RestPhase::MessagePreflight { account } => {
            if !active.online {
                Some(LoginError::new(es(
                    "La conexión todavía no está disponible.",
                )))
            } else {
                Messages::readable(&active.snapshot, &work.command, account).err()
            }
        }
        RestPhase::MessageWrite { account, author } => {
            if !active.online {
                Some(LoginError::new(es(
                    "La conexión todavía no está disponible.",
                )))
            } else {
                Messages::allowed_with_author(&active.snapshot, &work.command, account, author)
                    .err()
            }
        }
        RestPhase::InvitationPreflight => invitation_allowed(Some(active), &work.command)
            .err()
            .map(|error| LoginError::new(error.label())),
        RestPhase::InvitationPost => {
            // GUILD_CREATE can arrive before the HTTP acknowledgement. It must
            // not cancel this write or trigger another attempt.
            let Command::AcceptInvitation { account, .. } = work.command else {
                unreachable!()
            };
            (!active.online
                || active
                    .snapshot
                    .me
                    .as_ref()
                    .is_none_or(|me| me.id != account))
            .then(|| LoginError::new(InvitationJoinError::Ambiguous.label()))
        }
        RestPhase::Standard if crate::admin::is_admin(&work.command) => {
            crate::admin::in_flight(&active.snapshot, &work.command)
        }
        RestPhase::Standard => active
            .messages
            .allowed(&active.snapshot, &work.command)
            .err(),
    }
}

fn message_changed(command: &Command, updates: &[Event]) -> bool {
    if let Command::LoadContext { channel, .. } = command {
        // A context snapshot can contain any neighbor in this conversation.
        // Reject it before observing baselines if Gateway supplied newer data.
        return updates.iter().any(|event| match event {
            Event::MessageCreated { message, .. } | Event::MessageUpdated(message) => {
                message.channel == *channel
            }
            Event::MessagePatched(patch) => patch.channel == *channel,
            Event::MessageDeleted {
                channel: changed, ..
            }
            // A live reaction or vote changes a neighbor, too.
            | Event::ReactionChanged {
                channel: changed, ..
            }
            | Event::ReactionsCleared {
                channel: changed, ..
            }
            | Event::PollVoteChanged {
                channel: changed, ..
            }
            | Event::BurstReactionChanged {
                channel: changed, ..
            } => changed == channel,
            Event::Ready(_)
            | Event::Connection(
                ConnectionState::Connecting
                | ConnectionState::Reconnecting { .. }
                | ConnectionState::Disconnected { .. },
            ) => true,
            _ => false,
        });
    }
    let target = match command {
        Command::VotePoll {
            channel, message, ..
        }
        | Command::PinMessage { channel, message }
        | Command::UnpinMessage { channel, message } => Some((*channel, *message)),
        _ => write_target(command),
    };
    target.is_some_and(|(channel, message)| updates.iter().any(|event| {
        matches!(event, Event::MessageDeleted { channel: gone, message: id } if *gone == channel && *id == message)
            || matches!(event, Event::Ready(_))
    }))
}

/// The server of an open-threads page and whether its parent is a forum,
/// for a [`Command::LoadThreads`] of [`ThreadScope::Active`] only.
fn open_threads(snapshot: &Snapshot, command: &Command) -> Option<(rune_model::ServerId, bool)> {
    let Command::LoadThreads {
        parent,
        scope: rune_provider::ThreadScope::Active,
        ..
    } = command
    else {
        return None;
    };
    let found = snapshot.channels.iter().find(|known| known.id == *parent)?;
    Some((found.server?, found.kind == ChannelKind::Forum))
}

fn history_request(command: &Command, request: u64) -> bool {
    matches!(command, Command::LoadHistory { request: id, .. } | Command::LoadContext { request: id, .. }
        | Command::LoadPins { request: id, .. } | Command::SearchMessages { request: id, .. } if *id == request)
}

#[cfg(test)]
#[test]
fn context_mutation_invalidation_is_scoped_and_does_not_change_history() {
    use rune_model::{ChannelId, MessageId, MessagePatch};
    let message = wire::message(&serde_json::json!({
        "id":"99","channel_id":"20","author":{"id":"42","username":"Owner"},
        "content":"current","timestamp":"2026-10-14T12:34:56Z","attachments":[],"mentions":[]
    }))
    .unwrap()
    .0;
    let context = Command::LoadContext {
        request: 1,
        channel: ChannelId(20),
        anchor: MessageId(100),
        limit: 3,
    };
    let other_context = Command::LoadContext {
        request: 2,
        channel: ChannelId(21),
        anchor: MessageId(100),
        limit: 3,
    };
    let history = Command::LoadHistory {
        request: 1,
        channel: ChannelId(20),
        before: None,
        limit: 3,
    };
    for update in [
        Event::MessageCreated {
            message: message.clone(),
            nonce: None,
        },
        Event::MessageUpdated(message),
        Event::MessagePatched(MessagePatch {
            channel: ChannelId(20),
            id: MessageId(99),
            content: Some("current".into()),
            ..Default::default()
        }),
        Event::MessageDeleted {
            channel: ChannelId(20),
            message: MessageId(99),
        },
        Event::ReactionChanged {
            channel: ChannelId(20),
            message: MessageId(99),
            emoji: "🦀".into(),
            me: false,
            added: true,
        },
        Event::BurstReactionChanged {
            channel: ChannelId(20),
            message: MessageId(99),
            emoji: "🦀".into(),
            me: false,
            added: true,
            colors: Vec::new(),
        },
        Event::ReactionsCleared {
            channel: ChannelId(20),
            message: MessageId(99),
            emoji: None,
        },
        Event::PollVoteChanged {
            channel: ChannelId(20),
            message: MessageId(99),
            answer: 1,
            me: false,
            added: true,
        },
    ] {
        assert!(
            message_changed(&context, std::slice::from_ref(&update)),
            "{update:?}"
        );
        assert!(!message_changed(
            &other_context,
            std::slice::from_ref(&update)
        ));
        assert!(!message_changed(&history, std::slice::from_ref(&update)));
    }
}

async fn cancel_work(
    events: &EventSink,
    rest: &mut Option<RestOperation>,
    queue: &mut VecDeque<Queued>,
    reason: LoginError,
) -> Result<(), ProviderError> {
    if let Some(work) = rest.take() {
        let ambiguous = work.phase.ambiguous(&work.command);
        drop(work.future);
        fail(events, &work.command, reason, ambiguous).await?;
    }
    for queued in queue.drain(..) {
        fail(events, &queued.command, reason, false).await?;
    }
    Ok(())
}

/// Where public pictures may load from, without the account's credentials.
pub(crate) const IMAGE_ORIGINS: [&str; 12] = [
    "https://cdn.discordapp.com",
    "https://media.discordapp.net",
    "https://images-ext-1.discordapp.net",
    "https://images-ext-2.discordapp.net",
    // The pictures of Discord's GIF search (P5), as Serein loads them.
    "https://static.klipy.com",
    "https://static1.klipy.com",
    "https://static2.klipy.com",
    "https://media.tenor.com",
    "https://c.tenor.com",
    // Activity covers of Spotify, Twitch and YouTube, as the official
    // client loads them (owner-approved 2026-10-18, SECURITY.md).
    "https://i.scdn.co",
    "https://static-cdn.jtvnw.net",
    "https://i.ytimg.com",
];

impl Provider for DiscordSessionProvider {
    fn invitation_target(&self, url: &str) -> Option<rune_provider::InvitationTarget> {
        crate::invitations::target(url)
    }

    fn inspect_invitation(
        &self,
        target: rune_provider::InvitationTarget,
    ) -> BoxFuture<'_, Result<rune_provider::InvitationPreview, rune_provider::InvitationError>>
    {
        Box::pin(crate::invitations::inspect(target))
    }

    fn image_origins(&self) -> &'static [&'static str] {
        &IMAGE_ORIGINS
    }

    fn image_at(&self, url: &str, density: f32) -> Option<String> {
        crate::wire::sized(url, density)
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "Discord".into(),
            kind: ProviderKind::Unofficial,
            capabilities: Capabilities {
                invitation_preview: true,
                // One explicit write; a challenge is solved by the person in
                // the host's own window (owner-authorized 2026-10-16).
                invitation_accept: true,
                authentication: true, saved_accounts: true, account_servers: true, direct_messages: true,
                history: true, message_context: true, send_messages: true, edit_messages: true, delete_messages: true, stickers: true,
                // Same queue, permission recheck and single attempt as message writes.
                poll_votes: true, reactions: true, typing: true, pins: true,
                // Friend requests, blocks, nicknames, opening and closing direct
                // conversations, and a group's name, picture and leaving, by the
                // routes Serein uses (owner rule «si lo usa Serein»); fixtures
                // only. Making groups, changing their people and handing them
                // over by the official web client's routes (REA); fixtures only.
                relationships: true, friend_requests: true, block_users: true,
                friend_nicknames: true, open_direct: true, close_direct: true,
                manage_groups: true,
                // Friends' presence from READY and the Gateway, as Serein reads it.
                friend_presence: true,
                // The full profile and the member list by subscription, as
                // Serein reads them (owner-authorized 2026-10-16); fixtures only.
                profiles: true, member_list: true,
                // Server voice channels with DAVE, as Serein connects them
                // (owner-authorized 2026-10-16); fixtures only, no live call yet.
                voice: VoiceSupport::Real,
                // Others' screen streams in the call, H264 through DAVE;
                // fixtures only, no live stream yet.
                watch_streams: true,
                // The others' cameras over the call's connection, likewise.
                watch_cameras: true,
                // The account's screen as Go Live, H264 through DAVE, at
                // the quality chosen with no tier check; fixtures only.
                share_screen: true,
                // The account's camera over the call's connection, H264
                // through DAVE; fixtures only.
                video: true,
                // A voice channel's chat goes through the same REST and Gateway
                // paths as a text channel's; fixtures only.
                voice_text: true,
                // A stream's picture by the route Serein reads; fixtures only.
                stream_previews: true,
                // Streams (transmisiones): the account's own picture goes up
                // as the official client sends it (REA); fixtures only.
                upload_stream_previews: true,
                // Files staged to Discord's storage and named by the message,
                // as Serein sends them; fixtures only, never tried for real.
                attachments: true,
                // Threads and forum posts: starting, following, closing, locking,
                // renaming, tagging, deleting and archived pages, by the routes
                // Serein uses; fixtures only.
                create_threads: true,
                thread_pages: true,
                follow_threads: true,
                manage_threads: true,
                thread_replies: true,
                channel_links: Some("https://discord.com/channels"),
                // Stages by the routes the official web client calls (REA of
                // its public bundle); Serein has none. Fixtures only.
                stages: true,
                // The account's profile: name, avatar, «Sobre mí», pronouns,
                // color and banner, and its profile in each server, by the
                // routes Serein and the official web client use; fixtures only.
                edit_profile: true,
                profile_images: true,
                server_profiles: true,
                create_groups: true,
                group_members: true,
                group_owner: true,
                // Servers, channels, overwrites, roles, members, invitations,
                // expressions and the audit log by the routes Serein uses;
                // fixtures only.
                manage_servers: true, manage_channels: true, moderation: true, audit_log: true,
                manage_invites: true, manage_expressions: true, create_invites: true,
                upload_expressions: true, invite_links: Some("https://discord.gg/"),
                // Bans, timeouts and the audit log reason by Discord's
                // documented routes (Serein has none); fixtures only.
                timeouts: true,
                // The member search, prune, webhooks and integrations, as
                // Serein reads and changes them; fixtures only.
                member_pages: true, prune: true, webhooks: true, integrations: true,
                // GIF favorites in the settings protobuf Serein keeps them in;
                // fixtures only.
                synced_gif_favorites: true,
                // The account's status and custom status (op 3 and
                // `settings-proto/1`), notifications by server and channel,
                // folders, messaging privacy and notes, by the routes Serein
                // uses and the official web client's schema (REA); fixtures only.
                presence: true,
                custom_status: true,
                synced_notifications: true,
                server_folders: true,
                messaging_privacy: true,
                notes: true,
                // Reads, search and remote members (P2): acknowledgements,
                // reminders and a whole server read by the routes Serein
                // uses, other devices' reads by `MESSAGE_ACK`, the search
                // route with its filters and members looked up by name over
                // the Gateway (op 8), as Serein does; fixtures only.
                read_state: true,
                server_read: true,
                remote_unread: true,
                message_search: true,
                member_search: true,
                // Apps' parts, modals and commands with suggestions, by
                // `POST /interactions` with the Gateway's session as Serein
                // and the official web client (REA) send them; fixtures only.
                interactions: true,
                app_commands: true,
                // Message actions and moderation (P4): forwards, polls and
                // ending them, who reacted, silent messages, replies without
                // a mention and voice moderation by the routes Serein uses;
                // who voted, server deafening and a conversation read by its
                // id by Discord's documented routes. Leaving needs no
                // management. Links of Discord's own hosts open without
                // asking. Fixtures only.
                forward_messages: true,
                create_polls: true,
                reaction_lists: true,
                poll_voters: true,
                load_channels: true,
                silent_messages: true,
                reply_mentions: true,
                voice_moderation: true,
                leave_servers: true,
                trusted_links: &["discord.com", "discord.gg", "discordapp.com", "discordapp.net"],
                // Desktop, language and local voice (P6): a program's or a
                // game's activity in the presence update, its name and
                // pictures looked up as Serein does; Discord's public list
                // of games; DAVE's call and per-person codes. Fixtures only.
                local_activity: true,
                detectable_games: true,
                voice_privacy_code: true,
                // Standard packs and Discord's own GIF search by the routes of
                // Serein and the official web client (REA); fixtures only.
                sticker_packs: true,
                gif_search: true,
                // Server administration, rest (P7): the server's picture,
                // traits and safety, paused invitations, channels moved at
                // once, a forum's settings, a role's picture, emoji and
                // gradient, the count of each role and member pages in their
                // order, by the routes Serein uses; fixtures only.
                server_identity: true,
                server_safety: true,
                pause_invites: true,
                reorder_channels: true,
                forum_settings: true,
                role_styles: true,
                role_counts: true,
                member_queries: true,
                // Private calls: in direct and group conversations, by
                // the official client's routes and Serein's; fixtures only.
                private_calls: true,
                // Invitations of every kind: the same accept route the
                // official client uses for all of them; fixtures only.
                group_invitations: true,
                friend_invitations: true,
                guest_invitations: true,
                // The Gateway's wait between attempts, cut short on request.
                reconnect_now: true,
                // `GET /channels/{id}/call` before a direct call, as the
                // official client asks (llamadas-y-panel-de-voz); fixtures only.
                call_check: true,
                // Rules and onboarding: read, answered and edited by the
                // official web client's routes (REA), as Serein reads and
                // answers them; fixtures only.
                server_rules: true,
                onboarding: true,
                manage_onboarding: true,
                // A followed thread silenced by the member settings route
                // Serein and the official web client use; fixtures only.
                mute_threads: true,
                // Desktop session and games: `show_current_game` of
                // `settings-proto/1` and `SESSIONS_REPLACE`, as Serein reads
                // them; fixtures only.
                activity_sharing: true,
                // A server's banner and becoming a community, by the
                // documented guild route the official client saves with
                // (REA, 2026-10-18); fixtures only.
                server_banner: true,
                community: true,
                // Scheduled events: the official web client's routes (REA)
                // and the documented ones; Serein has none. Fixtures only.
                scheduled_events: true,
                manage_scheduled_events: true,
                // AutoMod: the routes Discord documents and the official
                // web client uses (REA); fixtures only.
                automod: true,
                ..Default::default()
            },
            notice: es("Sesión personal, recordada en el llavero del sistema si hay uno. Conversaciones, mensajes, voz en canales de servidor y mirar transmisiones y cámaras, compartir la pantalla y enviar la cámara (sin probar todavía con una llamada real), llamadas en privados y grupos (sin probar todavía contra Discord), invitaciones con verificación, amigos, privados, grupos, hilos, foros, administración de servidores, lecturas sincronizadas y búsqueda en todo el historial (sin probar todavía con una cuenta real).").into(),
        }
    }

    fn upload_limits(&self) -> Option<rune_provider::UploadLimits> {
        Some(crate::uploads::CEILING)
    }

    fn take_voice_link(&self) -> Option<rune_provider::voice::VoiceLink> {
        self.voice_link
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    fn take_video_link(&self) -> Option<rune_provider::video::VideoLink> {
        self.video_link
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    fn connect<'a>(
        &'a self,
        commands: &'a mut mpsc::Receiver<Command>,
        events: &'a EventSink,
    ) -> BoxFuture<'a, Result<(), ProviderError>> {
        Box::pin(self.run(commands, events))
    }
}

async fn fail(
    events: &EventSink,
    command: &Command,
    reason: LoginError,
    ambiguous: bool,
) -> Result<(), ProviderError> {
    if matches!(
        command,
        Command::AcceptInvitation { .. } | Command::VerifyInvitation { .. }
    ) {
        return fail_invitation(
            events,
            command,
            if ambiguous {
                InvitationJoinError::Ambiguous
            } else {
                InvitationJoinError::Unavailable
            },
        )
        .await;
    }
    if let Some(failure) = Failure::of(command) {
        // Both labels are fixed local text: never log a command's payload,
        // account identifiers, credentials or a remote response body.
        eprintln!(
            "rune: action failed: {}: {reason} (ambiguous={ambiguous})",
            failure.action()
        );
        emit(
            events,
            Event::Failed {
                failure,
                reason: reason.to_string(),
                ambiguous,
            },
        )
        .await?;
    }
    Ok(())
}

// AutoMod (automod).
/// As [`fail`], but a write a server's AutoMod refused (`blocked`, with
/// what the server tells whoever wrote it) says so, for the core to word.
async fn fail_or_blocked(
    events: &EventSink,
    command: &Command,
    blocked: Option<Option<String>>,
    reason: LoginError,
    ambiguous: bool,
) -> Result<(), ProviderError> {
    match (blocked, Failure::of(command)) {
        (Some(notice), Some(failure)) => {
            emit(events, Event::AutoModBlocked { failure, notice }).await
        }
        _ => fail(events, command, reason, ambiguous).await,
    }
}

async fn fail_invitation(
    events: &EventSink,
    command: &Command,
    error: InvitationJoinError,
) -> Result<(), ProviderError> {
    // What the person is told, by name: after the step's HTTP line, if one
    // was sent; alone, RUNE refused before sending anything.
    crate::voice::diag::write("invitation", format_args!("not accepted: {error:?}"));
    if let Some(Failure::Invitation {
        request,
        destination,
    }) = Failure::of(command)
    {
        emit(
            events,
            Event::InvitationAcceptance {
                request,
                destination,
                result: Err(error),
            },
        )
        .await?;
    }
    Ok(())
}

async fn emit(events: &EventSink, event: Event) -> Result<(), ProviderError> {
    events
        .send(event)
        .await
        .map_err(|_| ProviderError::Fatal("La interfaz se cerró.".into()))
}

/// Whether `a` and `b` are the same file send: their bytes are not compared.
fn same_upload(a: &Command, b: &Command) -> bool {
    matches!((a, b), (Command::SendFiles { scope: x, .. }, Command::SendFiles { scope: y, .. })
        if x.request == y.request && x.nonce == y.nonce)
}
