//! RUNE's window: what it shows for the state ([`view`]), and what the
//! person's actions ask of the services ([`update`], [`intercept`],
//! [`after_layout`]). It owns only the interface's own state ([`Ui`]): what
//! is selected, open or being written. Everything it wants done outside the
//! interface it returns as [`Intent`]s, which the host carries out, so the
//! whole window runs and is tested without a provider or a screen.

// The account's synced settings (P1).
mod account;
mod alerts;
mod call_sounds;
// Commands and interactions (P3).
mod attachments;
mod auth;
mod challenges;
mod channel_list;
mod channel_settings;
mod commands;
mod composer_selection;
mod context;
mod conversations;
mod documents;
mod expressions;
mod gifs;
mod images;
mod inbox;
mod interactions;
mod invitation_cards;
mod invitations;
mod messages;
mod navigation;
mod profile;
mod providers;
mod reads;
mod relationships;
mod save_as;
mod search;
mod server_menu;
mod server_settings;
// Administration (19).
mod gif_favorites;
mod person_menu;
mod settings;
mod stage;
mod threads;
mod timeline;
mod uploads;
mod video;
mod view;
mod voice;
mod voice_messages;
// Message actions and moderation (P4).
mod message_actions;
// Desktop, language and local voice (P6).
pub mod desktop;
// Presence, identity and expressions (P5).
mod identity;
// Stability: settings kept by account.
mod account_settings;
// Private calls.
mod private_calls;
// Streams (transmisiones).
mod streams;
pub use account_settings::ACCOUNT_SETTINGS;
pub use private_calls::{
    answer_alert, call_alerts, ring_tick, ringing as call_ringing, ANSWER as CALL_ANSWER,
    DECLINE as CALL_DECLINE,
};
// Interface languages (i18n).
mod language;
// The screen until the session is loaded (pulido-ui-real).
pub mod splash;
// The quick voice panel (2026-10-18).
mod quick_voice;
// limn adoption (2026-10-18).
mod color_picker;
// Sends and uploads (envios-y-subidas).
mod emoticons;
// Rail and notifications.
mod rail;
// Emoji and states (2026-10-18).
mod emoji_card;
// Profile editing (edicion-de-perfil).
mod framing;
pub use framing::PictureSource;
// Reading settings and pictures on disk (imagenes-y-lectura).
pub mod reading;
// Rules and onboarding (2026-10-18).
mod onboarding;
// Channels and voice administration (admin-canales-y-voz).
mod voice_drag;
// People's rows in the column: the card of someone live (voz-filas-y-arrastre).
mod voice_live;
// Scheduled events (eventos-programados).
mod events;
// Desktop session and games (escritorio-sesion-y-juegos).
pub mod games;
// Editable shortcuts (atajos-y-navegacion).
pub mod keybinds;
// Own themes, main color and transparency (temas-propios).
pub mod themes;
// RUNE's own updates (actualizaciones).
pub mod updates;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use limn::{Key, UiAction, UiEvent, UiTree};
use rune_i18n::{t, tr};
use rune_model::{ChannelId, ChannelKind, MessageId, ServerId, Timestamp, UserId};
use rune_provider::{Command, ConnectionState};
use rune_state::{Applied, Store};

pub use alerts::{message_alert, open_alert, Alert};
pub use attachments::{follow_viewer, AttachmentViewer, Viewed};
pub use channel_list::{ChannelList, ChannelNotify};
pub use context::open_message;
pub use documents::{document_result, document_text, DocumentSearch, DocumentViewer};
pub use gifs::{gif_result, sync_gif_search, GifPickerState};
pub use identity::activity_tick;
pub use images::{playable_images, wanted_images};
pub use interactions::interaction_tick;
pub use invitation_cards::{invitation_card_result, wanted_invitation_cards};
pub use invitations::{
    invitation_input_result, invitation_result, open_invitation, InvitationDialog,
};
pub use messages::{completions, Messages};
pub use navigation::{cycle_focus, toggle_shortcuts, travel};
pub use profile::{next_tick, profile_image_picked, tick};
pub use providers::{origin, provider_changed, ProviderOption, Providers, UNOFFICIAL};
pub use save_as::{
    attachment_download, save_finished, save_progress, save_saving, SaveAs, SavePhase,
};
// Linux desktop (escritorio-linux).
pub use call_sounds::sound_folder_chosen;
pub use save_as::save_exists;
pub use search::MessageSearch;
pub use search::{local_history_changed, local_messages_changed, local_search_result};
pub use server_menu::{Dialog, Notify, NotifyLevel, ServerMenu};
pub use server_settings::expression_image_picked;
pub use settings::{
    record_key, recording_key, Cosmetics, InputProfile, Prefs, Processing, SettingsPage,
    SettingsState,
};
pub use threads::post_refused;
pub use uploads::{
    attachment_pick_pending, attachment_send_admitted, attachment_send_pending,
    begin_attachment_import, set_upload_limits, upload_update, AttachmentImportSource,
};
// Sends and uploads (envios-y-subidas).
pub use uploads::{dirty_draft_files, draft_files_dirty, draft_files_kept};
pub use video::{
    camera_sending as video_camera_sending, cameras as video_cameras, ended as video_ended,
    fullscreen as video_fullscreen, idle_tick as stream_idle_tick,
    screen_chosen as video_screen_chosen, sending as video_sending, sound as video_sound,
    sound_failed as video_sound_failed, stir, stir_window as video_window_stirred,
    stream_previews as video_previews, window_closed as video_window_closed,
    window_idle as video_window_idle, window_view as video_window, windows as video_windows, Live,
    Share, Video,
};
// Streams (transmisiones).
pub use video::{
    own_preview_due as video_own_preview, own_preview_failed as video_own_preview_failed,
};
// The call's sounds and their settings (audio work).
pub use call_sounds::{
    folder_read as call_sounds_folder_read, take_preview as call_sounds_take_preview, CallSound,
    CallSounds, FolderReport,
};
// The shared screen's stream starts once its source runs (audio work).
pub use video::{capture_quality as video_capture_quality, screen_ready as video_screen_ready};
// RUNE's own screen picker, where the system has none (windows-medios).
pub use video::sources_listed as video_sources_listed;
pub use view::view;
// The window's own title bar (barra-e-icono).
pub mod title_bar;
pub use voice::call_tick;
// Voice protocol (voz-protocolo).
pub use voice::voice_since_tick;
pub use voice_messages::{voice_loaded, voice_speed, Played, VoicePlayer, VoiceState};
// Message actions and moderation (P4).
pub use message_actions::{moments_tick, MessageActions};

/// Something the interface wants done outside itself.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    /// Pass to the provider.
    Command(Command),
    /// Ask for a conversation's newest page, and what the local store has.
    LoadLatest(ChannelId),
    LoadOlder(ChannelId, MessageId),
    SearchLocal {
        request: u64,
        query: rune_model::SearchQuery,
        cursor: Option<rune_model::SearchCursor>,
    },
    LoadContext {
        request: u64,
        channel: ChannelId,
        anchor: MessageId,
        source: rune_model::HistorySource,
    },
    /// Stop waiting for pages of other conversations.
    CancelHistoryExcept(ChannelId),
    /// Send a message, showing it at once.
    Send {
        channel: ChannelId,
        content: String,
        reply_to: Option<MessageId>,
        /// A reply notifies who it answers (P4).
        mention_reply: bool,
    },
    SendSticker {
        channel: ChannelId,
        sticker: rune_model::Sticker,
    },
    PickAttachment {
        request: rune_provider::RequestId,
        channel: ChannelId,
    },
    CancelUpload {
        request: rune_provider::RequestId,
    },
    RemoveAttachment {
        channel: ChannelId,
        token: rune_model::UploadToken,
    },
    SendAttachments {
        request: rune_provider::RequestId,
        channel: ChannelId,
        content: String,
        reply_to: Option<MessageId>,
        files: Vec<rune_model::UploadToken>,
        /// As in [`Intent::Send`]: a reply notifies who it answers.
        mention_reply: bool,
    },
    RetryAttachmentSend {
        request: rune_provider::RequestId,
        channel: ChannelId,
        message: MessageId,
    },
    /// Replace the text of a field.
    SetInput {
        id: String,
        value: String,
    },
    Focus(String),
    /// Focus an input and then restore its Unicode character caret. The host
    /// defers this until layout only when the field does not exist yet.
    FocusInput {
        id: String,
        cursor: usize,
    },
    ScrollToEnd(String),
    /// Scroll the list `list` to its row `index`.
    ScrollToIndex {
        list: String,
        index: usize,
    },
    Copy(String),
    OpenUrl(String),
    /// Open this address in the system browser as it is. Unlike
    /// [`Intent::OpenUrl`], the host never reads it as an invitation to
    /// review in RUNE.
    OpenInBrowser(String),
    /// Save a user-selected remote attachment to the configured Downloads folder.
    DownloadFile {
        source: rune_model::RemoteFile,
        filename: String,
        size_bytes: u64,
    },
    PreviewDocument {
        request: u64,
        source: rune_model::RemoteFile,
        size: u64,
    },
    /// Another page of the document open for `request`, with `marks`
    /// highlighted on it; replaces one still rendering.
    DocumentPage {
        request: u64,
        page: u32,
        marks: Vec<rune_model::PdfMark>,
    },
    CancelDocument {
        request: u64,
    },
    /// Read the text of the document open for `request`, for its search.
    DocumentText {
        request: u64,
    },
    /// Fetch a voice message's audio, or an audio or video file, and play it
    /// as [`Ui::voice_player`] says; `source` is `None` when the provider
    /// gave no file to fetch.
    PlayVoice {
        request: u64,
        source: Option<rune_model::RemoteFile>,
        size: u64,
        /// What it is, which picks the sample a simulated one plays.
        kind: Played,
    },
    /// The voice message of `request` is no longer wanted.
    StopVoice {
        request: u64,
    },
    /// Ask the adapter to recognize a pasted invitation without opening its URL.
    InspectInvitation(String),
    /// Review a current adapter-tagged message, without a browser fallback.
    ReviewInvitation {
        channel: ChannelId,
        message: MessageId,
        account: rune_model::UserId,
        source: String,
    },
    PreviewInvitation {
        request: u64,
        target: rune_provider::InvitationTarget,
    },
    CancelInvitation(u64),
    SearchGifs {
        request: u64,
        query: rune_provider::GifQuery,
        offset: u32,
    },
    CancelGifSearch {
        request: u64,
    },
    /// Keep a setting for the next run.
    SaveSetting {
        key: String,
        value: Option<String>,
    },
    /// Send the unsent message `message` again: the host picks the nonce,
    /// the same one only when the provider's sends are idempotent.
    Resend {
        channel: ChannelId,
        message: MessageId,
    },
    /// Drop the simulated connection, to see the client reconnect.
    SimulateDisconnect,
    /// Have the simulated service refuse the next message.
    SimulateSendFailure,
    /// Have the simulated service never answer the next message.
    SimulateSendSilence,
    // Private calls.
    /// Have someone of the simulation call the account.
    SimulateIncomingCall,
    /// Have the simulation answer the call ringing the account on another
    /// of its devices.
    SimulateCallElsewhere,
    /// Delete everything the local store keeps for the account.
    ForgetLocalData,
    /// Start measuring the microphone `id`, or the default one.
    StartMeter(Option<String>),
    StopMeter,
    RefreshDevices,
    /// Apply reduced motion to the tree.
    ReducedMotion(bool),
    /// Scroll so the anchor or control `id` starts `margin` under the top of
    /// its scroll area, as a table of contents jumps to a section.
    Reveal {
        id: String,
        margin: f32,
    },
    /// Paint again: what the last layout showed changes what to show.
    Repaint,
    /// Let the person choose a picture for the group `channel`, through the
    /// system's selector, and send it as the group's icon.
    PickGroupIcon {
        channel: ChannelId,
    },
    // Attachments and platform (07/08).
    /// Ask where to save the attachment, then download it there. Only IDs:
    /// the host resolves the attachment as the conversation shows it now.
    SaveAttachmentAs {
        request: u64,
        channel: ChannelId,
        message: MessageId,
        attachment: rune_model::AttachmentId,
    },
    /// Close the dialog of that save, or stop its download.
    CancelSaveAttachment {
        request: u64,
    },
    /// Prepare the files or the picture on the clipboard for `channel`'s
    /// draft, as [`Intent::PickAttachment`] prepares chosen ones.
    PasteAttachments {
        request: u64,
        channel: ChannelId,
    },
    // Accounts and profiles.
    /// Let the person choose a picture of `kind` for the profile being
    /// edited, through the system's selector; the host answers with
    /// [`profile_image_picked`]. Nothing is sent until it is saved.
    PickProfileImage {
        request: u64,
        kind: rune_provider::ProfileImageKind,
        /// Whether a moving picture stays moving (edicion-de-perfil).
        animated: bool,
    },
    /// Stop the choice of a profile picture under way.
    CancelProfileImage,
    /// Start the provider the host lists by this key in place of the one in
    /// use; the host answers with [`provider_changed`].
    UseProvider(&'static str),
    // Threads and forums.
    /// Start a post in `forum` whose first message carries `files`, the
    /// ones prepared for it: the files' bytes join the command away from
    /// the interface. Answered as [`Command::CreatePost`] is.
    CreatePost {
        request: rune_provider::RequestId,
        forum: ChannelId,
        title: String,
        tags: Vec<rune_model::TagId>,
        content: String,
        files: Vec<rune_model::UploadToken>,
    },
    // Administration (19).
    /// Let the person choose the picture of a new emoji or sticker through
    /// the system's selector; the host answers with
    /// [`expression_image_picked`]. Nothing is sent until it is confirmed.
    PickExpressionImage {
        request: u64,
        kind: rune_provider::ExpressionKind,
    },
    // Message actions (P4).
    /// Focus an input and select its Unicode characters `start..end`, as
    /// [`Intent::FocusInput`] places its caret.
    SelectInput {
        id: String,
        start: usize,
        end: usize,
    },
    // Desktop, language and local voice (P6).
    /// Start RUNE with the desktop session, or no longer, in the tray when
    /// `minimized`; the host answers with [`desktop::autostart_known`].
    SetAutostart {
        on: bool,
        minimized: bool,
    },
    /// List the system's font families; the host answers with
    /// [`desktop::fonts_listed`].
    ListFonts,
    // Message content (contenido-de-mensajes).
    /// Fetch an attachment and put it on the system clipboard: a picture's
    /// pixels (`image`), or else the file itself.
    CopyFile {
        source: rune_model::RemoteFile,
        filename: String,
        size_bytes: u64,
        image: bool,
    },
    // Apps' modals with files (comandos-de-apps).
    /// Send `command`, an app's modal answered, with `files`, the ones
    /// prepared for its channel that its answers name: their bytes join it
    /// away from the interface. Answered as [`Command::Interact`] is; a
    /// refusal is reported to the store as the app's would be.
    InteractWithFiles {
        command: Command,
        files: Vec<rune_model::UploadToken>,
    },
    // Profile editing (edicion-de-perfil).
    /// Frame the picture last chosen for the profile again, as `crop`
    /// says, keeping it moving when `animated` allows it; the host answers
    /// with [`profile_image_picked`], as for a choice.
    FrameProfileImage {
        request: u64,
        kind: rune_provider::ProfileImageKind,
        crop: rune_provider::Crop,
        animated: bool,
    },
    // Reading settings and pictures on disk (imagenes-y-lectura).
    /// Delete one part of the account's pictures kept on disk now.
    ClearImageCache(reading::CachePart),
    // Linux desktop (escritorio-linux).
    /// Replace the file at the name chosen for that «Save as», as the
    /// person confirmed after [`save_exists`].
    ReplaceSavedFile {
        request: u64,
    },
    /// Let the person choose the folder of the call's sounds through the
    /// system's dialog; the host answers with [`sound_folder_chosen`].
    ChooseSoundFolder {
        request: u64,
    },
    /// Close the dialog [`Intent::ChooseSoundFolder`] opened: the account it
    /// was opened for is gone.
    CancelSoundFolder,
    /// Who draws the window's title bar (barra-e-icono), now and from the
    /// next start.
    TitleBar(title_bar::TitleBar),
    /// Draw with the GPU of that key (`None`: as RUNE chooses) from the
    /// next start; see [`desktop::Graphics`].
    DrawWith(Option<String>),
    // Desktop session and games (escritorio-sesion-y-juegos).
    /// List this user's programs open now, to add one as a game; the host
    /// answers with [`games::programs_listed`].
    ListPrograms,
    /// Send the providers' REST requests as `proxy` says, from the next
    /// start; the host answers with [`desktop::proxy_known`].
    SetProxy(rune_provider::ApiProxy),
    // Editable shortcuts and navigation (atajos-y-navegacion).
    /// Scroll the area `list` to `offset` pixels from its top, at once: a
    /// place returned to, to the pixel.
    SetScrollOffset {
        list: String,
        offset: f32,
    },
    // Own themes, main color and transparency (temas-propios).
    /// Let the person choose a theme file through the system's dialog and
    /// read it, bounded; the host answers with [`themes::imported`].
    ImportTheme {
        request: u64,
    },
    /// Ask where to save `contents`, a theme file, suggesting `filename`,
    /// and write it there, never over another file; the host answers with
    /// [`themes::exported`].
    ExportTheme {
        request: u64,
        filename: String,
        contents: String,
    },
    /// Let this percent of the desktop show through the window: now if it
    /// opened able to, else from the next start, which the host keeps.
    SetTransparency(u8),
    // RUNE's own updates (actualizaciones).
    /// Look for a newer RUNE now; the host keeps [`Ui::updates`] current.
    CheckForUpdates,
    /// Download the update offered, install it and start RUNE again.
    InstallUpdate,
    /// Look for a newer RUNE when it starts, or no longer.
    CheckUpdatesAtStart(bool),
    // The window's own title bar (barra-e-icono).
    /// Minimize, maximize or restore, or close the window, as its own
    /// buttons asked; closing is the system's close, tray and all.
    Window(title_bar::WindowAction),
}

/// The looks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Theme {
    Dark,
    Light,
    /// Pure black, the default.
    #[default]
    Onyx,
}

impl Theme {
    pub fn look(self) -> &'static rune_components::Look {
        match self {
            Self::Dark => &rune_components::Look::DARK,
            Self::Light => &rune_components::Look::LIGHT,
            Self::Onyx => &rune_components::Look::ONYX,
        }
    }

    /// The name it is kept by.
    pub fn key(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Onyx => "onyx",
        }
    }
}

/// The quick switcher: what is typed, and which result is chosen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Switcher {
    pub query: String,
    pub selected: usize,
}

/// A place the quick switcher goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Destination {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub kind: ChannelKind,
}

/// The audio devices and the microphone test, as the settings show them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Audio {
    pub devices: Option<Result<rune_media::Devices, String>>,
    pub input: Option<String>,
    /// The output chosen; `None` is the system's.
    pub output: Option<String>,
    pub testing: bool,
    /// The capture worker is opening or reconfiguring the selected input.
    pub starting: bool,
    /// Recent processed levels, oldest first, in meter positions from 0 to 1.
    pub levels: Vec<f32>,
    /// The same blocks before processing, for a local comparison.
    pub raw_levels: Vec<f32>,
    /// Where speech starts now, as a meter position, once the microphone was
    /// heard: with automatic sensitivity it follows the room.
    pub threshold: Option<f32>,
    /// Push-to-talk lets the microphone through now. Nothing is sent yet:
    /// this gates the call's speaking mark, and a transport would read it.
    pub transmitting: bool,
    /// Push-to-talk while another window has focus, as the desktop allows it.
    pub global_keys: GlobalKeys,
    /// While testing the microphone, one hears it back on the output.
    pub loopback: bool,
    /// The output is opening for the loopback.
    pub output_starting: bool,
    /// With the PC's sound cancelled and the microphone open: whether the
    /// canceller hears the output's loopback (`false`: only what RUNE plays).
    pub system_echo: Option<bool>,
}

/// Whether the device lists are in use: a call, a meter test, the voice page
/// or the quick voice panel. While they are, the app lists them again often.
pub fn devices_in_use(ui: &Ui, store: &Store) -> bool {
    ui.audio.testing
        || ui.settings == Some(settings::SettingsPage::Voice)
        || ui.profiles.devices.is_some()
        || store.call().is_some_and(|call| call.phase.is_connected())
}

/// Push-to-talk outside the window, through the desktop's global shortcuts.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum GlobalKeys {
    /// Push-to-talk is off or has no key.
    #[default]
    Off,
    /// Asked; the desktop may be showing its dialog.
    Waiting,
    /// Bound, with how the desktop describes its trigger.
    Bound(String),
    /// Only inside the window, and why.
    Unavailable(String),
}

/// A brief note in the corner.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub title: String,
    pub detail: Option<String>,
    pub error: bool,
    pub until: Instant,
}

/// The interface's own state.
#[derive(Debug, Default)]
pub struct Ui {
    uploads: uploads::Uploads,
    /// Only validated, bounded thumbnails supplied by the host's asset service.
    pub images: HashMap<rune_model::RemoteImage, limn::RasterImage>,
    pub image_failures: HashSet<rune_model::RemoteImage>,
    /// First-frame digests of the animations the host is presenting as
    /// surfaces now; see [`playable_images`].
    pub animated: HashSet<String>,
    pub attachment_viewer: Option<AttachmentViewer>,
    pub document_viewer: Option<DocumentViewer>,
    /// The voice message playing or paused; one at a time.
    pub voice_player: Option<VoicePlayer>,
    pub(crate) voice_memory: voice_messages::VoiceMemory,
    pub invitation: Option<InvitationDialog>,
    pub(crate) invitation_input: Option<invitations::InvitationInput>,
    pub(crate) invitation_join: invitations::InvitationJoinState,
    pub(crate) invitation_cards: invitation_cards::InvitationCards,
    pub theme: Theme,
    pub reduced_motion: bool,
    pub show_metrics: bool,
    /// `None` is the home of direct conversations.
    pub server: Option<ServerId>,
    /// The conversation last open in each server, and at home.
    pub channels: HashMap<Option<ServerId>, ChannelId>,
    pub collapsed: HashSet<ChannelId>,
    pub members_open: bool,
    /// The member list the provider was last asked to keep current: its
    /// server, the conversation it is seen from, and the rows in view.
    pub member_watch: Option<(ServerId, ChannelId, u32, u32)>,
    pub reply_to: Option<MessageId>,
    pub editing: Option<MessageId>,
    pub emoji_open: bool,
    /// Menus, pickers, the editor's text and the marks of what is done with messages.
    pub messages: Messages,
    /// The message the reaction picker is open on.
    pub reacting: Option<MessageId>,
    pub confirm_delete: Option<MessageId>,
    /// The member whose card is open from the member list.
    pub profile: Option<UserId>,
    /// The account's profile popout, with its status menu, is open.
    pub status_menu: bool,
    /// The popouts, the status timers and the profile editor.
    pub profiles: profile::Profiles,
    pub settings: Option<SettingsPage>,
    /// What the open settings are doing: the search, the section being read.
    pub settings_state: SettingsState,
    /// How the account likes RUNE.
    pub prefs: Prefs,
    /// The settings of a server, over the whole window, while open.
    pub server_settings: Option<server_settings::ServerSettings>,
    /// The settings of a channel or category, over the whole window, while open.
    pub channel_settings: Option<channel_settings::ChannelSettings>,
    /// Whether the channel settings show every overwrite: the choice stays.
    pub advanced_permissions: bool,
    pub switcher: Option<Switcher>,
    /// Local search of the current conversation's loaded messages.
    pub search: Option<MessageSearch>,
    local_history_available: bool,
    context_navigation: Option<context::Navigation>,
    inbox: Option<inbox::Inbox>,
    unread_anchor: Option<(ChannelId, MessageId)>,
    /// The conversations behind and ahead, for Alt+Left and Alt+Right.
    history: navigation::History,
    /// The list of keyboard shortcuts is open.
    pub shortcuts: bool,
    /// What is written in each conversation's composer.
    pub drafts: HashMap<ChannelId, String>,
    /// The chosen @mention completion while some are offered.
    pub completion: usize,
    pub completions_closed: bool,
    /// The first message not read when a conversation was opened.
    pub new_marker: HashMap<ChannelId, MessageId>,
    pub muted: bool,
    pub deafened: bool,
    /// The microphone as it was before deafening, to put back after.
    muted_before_deafen: bool,
    /// Draft revisions not yet accepted by the host's persistence queue.
    drafts_dirty: HashMap<ChannelId, u64>,
    draft_revision: u64,
    draft_revisions: HashMap<ChannelId, u64>,
    pub audio: Audio,
    /// The server's menu and its dialogs, and how each server marks what is new.
    pub server_menu: ServerMenu,
    /// The channel list's menus, drags and settings, and the header's tools.
    pub channel_list: ChannelList,
    pub notices: Vec<Notice>,
    /// The call's stage, menus and measures.
    pub voice: voice::Voice,
    /// A person's menu opened outside a voice channel (see `person_menu`).
    pub person_menu: Option<person_menu::PersonMenu>,
    /// An invitation to a server sent from a person's menu, until it is.
    pub person_invite: Option<person_menu::PersonInvite>,
    /// The account's screen and camera, previewed by the host.
    pub video: Video,
    /// The friends page of home.
    pub friends: relationships::Friends,
    /// Direct conversations being opened and the group dialog.
    pub conversations: conversations::Conversations,
    typing_sent: HashMap<ChannelId, Instant>,
    /// Requests the interface made, to know which answer is its own.
    requests_made: u64,
    /// The provider's own login page was opened once on its own at start;
    /// after a cancel or a sign-out the person asks for it again.
    sign_in_opened: bool,
    /// The remembered account the sign-in page asks to confirm forgetting.
    forgetting: Option<UserId>,
    // Attachments and platform (07/08).
    /// The «Guardar como» running, if any.
    pub save_as: Option<SaveAs>,
    /// The list of a channel's threads, their dialogs and menu, and how
    /// each forum is shown.
    pub threads: threads::Threads,
    // Accounts and profiles.
    /// The providers the host can start, and the one in use. Kept across
    /// accounts and providers: the host fills it once.
    pub providers: Providers,
    // Verification of writes.
    /// The verification an action waits on, while its question shows.
    pub challenge: Option<challenges::Waiting>,
    // The account's synced settings (P1).
    /// The rail's folders open, their menu and dialog, and notes being written.
    pub account: account::AccountUi,
    // Commands and interactions (P3).
    /// Apps' menus open, the modal being filled and the command being run.
    pub apps: interactions::Apps,
    // Message actions and moderation (P4).
    /// Forwarding, polls, who reacted, links and the reply's mention.
    pub message_actions: MessageActions,
    // Desktop, language and local voice (P6).
    /// What the host found of the desktop, and the panels being resized.
    pub desktop: desktop::Desktop,
    // Call sounds (audio).
    /// The sounds folder being typed, what the host found in it, and a
    /// sound to try.
    pub call_sounds: call_sounds::CallSounds,
    // Private calls.
    /// The calls coming in the system was told about.
    pub private_calls: private_calls::PrivateCalls,
    // The screen until the session is loaded (pulido-ui-real).
    /// Since when [`splash`] shows, while it does.
    pub splash_since: Option<Instant>,
    // limn adoption (2026-10-18).
    /// The color picker in use, by its id, and where its thumbs stand.
    color_picker: Option<(String, limn::Hsv)>,
    // The opening screen's questions (2026-10-18).
    /// What [`splash`] asks before a way out, while it does.
    pub splash_asking: Option<splash::Asking>,
    // Emoji and states (2026-10-18).
    /// The card of the emoji clicked in a message or a reaction.
    pub(crate) emoji_card: Option<emoji_card::Open>,
    // Voice protocol (voz-protocolo).
    /// The server the provider was last told the column shows.
    pub(crate) server_watch: Option<ServerId>,
    // Reading settings and pictures on disk (imagenes-y-lectura).
    /// What the account's pictures on disk take, as the host measured it;
    /// `None` while they are kept in memory only.
    pub image_cache: Option<reading::CacheUsage>,
    // Rules and onboarding (2026-10-18).
    /// What a server asks of the account, while its dialog is open.
    pub(crate) join: Option<onboarding::JoinDialog>,
    // Channels and voice administration (admin-canales-y-voz).
    /// Someone being dragged to another voice channel.
    pub(crate) voice_drag: Option<voice_drag::Drag>,
    // Own themes, main color and transparency (temas-propios).
    /// The themes made on this device, the main color, the window's
    /// transparency and the theme being edited.
    pub themes: themes::Themes,
    // Scheduled events (eventos-programados).
    /// The events' dialog, and which servers' lists were asked for.
    pub(crate) events: events::Events,
    // RUNE's own updates (actualizaciones).
    /// The version installed, the last check and the update offered.
    pub updates: updates::Updates,
}

/// What the host knows that the view needs and the state does not hold.
pub struct ViewContext {
    pub window: limn::Size,
    pub now: Instant,
    /// The wall clock, for dates.
    pub wall: Timestamp,
    /// The open conversation is scrolled to its end.
    pub at_end: bool,
    /// An older page of the open conversation is on its way.
    pub loading_older: bool,
    /// The provider's latest page is replacing a partial or cached window.
    pub loading_latest: bool,
    /// Measurements to show, one per line.
    pub metrics: Vec<String>,
}

/// The ID of a conversation's timeline list.
pub fn timeline_id(channel: ChannelId) -> String {
    format!("timeline:{}", channel.0)
}

/// The ID of a conversation's composer.
pub fn editor_id(channel: ChannelId) -> String {
    format!("composer:{}", channel.0)
}

/// How long a note stays in the corner.
const NOTICE: Duration = Duration::from_secs(4);
/// A conversation not open keeps this many messages in memory.
pub const KEPT_WHEN_CLOSED: usize = 150;

impl Ui {
    pub fn image(&self, source: &Option<rune_model::RemoteImage>) -> Option<limn::RasterImage> {
        self.picture(source.as_ref())
    }

    /// The pixels of `source`, once they arrived.
    pub fn picture(&self, source: Option<&rune_model::RemoteImage>) -> Option<limn::RasterImage> {
        source.and_then(|source| self.images.get(source)).cloned()
    }

    /// Drafts still waiting to be accepted: `None` removes a stored draft.
    /// Reading them does not acknowledge a write or clone their text.
    pub fn dirty_drafts(&self) -> impl Iterator<Item = (ChannelId, u64, Option<&str>)> {
        self.drafts_dirty.iter().map(move |(&channel, &revision)| {
            (
                channel,
                revision,
                self.drafts
                    .get(&channel)
                    .filter(|draft| !draft.is_empty())
                    .map(String::as_str),
            )
        })
    }

    /// A queued revision cannot acknowledge text edited after it was read.
    pub fn acknowledge_draft(&mut self, channel: ChannelId, revision: u64) {
        if self.drafts_dirty.get(&channel) == Some(&revision) {
            self.drafts_dirty.remove(&channel);
        }
    }

    /// Forgetting local data also cancels drafts not yet queued by the host.
    pub fn clear_drafts(&mut self) {
        self.drafts.clear();
        self.drafts_dirty.clear();
        // Nor are their files kept again (envios-y-subidas).
        crate::uploads::forget_draft_files(self);
    }

    fn set_draft(&mut self, channel: ChannelId, draft: Option<String>) {
        match draft {
            Some(draft) => self.drafts.insert(channel, draft),
            None => self.drafts.remove(&channel),
        };
        self.draft_revision += 1;
        self.draft_revisions.insert(channel, self.draft_revision);
        self.drafts_dirty.insert(channel, self.draft_revision);
    }

    /// The interface as it first opens: the member list shown.
    pub fn new() -> Self {
        Self {
            members_open: true,
            ..Self::default()
        }
    }

    /// The open conversation.
    pub fn channel(&self) -> Option<ChannelId> {
        self.channels.get(&self.server).copied()
    }

    /// A request id for a command whose answer the interface waits for, such
    /// as making a role. Apart from the ones the core numbers from 1.
    pub fn fresh_request(&mut self) -> rune_provider::RequestId {
        self.requests_made += 1;
        (1 << 48) + self.requests_made
    }

    /// Show a note for a few seconds.
    pub fn notify(
        &mut self,
        title: impl Into<String>,
        detail: Option<String>,
        error: bool,
        now: Instant,
    ) {
        self.notices.push(Notice {
            title: title.into(),
            detail,
            error,
            until: now + NOTICE,
        });
        if self.notices.len() > 3 {
            self.notices.remove(0);
        }
    }

    /// Expire notices and invitation waits. True when the view changed.
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.notices.len();
        self.notices.retain(|notice| notice.until > now);
        let invitations_changed = invitations::expire(self, now);
        let recording_ended = settings::expire_recording(self, now);
        self.notices.len() != before || invitations_changed || recording_ended
    }

    /// When a notice or invitation wait changes, so an idle host wakes then.
    pub fn next_expiry(&self, now: Instant) -> Option<Duration> {
        self.notices
            .iter()
            .map(|notice| notice.until.saturating_duration_since(now))
            .chain(invitations::next_expiry(self, now))
            .chain(settings::recording_expiry(self, now))
            .chain(splash::expiry(self, now))
            .min()
    }

    /// The settings worth keeping between runs.
    pub fn settings_to_keep(&self) -> Vec<(&'static str, Option<String>)> {
        vec![
            // Desktop (P6): or the desktop's, as it changes.
            (
                "theme",
                Some(
                    if self.prefs.system_theme {
                        "system"
                    } else {
                        self.theme.key()
                    }
                    .into(),
                ),
            ),
            ("reduced_motion", Some(self.reduced_motion.to_string())),
            ("show_metrics", Some(self.show_metrics.to_string())),
            ("members_open", Some(self.members_open.to_string())),
            ("server", self.server.map(|server| server.0.to_string())),
            (
                "channel",
                self.channel().map(|channel| channel.0.to_string()),
            ),
            ("microphone", self.audio.input.clone()),
            ("camera", self.video.device.clone()),
            (server_menu::NOTIFY_SETTING, server_menu::kept(self)),
            (gif_favorites::KEY, gif_favorites::kept(self)),
            (message_actions::TRUSTED_KEY, message_actions::kept(self)),
        ]
        .into_iter()
        .chain(channel_list::kept(self))
        .chain(profile::kept(self))
        // Rail and notifications.
        .chain([(
            account::OPEN_FOLDERS_SETTING,
            account::kept_open_folders(self),
        )])
        .chain(self.prefs.kept())
        // Own themes, main color and transparency (temas-propios).
        .chain(themes::kept(self))
        .collect()
    }

    /// Take back what [`Self::settings_to_keep`] kept.
    pub fn restore(&mut self, setting: impl Fn(&str) -> Option<String>) {
        self.theme = match setting("theme").as_deref() {
            Some("light") => Theme::Light,
            Some("dark") => Theme::Dark,
            _ => Theme::Onyx,
        };
        self.prefs.restore(&setting);
        self.reduced_motion = setting("reduced_motion").as_deref() == Some("true");
        self.show_metrics = setting("show_metrics").as_deref() == Some("true");
        self.members_open = setting("members_open").as_deref() != Some("false");
        self.audio.input = setting("microphone");
        self.audio.output = setting(profile::SPEAKER);
        self.video.device = setting("camera");
        gif_favorites::restore(self, setting(gif_favorites::KEY));
        message_actions::restore(self, setting(message_actions::TRUSTED_KEY));
        // Own themes, main color and transparency (temas-propios).
        themes::restore(self, &setting);
        self.restore_account(&setting);
    }
}

/// Open `channel` of `server` (`None` for home): remember it, mark where
/// the unread messages start, and ask for what it lacks.
pub fn open(
    ui: &mut Ui,
    store: &mut Store,
    server: Option<ServerId>,
    channel: ChannelId,
) -> Vec<Intent> {
    open_at(ui, store, server, channel, None)
}

pub(crate) fn open_at(
    ui: &mut Ui,
    store: &mut Store,
    server: Option<ServerId>,
    channel: ChannelId,
    target: Option<MessageId>,
) -> Vec<Intent> {
    ui.inbox = None;
    let previous = ui.channel();
    let mut intents = if previous != Some(channel) {
        let mut intents = search::dismiss(ui);
        intents.extend(context::cancel(ui));
        intents
    } else {
        Vec::new()
    };
    navigation::record(ui, store, previous, channel);
    ui.server = server;
    ui.channels.insert(server, channel);
    ui.voice.stage = None;
    ui.voice.chat = None;
    ui.friends.open = false;
    if previous != Some(channel) {
        ui.attachment_viewer = None;
        // The «más» of the header left behind, opened from the keyboard too.
        ui.channel_list.more = None;
        ui.reply_to = None;
        ui.editing = None;
        ui.confirm_delete = None;
        ui.emoji_open = false;
        ui.reacting = None;
        ui.profile = None;
        ui.profiles.author = None;
        ui.completion = 0;
        ui.messages.leave();
    }
    reads::opened(ui, store, channel);
    store.trim(Some(channel), KEPT_WHEN_CLOSED);
    intents.push(Intent::CancelHistoryExcept(channel));
    intents.extend(documents::refresh(ui, store));
    intents.extend(voice_messages::refresh(ui, store));
    // A forum has no history of its own: its posts are channels, the ones
    // the session was not told of asked for once.
    if store
        .channel(channel)
        .is_some_and(|found| found.kind == ChannelKind::Forum)
    {
        intents.extend(threads::opened(ui, store, channel));
        return intents;
    }
    if let Some(anchor) = target {
        ui.unread_anchor = None;
        let source = reads::source(ui, store, channel);
        intents.extend(open_message(ui, store, channel, anchor, source));
    } else if !store
        .timeline(channel)
        .is_some_and(|timeline| timeline.latest)
        && store
            .abilities(channel)
            .is_none_or(|abilities| abilities.read_history.allowed())
    {
        intents.push(Intent::LoadLatest(channel));
    } else {
        intents.extend(reads::reveal(ui, store));
    }
    intents.push(Intent::Focus(editor_id(channel)));
    intents
}

/// Open the server `server` at the conversation last open there, or its first.
pub fn open_server(ui: &mut Ui, store: &mut Store, server: Option<ServerId>) -> Vec<Intent> {
    ui.attachment_viewer = None;
    let remembered = ui
        .channels
        .get(&server)
        .copied()
        .filter(|channel| store.channel(*channel).is_some_and(Store::visible));
    let channel = remembered.or_else(|| match server {
        Some(server) => store.first_text_channel(server),
        None => store.directs().first().map(|channel| channel.id),
    });
    match channel {
        Some(channel) => open(ui, store, server, channel),
        None => {
            ui.server = server;
            let mut intents = documents::refresh(ui, store);
            intents.extend(voice_messages::refresh(ui, store));
            intents
        }
    }
}

/// Once the provider is ready: open what was open last time, or the first
/// server's first channel.
pub fn on_ready(ui: &mut Ui, store: &mut Store) -> Vec<Intent> {
    let server = ui
        .server
        .filter(|server| store.server(*server).is_some())
        .or(store.servers().first().map(|server| server.id));
    open_server(ui, store, server)
}

/// Answer what an event of the provider changed.
pub fn on_applied(ui: &mut Ui, store: &mut Store, applied: &Applied, now: Instant) -> Vec<Intent> {
    let mut upload_intents = uploads::on_applied(ui, store, applied);
    // Before sign-out returns early: another account never hears this one's audio.
    upload_intents.extend(voice_messages::refresh(ui, store));
    upload_intents.extend(save_as::refresh(ui, store));
    challenges::on_applied(ui, applied);
    let auth = auth::on_applied(ui, store, applied);
    // After auth, which may give the window a fresh state.
    splash::on_applied(ui, store, now);
    if let Some(intents) = auth {
        upload_intents.extend(intents);
        return upload_intents;
    }
    let mut intents = upload_intents;
    intents.extend(context::on_applied(ui, store, applied));
    intents.extend(server_settings::on_applied(ui, store, applied, now));
    intents.extend(profile::on_applied(ui, store, applied));
    intents.extend(channel_settings::on_applied(ui, store, applied));
    intents.extend(answer(ui, store, applied, now));
    intents.extend(person_menu::on_applied(ui, store, applied, now));
    relationships::on_applied(ui, store, applied);
    intents.extend(conversations::on_applied(ui, store, applied));
    intents.extend(threads::on_applied(ui, store, applied));
    intents.extend(invitations::on_applied(ui, store, applied, now));
    // Rules and onboarding (2026-10-18).
    intents.extend(onboarding::on_applied(ui, store, applied));
    // Scheduled events (eventos-programados).
    intents.extend(events::on_applied(ui, store, applied));
    attachments::refresh(ui, store);
    intents.extend(documents::refresh(ui, store));
    intents.extend(channel_list::on_applied(ui, store, applied, now));
    voice::on_applied(ui, store, applied, now);
    // Calls and the voice panel: a direct call asked about goes on, and
    // the bar's panels go with the call.
    intents.extend(private_calls::on_applied(ui, store, applied, now));
    intents.extend(quick_voice::on_applied(ui, store));
    intents.extend(search::on_applied(ui, store, applied));
    intents.extend(reads::on_applied(ui, store, applied));
    intents.extend(account::on_applied(ui, store, applied));
    intents.extend(message_actions::on_applied(ui, store, applied, now));
    // The provider's GIFs and suggestions (P5).
    intents.extend(gifs::on_applied(ui, store, applied));
    // Reads (P2): what was read while a confirmation was in flight.
    if let rune_state::Applied::ReadState(channel) = applied {
        intents.extend(store.read_follow_up(*channel).map(Intent::Command));
    }
    intents.extend(commands::on_applied(ui, store, applied));
    inbox::refresh(ui, store);
    // A modal that closed lets go of its files (comandos-de-apps).
    intents.extend(interactions::on_applied(ui, store));
    intents
}

/// A secondary click on `target` at `at`: the menu of what is there, if it
/// has one. `None` lets the tree have the click.
pub fn secondary_click(
    ui: &mut Ui,
    store: &Store,
    target: &str,
    at: limn::Point,
) -> Option<Vec<Intent>> {
    let opened = streams::secondary_click(ui, store, target, at)
        .or_else(|| voice::secondary_click(ui, store, target, at))
        .or_else(|| person_menu::secondary_click(ui, store, target, at))
        .or_else(|| account::secondary_click(ui, store, target, at))
        .or_else(|| threads::secondary_click(ui, store, target, at))
        .or_else(|| channel_list::secondary_click(ui, store, target, at))
        .or_else(|| conversations::secondary_click(ui, store, target, at))
        .or_else(|| emoji_card::secondary_click(ui, target, at));
    // Another menu opens: limn never sees the click, so a message's goes here.
    if opened.is_some() {
        ui.messages.context = None;
        ui.messages.more = None;
    }
    opened
}

/// A double click of the primary button on `target`, what is under the
/// pointer (transmisiones): the stream there full screen, or the stage back
/// from it. `None` lets the tree have the press.
pub fn double_click(ui: &mut Ui, store: &Store, target: Option<&str>) -> Option<Vec<Intent>> {
    streams::double_click(ui, store, target)
}

fn answer(ui: &mut Ui, store: &mut Store, applied: &Applied, now: Instant) -> Vec<Intent> {
    match applied {
        Applied::Ready => {
            // A new session watches nothing yet: ask again on the next layout.
            ui.member_watch = None;
            ui.profiles.people_watch = None;
            ui.server_watch = None;
            voice::keeping_stage(ui, |ui| inbox::on_ready(ui, store))
        }
        // A message that did not get there says so where it is shown.
        Applied::Failed {
            failure: rune_provider::Failure::Send { .. } | rune_provider::Failure::SendFiles { .. },
            ..
        } => Vec::new(),
        // The pins popout owns its request's error; old or dismissed loads
        // must not produce a toast for a newer view.
        Applied::Failed {
            failure: rune_provider::Failure::Pins { .. },
            ..
        } => Vec::new(),
        Applied::Failed {
            failure: rune_provider::Failure::Context { .. },
            ..
        } => Vec::new(),
        // An empty timeline shows its history failure in place of the
        // skeleton; with messages already loaded, keep the usual notice.
        Applied::Failed {
            failure: rune_provider::Failure::History { channel, .. },
            ..
        } if store
            .timeline(*channel)
            .is_none_or(|timeline| timeline.is_empty()) =>
        {
            Vec::new()
        }
        // The search dialog says it beside its results (P2).
        Applied::Failed {
            failure: rune_provider::Failure::Search { .. },
            ..
        } => Vec::new(),
        // The card says what did not load: "No disponible".
        Applied::Failed {
            failure: rune_provider::Failure::ProfileLoad { .. },
            ..
        } => Vec::new(),
        // A note that could not be read says so in its card.
        Applied::Failed { failure, .. } if account::note_failed(ui, store, failure) => Vec::new(),
        // The profile editor says it beside its form.
        Applied::Failed {
            failure: rune_provider::Failure::Profile,
            ..
        } if ui.profiles.editor.is_some() => Vec::new(),
        Applied::Failed { failure, .. } if invitations::shows_failure(ui, failure) => Vec::new(),
        // Apps' failures show where they happened (P3).
        Applied::Failed {
            failure:
                rune_provider::Failure::Interaction { .. } | rune_provider::Failure::AppCommands { .. },
            ..
        } => Vec::new(),
        Applied::Failed { failure, .. } if relationships::shows_failure(ui, failure) => Vec::new(),
        Applied::Failed { failure, .. } if message_actions::shows_failure(ui, failure) => {
            Vec::new()
        }
        // A refused direct call says it in its own dialog.
        Applied::Failed { failure, .. } if private_calls::shows_failure(store, failure) => {
            Vec::new()
        }
        // Scheduled events say it in their dialog; the entry's count read in
        // the background is not worth a notice.
        Applied::Failed { failure, .. } if events::shows_failure(ui, failure) => Vec::new(),
        Applied::Failed {
            failure, reason, ..
        } => {
            ui.notify(
                tr!("No se pudo {action}", action = failure.action()),
                Some(reason.clone()),
                true,
                now,
            );
            Vec::new()
        }
        Applied::CallEnded {
            reason: Some(reason),
        } => {
            ui.call_sounds.dropped = true;
            ui.notify(t("Se cortó la llamada"), Some(reason.clone()), true, now);
            Vec::new()
        }
        Applied::Connection => {
            if let ConnectionState::Disconnected { reason } = &store.connection {
                ui.notify(t("Sin conexión"), Some(t(reason).to_owned()), true, now);
            }
            Vec::new()
        }
        // What was open is gone: open something else of the same place.
        Applied::ServerRemoved { server, reason } => {
            ui.channels.remove(&Some(*server));
            if let Some(reason) = reason {
                let name = t("Ya no estás en un servidor");
                ui.notify(name, Some(reason.clone()), true, now);
            }
            if ui.server == Some(*server) {
                let next = store.servers().first().map(|server| server.id);
                return open_server(ui, store, next);
            }
            Vec::new()
        }
        Applied::ChannelRemoved(channel) | Applied::Channel(channel) => {
            let gone = store
                .channel(*channel)
                .is_none_or(|found| !Store::visible(found));
            if gone && ui.channel() == Some(*channel) {
                ui.channels.remove(&ui.server);
                return voice::keeping_stage(ui, |ui| open_server(ui, store, ui.server));
            }
            Vec::new()
        }
        Applied::Created { request, created } => {
            server_menu::on_created(ui, store, *request, *created)
        }
        Applied::InviteCreated { .. } => server_menu::on_invite(ui, store, applied, now),
        _ => Vec::new(),
    }
}

/// After a layout: ask for older messages before the first one shows, and
/// mark the conversation read while its end is in view; mark the section of
/// the settings being read.
pub fn after_layout(ui: &mut Ui, store: &mut Store, tree: &UiTree, active: bool) -> Vec<Intent> {
    capture_composer_selection(ui, tree);
    messages::forget_gone(ui, store);
    messages::forget_unshown_menus(ui, tree);
    let blocked_before = context::blocks_reading(ui) || reads::obscured(ui);
    let mut intents = settings::after_layout(ui, tree);
    intents.extend(watch_members(ui, tree));
    intents.extend(voice::watch_server(ui));
    // Scheduled events (eventos-programados): the list the entry counts.
    intents.extend(events::after_layout(ui, store));
    profile::hold_member_card(ui, tree);
    profile::hold_author_card(ui, tree);
    profile::hold_scroll_close(ui, tree);
    identity::hold_hovered(ui, store, tree);
    intents.extend(emoji_card::hold(ui, store, tree));
    intents.extend(identity::after_layout(ui, store));
    intents.extend(reads::reveal(ui, store));
    intents.extend(context::after_layout(ui, store, tree));
    intents.extend(navigation::after_layout(ui, store, tree));
    voice_messages::after_layout(ui, store, tree);
    video::hold(ui, tree);
    if active && !blocked_before && !reads::obscured(ui) {
        intents.extend(read_and_load(ui, store, tree));
    }
    intents
}

/// Ask the provider to keep the member list current around the rows in view,
/// once they change; to stop once it is not shown.
fn watch_members(ui: &mut Ui, tree: &UiTree) -> Vec<Intent> {
    let want = tree
        .visible_rows("members")
        .zip(ui.server)
        .zip(ui.channel())
        .map(|((rows, server), channel)| {
            let last = rows.end.saturating_sub(1).max(rows.start);
            (server, channel, rows.start as u32, last as u32)
        });
    if want == ui.member_watch {
        return Vec::new();
    }
    ui.member_watch = want;
    vec![Intent::Command(match want {
        Some((server, channel, first, last)) => Command::WatchMemberList {
            server,
            channel,
            first,
            last,
        },
        None => Command::UnwatchMemberList,
    })]
}

/// Capture the composer's current selection after layout and before each input
/// event. The latter keeps multiple events in one host frame in order.
pub fn capture_composer_selection(ui: &mut Ui, tree: &UiTree) {
    composer_selection::capture(ui, tree);
}

fn read_and_load(ui: &mut Ui, store: &mut Store, tree: &UiTree) -> Vec<Intent> {
    if context::blocks_reading(ui) {
        return Vec::new();
    }
    let Some(channel) = ui.channel() else {
        return Vec::new();
    };
    let Some(timeline) = store.timeline(channel) else {
        return Vec::new();
    };
    if !store
        .abilities(channel)
        .is_some_and(|a| a.read_history.allowed())
    {
        return Vec::new();
    }
    let list = timeline_id(channel);
    let mut intents = Vec::new();
    if let (Some(rows), Some(oldest)) = (tree.visible_rows(&list), timeline.oldest()) {
        if rows.start <= 5 && !timeline.reached_start && timeline.latest {
            intents.push(Intent::LoadOlder(channel, oldest));
        }
    }
    // Marked unread by hand, it stays so while it is open.
    if timeline.latest
        && tree.is_scroll_at_end(&list, 24.0)
        && !messages::held_unread(store, channel)
    {
        if let Some(command) = store.mark_read_visible_command(channel) {
            intents.push(Intent::Command(command));
        }
    }
    intents
}

/// Apply one action of the window.
pub fn update(ui: &mut Ui, store: &mut Store, action: &UiAction, now: Instant) -> Vec<Intent> {
    // A color picker's field and hue bar write the color as its hex field.
    if let Some((typed, shown)) = color_picker::translate(ui, action) {
        let mut intents = answer_action(ui, store, &typed, now);
        // The avatar's color is not a draft: it is asked for on letting go.
        intents.extend(settings::picker_released(ui, store, action));
        intents.push(shown);
        return intents;
    }
    if let UiAction::InputChanged { id, value } = action {
        color_picker::note(ui, id, value);
    }
    let intents = answer_action(ui, store, action, now);
    for intent in &intents {
        if let Intent::SetInput { id, value } = intent {
            color_picker::note(ui, id, value);
        }
    }
    intents
}

fn answer_action(ui: &mut Ui, store: &mut Store, action: &UiAction, now: Instant) -> Vec<Intent> {
    // The window's own buttons (barra-e-icono), over everything.
    if let Some(intents) = title_bar::update(ui, action) {
        return intents;
    }
    // Choosing anything closes a narrow header's «más», its own items too
    // (2026-10-18); its button toggles it in `channel_list`.
    if matches!(action, UiAction::Clicked { id }
        if id != rune_components::channel_menu::ids::MORE)
    {
        ui.channel_list.more = None;
    }
    if let Some(intents) = providers::update(ui, action) {
        return intents;
    }
    // RUNE's own updates (actualizaciones).
    if let Some(intents) = updates::update(ui, action) {
        return intents;
    }
    if let Some(intents) = splash::update(store, ui, action) {
        return intents;
    }
    if let Some(intents) = auth::update(ui, store, action) {
        return intents;
    }
    if let Some(intents) = challenges::update(ui, action) {
        return intents;
    }
    // A badge's or connected account's link on the profile shown (P5).
    if let Some(intents) = identity::update(ui, store, action) {
        return intents;
    }
    if let Some(intents) = invitations::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = documents::update(ui, store, action) {
        return intents;
    }
    if let Some(intents) = voice_messages::update(ui, store, action) {
        return intents;
    }
    if let Some(intents) = attachments::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = save_as::update(ui, action) {
        return intents;
    }
    if let Some(intents) = inbox::update(ui, store, action) {
        return intents;
    }
    // Desktop (P6): the panels' handles, hidden channels, call codes.
    if let Some(intents) = desktop::update(ui, store, action) {
        return intents;
    }
    // Own themes, main color and transparency (temas-propios).
    if let Some(intents) = themes::update(ui, action, now) {
        return intents;
    }
    // Channels and voice administration: people dragged between voice channels.
    if let Some(intents) = voice_drag::update(ui, store, action) {
        return intents;
    }
    match action {
        UiAction::Clicked { id } if id == navigation::SHORTCUTS_CLOSE => {
            ui.shortcuts = false;
            return Vec::new();
        }
        // What Ctrl+/ does, for a host that has no keyboard to press it.
        UiAction::Clicked { id } if id == navigation::SHORTCUTS_OPEN && !ui.shortcuts => {
            return toggle_shortcuts(ui);
        }
        // The header's arrows (atajos-y-navegacion).
        UiAction::Clicked { id } if id == rune_components::chrome::BACK => {
            return travel(ui, store, true);
        }
        UiAction::Clicked { id } if id == rune_components::chrome::FORWARD => {
            return travel(ui, store, false);
        }
        _ => {}
    }
    // Choosing anything closes the server's menu, its own items too.
    if matches!(action, UiAction::Clicked { id } if id != server_menu::TOGGLE) {
        ui.server_menu.open = false;
        conversations::close_menu(ui);
    }
    messages::before(ui, action);
    // Choosing anything closes a person's menu, before the handler of the choice.
    person_menu::update(ui, action);
    if let Some(intents) = person_menu::action(ui, store, action, now) {
        return intents;
    }
    // Rules and onboarding (2026-10-18).
    if let Some(intents) = onboarding::update(ui, store, action) {
        return intents;
    }
    // Scheduled events (eventos-programados).
    if let Some(intents) = events::update(ui, store, action) {
        return intents;
    }
    // Commands and interactions (P3).
    if let Some(intents) = interactions::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = commands::update(ui, store, action, now) {
        return intents;
    }
    // Message actions (P4): before the links the window opens by itself.
    if let Some(intents) = message_actions::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = threads::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = stage::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = search::update(ui, store, action) {
        return intents;
    }
    // Private calls: before the voice's, which shares its stage.
    if let Some(intents) = private_calls::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = quick_voice::update(ui, store, action, now) {
        return intents;
    }
    // Streams (transmisiones): before the voice's, whose menus it shares.
    if let Some(intents) = streams::update(ui, store, action) {
        return intents;
    }
    if let Some(intents) = voice::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = profile::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = settings::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = server_settings::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = channel_settings::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = account::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = channel_list::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = relationships::update(ui, store, action, now) {
        return intents;
    }
    if let Some(intents) = conversations::update(ui, store, action, now) {
        return intents;
    }
    // Before the messages', whose actions it shares the prefix of.
    if let Some(intents) = emoji_card::update(ui, action) {
        return intents;
    }
    if let Some(intents) = messages::update(ui, store, action, now) {
        return intents;
    }
    match action {
        UiAction::Clicked { id } => clicked(ui, store, id, now),
        UiAction::InputChanged { id, value } => {
            if server_menu::input_changed(ui, id, value) {
                return server_menu::lookup(ui, store, id).into_iter().collect();
            }
            if let Some(switcher) = ui
                .switcher
                .as_mut()
                .filter(|_| id == settings::SWITCHER_QUERY)
            {
                switcher.query = value.clone();
                switcher.selected = 0;
                return Vec::new();
            }
            Vec::new()
        }
        UiAction::Submitted { id, value } => {
            if id == settings::SWITCHER_QUERY {
                return settings::go_to_selected(ui, store);
            }
            server_menu::submitted(ui, store, id, value).unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

fn clicked(ui: &mut Ui, store: &mut Store, id: &str, now: Instant) -> Vec<Intent> {
    let number = |prefix: &str| {
        id.strip_prefix(prefix)
            .and_then(|rest| rest.parse::<u64>().ok())
    };
    if server_menu::owns(id) {
        return server_menu::clicked(ui, store, id, now);
    }
    let channel = ui.channel();
    if let Some(url) = id.strip_prefix("markdown_link:") {
        // The action is the prefix, the block, the run and the address.
        return url
            .splitn(3, ':')
            .nth(2)
            .map(|url| vec![Intent::OpenUrl(url.to_owned())])
            .unwrap_or_default();
    }
    if let Some(server) = number("server:") {
        return open_server(ui, store, Some(ServerId(server)));
    }
    if let Some(target) = number("channel:")
        .or_else(|| number("thread:"))
        .or_else(|| number("post:"))
    {
        let target = ChannelId(target);
        let server = store.channel(target).and_then(|channel| channel.server);
        return open(ui, store, server, target);
    }
    if let Some(user) = number("member:") {
        let user = UserId(user);
        ui.profile = if ui.profile == Some(user) {
            None
        } else {
            Some(user)
        };
        return match ui.profile {
            Some(user) => profile::request_profile(ui, store, user, ui.server),
            None => Vec::new(),
        };
    }
    if id.starts_with("settings:") || id.starts_with("switcher:") {
        return settings::clicked(ui, store, id, now);
    }
    match id {
        "home" => open_server(ui, store, None),
        "members:toggle" => {
            if !has_member_list(store) {
                return Vec::new();
            }
            ui.members_open = !ui.members_open;
            vec![Intent::SaveSetting {
                key: "members_open".into(),
                value: Some(ui.members_open.to_string()),
            }]
        }
        "me:mute" => {
            if !has_voice_controls(store) {
                return Vec::new();
            }
            if ui.muted {
                if let Some(reason) = store.mic_held() {
                    ui.notify(t("Tu micrófono sigue cerrado"), Some(reason), true, now);
                    return Vec::new();
                }
            }
            if ui.deafened {
                // Unmuting while deafened turns the sound back on too.
                ui.deafened = false;
                ui.muted = false;
            } else {
                ui.muted = !ui.muted;
            }
            set_self_voice(ui, store)
        }
        "me:deafen" => {
            if !has_voice_controls(store) {
                return Vec::new();
            }
            if ui.deafened {
                ui.deafened = false;
                ui.muted = ui.muted_before_deafen || store.mic_held().is_some();
            } else {
                ui.muted_before_deafen = ui.muted;
                ui.deafened = true;
                ui.muted = true;
            }
            set_self_voice(ui, store)
        }
        "me:settings" => settings::open_settings(ui, store, SettingsPage::default()),
        "jump:latest" => channel
            .map(|channel| {
                ui.unread_anchor = None;
                let mut intents = context::cancel(ui);
                if store
                    .timeline(channel)
                    .is_some_and(|timeline| !timeline.latest)
                    && store
                        .abilities(channel)
                        .is_none_or(|abilities| abilities.read_history.allowed())
                {
                    intents.push(Intent::LoadLatest(channel));
                }
                intents.push(Intent::ScrollToEnd(timeline_id(channel)));
                intents
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn has_member_list(store: &Store) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.member_list)
}

fn has_voice_controls(store: &Store) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.voice != rune_provider::VoiceSupport::None)
}

/// Show the account's microphone and sound in its call, and tell the provider.
fn set_self_voice(ui: &mut Ui, store: &mut Store) -> Vec<Intent> {
    store.set_self_voice(ui.muted, ui.deafened);
    match store.call() {
        Some(_) => vec![Intent::Command(Command::SetVoiceSelf {
            muted: ui.muted,
            deafened: ui.deafened,
        })],
        None => Vec::new(),
    }
}

/// The window lost the focus: a key being recorded is not waited for from
/// another window, and Shift no longer counts as held
/// (limn-teclado-y-superficies).
pub fn window_unfocused(ui: &mut Ui) -> Vec<Intent> {
    ui.messages.shift_held = false;
    record_key(ui, None)
}

/// Keys the window answers before limn's tree sees them. Returns what to do
/// when the key was taken, `None` to let the tree have it.
pub fn intercept(
    ui: &mut Ui,
    store: &mut Store,
    event: &UiEvent,
    focused: Option<&str>,
) -> Option<Vec<Intent>> {
    messages::note_press(ui, event);
    messages::note_modifiers(ui, event);
    profile::close_card_on_scroll(ui, event);
    let UiEvent::Key { key, ctrl, .. } = event else {
        return None;
    };
    let (key, ctrl) = (*key, *ctrl);
    if let Some(intents) = auth::key(store, key, ctrl) {
        return Some(intents);
    }
    // The list of shortcuts is a limn modal: it takes Escape and Tab itself.
    if ui.shortcuts {
        return None;
    }
    if ui.invitation_input.is_some() {
        return invitations::intercept(ui, event);
    }
    if let Some(intents) = invitations::intercept(ui, event) {
        return Some(intents);
    }
    // Rules and onboarding (2026-10-18).
    if let Some(intents) = onboarding::intercept(ui, event) {
        return Some(intents);
    }
    // Scheduled events (eventos-programados).
    if let Some(intents) = events::intercept(ui, event) {
        return Some(intents);
    }
    if let Some(intents) = documents::intercept(ui, event) {
        return Some(intents);
    }
    if let Some(intents) = video::intercept(ui, event) {
        return Some(intents);
    }
    if let Some(intents) = voice_messages::intercept(ui, event) {
        return Some(intents);
    }
    if let Some(intents) = attachments::intercept(ui, store, event) {
        return Some(intents);
    }
    if keybinds::pressed(&ui.prefs, event) == Some(keybinds::Action::Switcher) {
        let mut intents = search::dismiss(ui);
        intents.extend(context::cancel(ui));
        ui.inbox = None;
        if ui.switcher.is_some() {
            ui.switcher = None;
        } else {
            intents.extend(settings::open_switcher(ui));
        }
        return Some(intents);
    }
    // A message's menu opened where a secondary click was leaves the focus
    // where it stood, in the composer most often: the arrows and Escape are
    // the menu's, and limn takes them.
    if ui.messages.context.is_some()
        && matches!(
            key,
            Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End | Key::Escape
        )
    {
        return None;
    }
    if keybinds::pressed(&ui.prefs, event) == Some(keybinds::Action::Search) {
        return Some(search::open(ui, store));
    }
    if let Some(intents) = search::keys(ui, store, key, ctrl, focused) {
        return Some(intents);
    }
    if let Some(intents) = inbox::key(ui, key) {
        return Some(intents);
    }
    if ui.switcher.is_some() && focused == Some(settings::SWITCHER_QUERY) {
        let count = settings::destinations(ui, store).len();
        let switcher = ui.switcher.as_mut()?;
        match key {
            Key::ArrowDown if count > 0 => switcher.selected = (switcher.selected + 1) % count,
            Key::ArrowUp if count > 0 => {
                switcher.selected = (switcher.selected + count - 1) % count
            }
            Key::Escape => ui.switcher = None,
            _ => return None,
        }
        return Some(Vec::new());
    }
    if let Some(intents) = profile::intercept(ui, store, event) {
        return Some(intents);
    }
    if let Some(intents) = settings::intercept(ui, event) {
        return Some(intents);
    }
    if let Some(intents) = channel_settings::intercept(ui, store, event) {
        return Some(intents);
    }
    if let Some(intents) = server_settings::intercept(ui, store, event) {
        return Some(intents);
    }
    if let Some(intents) = commands::keys(ui, store, key, ctrl, focused) {
        return Some(intents);
    }
    if let Some(intents) = message_actions::keys(ui, store, key, Instant::now()) {
        return Some(intents);
    }
    if let Some(intents) = messages::keys(ui, store, key, ctrl, focused) {
        return Some(intents);
    }
    // Editable shortcuts (atajos-y-navegacion): the composer's, as the table
    // binds them.
    if let Some(intents) = keybinds::composer(ui, store, event, focused) {
        return Some(intents);
    }
    context::key(ui, key, focused)
}

/// Move to the conversation above or below the open one in the column.
pub fn navigate(ui: &mut Ui, store: &mut Store, down: bool) -> Vec<Intent> {
    if invitations::visible(ui) || onboarding::visible(ui) || events::visible(ui) {
        return Vec::new();
    }
    let order: Vec<ChannelId> = match ui.server {
        Some(server) => store
            .channel_tree(server)
            .into_iter()
            .filter(|(category, _)| {
                category.is_none_or(|category| !ui.collapsed.contains(&category.id))
            })
            .flat_map(|(_, channels)| channels)
            .filter(|channel| channel.kind.has_messages())
            .map(|channel| channel.id)
            .collect(),
        None => store
            .directs()
            .into_iter()
            .map(|channel| channel.id)
            .collect(),
    };
    let Some(current) = ui
        .channel()
        .and_then(|channel| order.iter().position(|id| *id == channel))
    else {
        return Vec::new();
    };
    let next = if down {
        (current + 1).min(order.len() - 1)
    } else {
        current.saturating_sub(1)
    };
    if next == current {
        return Vec::new();
    }
    open(ui, store, ui.server, order[next])
}

#[cfg(test)]
mod tests;
