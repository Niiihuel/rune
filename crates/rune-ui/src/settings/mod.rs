//! The account's settings and the quick switcher.
//!
//! The settings are a panel over the dimmed window: a sidebar of pages, a
//! search box that filters them, and the open page, whose sections the
//! sidebar links to and follows as it scrolls. Every setting shown does
//! something in RUNE and is kept between runs (`Ui::settings_to_keep`);
//! what RUNE cannot do yet says so, disabled, or is left out. Actions start
//! with `us:`; the older `settings:` ones still work.

mod pages;

#[cfg(test)]
pub(crate) use pages::jump_margin;
pub use pages::{modal, switcher};

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use limn::{Key, UiAction, UiEvent, UiTree};
use rune_components::message::{MessageStyle, GROUP_GAP, TEXT_SIZE};
use rune_components::server_settings::parse_hex;
use rune_components::user_settings::mark_at;
use rune_i18n::t;
use rune_model::{ChannelId, ChannelKind, ServerId};
use rune_provider::{Command, ProfileEdit};
use rune_state::Store;

use crate::{open, open_server, Destination, Intent, Theme, Ui};

pub const SWITCHER_QUERY: &str = "switcher:query";
/// The fields of the settings, whose text limn keeps by id.
pub(crate) const SEARCH: &str = "us:search";
pub(crate) const NAME: &str = "us:name";
pub(crate) const ACCENT: &str = "us:accent";
pub(crate) const ACCENT_HEX: &str = "us:accent:hex";

/// The marks of the sliders, and the one each starts at.
pub(crate) const TEXT_SIZES: [u8; 7] = [12, 14, 15, 16, 18, 20, 24];
pub(crate) const GROUP_GAPS: [u8; 5] = [0, 4, 8, 16, 24];
pub(crate) const ZOOMS: [u16; 11] = [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200];

/// A page of the settings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SettingsPage {
    #[default]
    Account,
    Privacy,
    Notifications,
    Voice,
    Appearance,
    Accessibility,
    Keybinds,
    Desktop,
    Language,
    Connection,
    // RUNE's own updates (actualizaciones).
    Updates,
    Developer,
    Activity,
}

impl SettingsPage {
    /// In the order of the sidebar's groups.
    pub const ALL: [Self; 13] = [
        Self::Account,
        Self::Privacy,
        Self::Connection,
        Self::Appearance,
        Self::Voice,
        Self::Notifications,
        Self::Accessibility,
        Self::Keybinds,
        Self::Language,
        Self::Desktop,
        Self::Updates,
        Self::Developer,
        Self::Activity,
    ];

    /// The page's name in its action, `us:page={key}`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Privacy => "privacy",
            Self::Notifications => "notifications",
            Self::Voice => "voice",
            Self::Appearance => "appearance",
            Self::Accessibility => "accessibility",
            Self::Keybinds => "keybinds",
            Self::Desktop => "desktop",
            Self::Language => "language",
            Self::Connection => "connection",
            Self::Developer => "developer",
            Self::Updates => "updates",
            Self::Activity => "activity",
        }
    }

    /// The page a key names; the names of the first settings still work, and
    /// the page that was «Sistema» is the desktop's.
    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "audio" => Self::Voice,
            "shortcuts" => Self::Keybinds,
            "system" => Self::Desktop,
            _ => *Self::ALL.iter().find(|page| page.key() == key)?,
        })
    }

    /// What the sidebar and the header call it.
    pub fn title(self) -> &'static str {
        match self {
            Self::Account => t("Cuenta"),
            Self::Privacy => t("Datos y privacidad"),
            Self::Notifications => t("Notificaciones"),
            Self::Voice => t("Voz y vídeo"),
            Self::Appearance => t("Apariencia"),
            Self::Accessibility => t("Accesibilidad"),
            Self::Keybinds => t("Atajos de teclado"),
            Self::Desktop => t("Escritorio"),
            Self::Language => t("Idioma"),
            Self::Connection => t("Conexiones"),
            Self::Developer => t("Avanzado"),
            Self::Updates => t("Actualizaciones"),
            Self::Activity => t("Privacidad de la actividad"),
        }
    }

    /// The scroll area of its column.
    pub(crate) fn scroll_id(self) -> String {
        format!("us:content:{}", self.key())
    }
}

/// The anchor a section's link in the sidebar jumps to.
pub(crate) fn anchor(key: &str) -> String {
    format!("us:section:{key}")
}

/// How the account likes RUNE, kept between runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    /// Messages on one line each, without avatars.
    pub compact: bool,
    /// What messages say, in pixels.
    pub text_size: u8,
    /// Room above a group of messages, in pixels.
    pub group_gap: u8,
    /// The whole interface's size, in percent.
    pub zoom: u16,
    pub twelve_hour: bool,
    /// Menus offer to copy ids.
    pub developer: bool,
    pub reactions: bool,
    pub image_previews: bool,
    /// Names take their top role's color.
    pub role_colors: bool,
    /// The rail marks servers with something unread.
    pub unread_badges: bool,
    /// The rail counts mentions.
    pub mention_badges: bool,
    /// How the microphone is processed: a preset, or [`Self::processing`].
    pub input_profile: InputProfile,
    /// The processing of [`InputProfile::Custom`]; off until chosen.
    pub processing: Processing,
    /// With echo cancellation, also cancel what other applications play on
    /// the output (a game, music), with the output's monitor as the
    /// canceller's reference only. On by default (2026-10-19).
    pub system_echo: bool,
    /// The microphone's level before anything else, as a linear gain from 0
    /// to [`rune_media::MAX_INPUT_VOLUME`].
    pub input_volume: f32,
    /// The level that counts as voice follows the room's.
    pub auto_sensitivity: bool,
    /// Otherwise this one, as a position of the level meter, 0 to 1.
    pub sensitivity: f32,
    /// The microphone goes through only while a key is held.
    pub push_to_talk: bool,
    /// That key, as the host names it (`Ctrl+Shift+T`); none until recorded.
    pub push_to_talk_key: Option<String>,
    /// How long it keeps going through after the key is let go, in ms.
    pub push_to_talk_release: u16,
    /// What RUNE plays, as a linear gain from 0 to [`rune_media::MAX_VOLUME`].
    pub output_volume: f32,
    /// How loud each person plays in calls, from 0 to
    /// [`rune_media::MAX_VOLUME`], and whether they are silenced, by user ID.
    /// Only on this device: it is never sent.
    pub voice_volumes: BTreeMap<u64, (f32, bool)>,
    /// Whose camera is hidden for the account, by user ID: their tile shows
    /// the avatar. Only on this device: it is never sent.
    pub hidden_video: BTreeSet<u64>,
    /// The desktop portal's token for the screen last chosen, to share it
    /// again without its picker. It names a choice there, not a credential.
    pub screen_restore: Option<String>,
    // Attachments and platform (07/08).
    /// Mentions and direct messages show a notification of the system.
    pub desktop_notifications: bool,
    /// The notification says what the message says; otherwise «Nuevo mensaje».
    pub notification_content: bool,
    /// Mentions and direct messages play the message sound.
    pub message_sounds: bool,
    // Desktop, language and local voice (P6).
    /// Closing the window leaves RUNE in the system tray, when there is one.
    pub close_to_tray: bool,
    /// The light or dark theme follows the desktop's.
    pub system_theme: bool,
    /// The family of the interface's text; `None` is RUNE's own.
    pub ui_font: Option<String>,
    /// The widths of the column of channels and of the member list.
    pub sidebar_width: f32,
    pub members_width: f32,
    /// The width of a call's chat beside its stage.
    pub call_chat_width: f32,
    /// Server channels shown first, and direct conversations pinned on top,
    /// the newest first; only on this device.
    pub favorites: Vec<ChannelId>,
    pub pinned: Vec<ChannelId>,
    /// Channels the account cannot see are listed, dimmed.
    pub show_hidden: bool,
    /// Global shortcuts: the microphone and the sound on and off, and a key
    /// held to keep the microphone closed.
    pub mute_key: Option<String>,
    pub deafen_key: Option<String>,
    pub push_to_mute_key: Option<String>,
    /// Other programs on this computer may say what the person is doing
    /// (the local Rich Presence socket). Off until turned on.
    pub rich_presence: bool,
    /// A known game running is said as the account's activity. Off until
    /// turned on.
    pub game_activity: bool,
    // Interface languages (i18n).
    /// The interface's language; `None` is the system's.
    pub language: Option<rune_i18n::Language>,
    // Call sounds (audio).
    /// The events whose sound is off, in order; the message's has its own
    /// switch, [`Self::message_sounds`].
    pub sounds_off: Vec<crate::CallSound>,
    /// How loud they play, 0 to 1, under the output's volume.
    pub sound_volume: f32,
    /// A folder whose files named as the events (`join.ogg`…) play instead
    /// of RUNE's, absolute or from the home folder (`~/`).
    pub sound_folder: Option<String>,
    // Video by the GPU (video-por-hardware).
    /// H264 is encoded and decoded by the GPU when there is one (VA-API,
    /// NVENC/NVDEC); off, by OpenH264 on the CPU.
    pub video_hardware: bool,
    // Sends and uploads (envios-y-subidas).
    /// Emoticons such as `:)` go as emoji when a message is sent or edited,
    /// as in Serein; off until turned on.
    pub convert_emoticons: bool,
    // Reading and pictures on disk (imagenes-y-lectura).
    /// GIFs and other animations play by themselves; off, they stand on
    /// their first frame until the pointer is over them.
    pub animate_gifs: bool,
    /// A message that is only links to pictures or GIFs its cards show
    /// hides them; off, the links stay over their pictures.
    pub hide_media_links: bool,
    /// The wheel's and the trackpad's movement, in percent.
    pub scroll_speed: u16,
    /// The member list stays open in windows too narrow for it beside the
    /// conversation, which narrows instead.
    pub members_narrow: bool,
    /// The disk's budgets for media and for shared artwork, in MiB, and
    /// the days a picture not shown again stays.
    pub media_cache_mib: u32,
    pub artwork_cache_mib: u32,
    pub cache_days: u16,
    // Profile card and cosmetics (perfil-tarjeta-y-cosmeticos).
    /// Which of people's cosmetics this device shows; a hidden one is not
    /// even downloaded.
    pub cosmetics: Cosmetics,
    // Desktop session and games (escritorio-sesion-y-juegos).
    /// The games added, renamed or hidden on this device.
    pub registered_games: Vec<crate::games::RegisteredGame>,
    // Editable shortcuts (atajos-y-navegacion).
    /// The keys chosen in place of the defaults, voice aside; see
    /// [`crate::keybinds`].
    pub shortcuts: Vec<(crate::keybinds::Action, crate::keybinds::Chord)>,
}

/// The cosmetics a person can hide, as Serein lets them: each one is
/// drawn and fetched only while on (2026-10-18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cosmetics {
    /// Frames over faces.
    pub decorations: bool,
    /// Artwork behind names in the member list.
    pub nameplates: bool,
    /// Effects over profile cards.
    pub effects: bool,
}

impl Default for Cosmetics {
    fn default() -> Self {
        Self {
            decorations: true,
            nameplates: true,
            effects: true,
        }
    }
}

/// Discord's input profiles: two presets of the processing, and one's own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputProfile {
    /// RNNoise, echo cancellation and automatic gain: only the voice. The
    /// default, as in Discord and Serein.
    #[default]
    VoiceIsolation,
    /// Nothing: the microphone as it is.
    Studio,
    /// [`Prefs::processing`], as the person set it.
    Custom,
}

impl InputProfile {
    pub const ALL: [Self; 3] = [Self::VoiceIsolation, Self::Studio, Self::Custom];

    pub fn key(self) -> &'static str {
        match self {
            Self::VoiceIsolation => "voice-isolation",
            Self::Studio => "studio",
            Self::Custom => "custom",
        }
    }

    /// What the profile sets; `None` for one's own.
    pub fn preset(self) -> Option<Processing> {
        match self {
            Self::VoiceIsolation => Some(Processing {
                suppression: rune_media::ProcessingMode::RnNoise,
                echo_cancellation: true,
                automatic_gain: true,
            }),
            Self::Studio => Some(Processing::default()),
            Self::Custom => None,
        }
    }
}

/// What is done to the microphone before anyone hears it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Processing {
    pub suppression: rune_media::ProcessingMode,
    /// Cancel the echo of what RUNE plays; it acts only while something does.
    pub echo_cancellation: bool,
    pub automatic_gain: bool,
}

/// How a noise suppressor is kept.
fn suppression_key(mode: rune_media::ProcessingMode) -> &'static str {
    match mode {
        rune_media::ProcessingMode::Off => "off",
        rune_media::ProcessingMode::WebRtc => "webrtc",
        rune_media::ProcessingMode::RnNoise => "rnnoise",
    }
}

/// The longest release delay of push-to-talk, in ms.
pub const MAX_RELEASE: u16 = 2000;
/// A key being recorded stops waiting after this long.
pub const RECORD_TIMEOUT: Duration = Duration::from_secs(10);

impl Default for Prefs {
    fn default() -> Self {
        Self {
            compact: false,
            text_size: TEXT_SIZE as u8,
            group_gap: GROUP_GAP as u8,
            zoom: 100,
            twelve_hour: false,
            developer: false,
            reactions: true,
            image_previews: true,
            role_colors: true,
            unread_badges: true,
            mention_badges: true,
            input_profile: InputProfile::default(),
            processing: Processing::default(),
            // Measured (docs/ARCHITECTURE.md): it removes a game's and
            // music's echo and leaves the voice as it was.
            system_echo: true,
            input_volume: 1.0,
            auto_sensitivity: true,
            sensitivity: rune_media::VoiceActivity::default().on,
            push_to_talk: false,
            push_to_talk_key: None,
            // Discord's own default: enough not to clip the last consonant.
            push_to_talk_release: 20,
            output_volume: 1.0,
            voice_volumes: BTreeMap::new(),
            hidden_video: BTreeSet::new(),
            screen_restore: None,
            // Asked for: a notification of the system is never a surprise.
            desktop_notifications: false,
            notification_content: true,
            message_sounds: true,
            close_to_tray: false,
            system_theme: false,
            ui_font: None,
            sidebar_width: rune_components::sidebar::WIDTH,
            members_width: rune_components::members::WIDTH,
            call_chat_width: 400.0,
            favorites: Vec::new(),
            pinned: Vec::new(),
            show_hidden: false,
            mute_key: None,
            deafen_key: None,
            push_to_mute_key: None,
            // The owner's rule (2026-10-17): both off, each with its switch.
            rich_presence: false,
            game_activity: false,
            language: None,
            sounds_off: Vec::new(),
            sound_volume: 1.0,
            sound_folder: None,
            video_hardware: true,
            convert_emoticons: false,
            // Serein's defaults.
            animate_gifs: true,
            hide_media_links: true,
            scroll_speed: 100,
            members_narrow: false,
            media_cache_mib: 1024,
            artwork_cache_mib: 128,
            cache_days: 90,
            cosmetics: Cosmetics::default(),
            registered_games: Vec::new(),
            shortcuts: Vec::new(),
        }
    }
}

impl Prefs {
    /// How the timeline shows messages.
    pub fn message_style(&self) -> MessageStyle {
        MessageStyle {
            compact: self.compact,
            text_size: f32::from(self.text_size),
            group_gap: f32::from(self.group_gap),
            reactions: self.reactions,
            image_previews: self.image_previews,
            copy_id: self.developer,
            sample: false,
            hide_media_links: self.hide_media_links,
        }
    }

    /// The interface's scale, for the host: 1 is its own size.
    pub fn zoom_factor(&self) -> f32 {
        f32::from(self.zoom) / 100.0
    }

    /// Set the levels `voice` decides speech by: the room's, or the account's
    /// threshold with silence a little under it, so the mark does not
    /// flicker between words.
    pub fn tune(&self, voice: &mut rune_media::VoiceActivity) {
        voice.automatic = self.auto_sensitivity && !self.speech_gate();
        if self.speech_gate() {
            // RNNoise's gate decided already: what it let through is speech.
            voice.on = rune_media::VoiceActivity::GATED_ON;
            voice.off = rune_media::VoiceActivity::GATED_OFF;
        } else if !self.auto_sensitivity {
            let usual = rune_media::VoiceActivity::default();
            voice.on = self.sensitivity;
            voice.off = (voice.on - (usual.on - usual.off)).max(0.0);
        }
    }

    /// Whether RNNoise's speech gate decides what goes out: voice activity
    /// with automatic sensitivity and RNNoise, the default. Push-to-talk
    /// and a threshold chosen by hand go by level, as before.
    pub fn speech_gate(&self) -> bool {
        self.auto_sensitivity
            && !self.push_to_talk
            && self.effective_processing().suppression == rune_media::ProcessingMode::RnNoise
    }

    /// The processing in effect: the profile's, or one's own.
    pub fn effective_processing(&self) -> Processing {
        self.input_profile.preset().unwrap_or(self.processing)
    }

    /// Change the processing by hand: a preset becomes one's own, starting
    /// from what it set, as Discord does. Returns what to keep.
    fn edit_processing(&mut self, change: impl FnOnce(&mut Processing)) -> Vec<Intent> {
        self.processing = self.effective_processing();
        self.input_profile = InputProfile::Custom;
        change(&mut self.processing);
        self.kept()
            .into_iter()
            .filter(|(key, _)| {
                matches!(
                    *key,
                    "input_profile" | "noise_suppression" | "echo_cancellation" | "automatic_gain"
                )
            })
            .map(|(key, value)| Intent::SaveSetting {
                key: key.into(),
                value,
            })
            .collect()
    }

    /// The preferences as settings to keep.
    pub(crate) fn kept(&self) -> Vec<(&'static str, Option<String>)> {
        let flag = |on: bool| Some(on.to_string());
        vec![
            ("compact", flag(self.compact)),
            ("chat_text_size", Some(self.text_size.to_string())),
            ("group_gap", Some(self.group_gap.to_string())),
            ("zoom", Some(self.zoom.to_string())),
            (
                "clock",
                Some(if self.twelve_hour { "12" } else { "24" }.into()),
            ),
            ("developer", flag(self.developer)),
            ("reactions", flag(self.reactions)),
            ("image_previews", flag(self.image_previews)),
            ("role_colors", flag(self.role_colors)),
            ("unread_badges", flag(self.unread_badges)),
            ("mention_badges", flag(self.mention_badges)),
            ("input_profile", Some(self.input_profile.key().into())),
            (
                "noise_suppression",
                Some(suppression_key(self.processing.suppression).into()),
            ),
            ("echo_cancellation", flag(self.processing.echo_cancellation)),
            ("automatic_gain", flag(self.processing.automatic_gain)),
            ("system_echo", flag(self.system_echo)),
            ("input_volume", Some(self.input_volume.to_string())),
            ("auto_sensitivity", flag(self.auto_sensitivity)),
            ("sensitivity", Some(self.sensitivity.to_string())),
            ("push_to_talk", flag(self.push_to_talk)),
            ("push_to_talk_key", self.push_to_talk_key.clone()),
            (
                "push_to_talk_release",
                Some(self.push_to_talk_release.to_string()),
            ),
            ("output_volume", Some(self.output_volume.to_string())),
            ("voice_volumes", Some(self.voice_volumes_kept())),
            ("hidden_video", Some(self.hidden_video_kept())),
            ("screen_restore", self.screen_restore.clone()),
            ("desktop_notifications", flag(self.desktop_notifications)),
            ("notification_content", flag(self.notification_content)),
            ("message_sounds", flag(self.message_sounds)),
            ("video_hardware", flag(self.video_hardware)),
            ("convert_emoticons", flag(self.convert_emoticons)),
        ]
        .into_iter()
        .chain(crate::desktop::kept(self))
        .chain(crate::call_sounds::kept(self))
        .chain([crate::language::kept(self)])
        .chain(crate::reading::kept(self))
        .chain(pages::cosmetics::kept(self))
        .chain([(crate::keybinds::SETTING, crate::keybinds::kept(self))])
        .collect()
    }

    /// Take back what [`Self::kept`] kept; what is missing or wrong stays as it starts.
    pub(crate) fn restore(&mut self, setting: &impl Fn(&str) -> Option<String>) {
        let usual = Self::default();
        let flag = |key: &str, default: bool| match setting(key).as_deref() {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        };
        let number = |key: &str| setting(key).and_then(|value| value.parse::<f32>().ok());
        self.compact = flag("compact", usual.compact);
        self.text_size = number("chat_text_size")
            .map(|value| value as u8)
            .filter(|value| TEXT_SIZES.contains(value))
            .unwrap_or(usual.text_size);
        self.group_gap = number("group_gap")
            .map(|value| value as u8)
            .filter(|value| GROUP_GAPS.contains(value))
            .unwrap_or(usual.group_gap);
        self.zoom = number("zoom")
            .map(|value| value as u16)
            .filter(|value| (50..=200).contains(value))
            .unwrap_or(usual.zoom);
        self.twelve_hour = setting("clock").as_deref() == Some("12");
        self.developer = flag("developer", usual.developer);
        self.reactions = flag("reactions", usual.reactions);
        self.image_previews = flag("image_previews", usual.image_previews);
        self.role_colors = flag("role_colors", usual.role_colors);
        self.unread_badges = flag("unread_badges", usual.unread_badges);
        self.mention_badges = flag("mention_badges", usual.mention_badges);
        // Settings from before profiles were one's own processing: keep it.
        let earlier = setting("input_profile").is_none()
            && ["noise_suppression", "echo_cancellation", "automatic_gain"]
                .iter()
                .any(|key| setting(key).is_some());
        self.input_profile = if earlier {
            InputProfile::Custom
        } else {
            InputProfile::ALL
                .into_iter()
                .find(|profile| setting("input_profile").as_deref() == Some(profile.key()))
                .unwrap_or(usual.input_profile)
        };
        self.processing = Processing {
            suppression: match setting("noise_suppression").as_deref() {
                // Before profiles it was a switch for WebRTC's suppressor.
                Some("webrtc" | "true") => rune_media::ProcessingMode::WebRtc,
                Some("rnnoise") => rune_media::ProcessingMode::RnNoise,
                _ => usual.processing.suppression,
            },
            echo_cancellation: flag("echo_cancellation", usual.processing.echo_cancellation),
            automatic_gain: flag("automatic_gain", usual.processing.automatic_gain),
        };
        self.system_echo = flag("system_echo", usual.system_echo);
        self.input_volume = number("input_volume")
            .filter(|value| (0.0..=rune_media::MAX_INPUT_VOLUME).contains(value))
            .unwrap_or(usual.input_volume);
        self.auto_sensitivity = flag("auto_sensitivity", usual.auto_sensitivity);
        self.sensitivity = number("sensitivity")
            .filter(|value| (0.0..=1.0).contains(value))
            .unwrap_or(usual.sensitivity);
        self.push_to_talk = flag("push_to_talk", usual.push_to_talk);
        self.push_to_talk_key = setting("push_to_talk_key")
            .filter(|key| valid_key(key))
            .or(usual.push_to_talk_key);
        self.push_to_talk_release = setting("push_to_talk_release")
            .and_then(|value| value.parse().ok())
            .filter(|value| *value <= MAX_RELEASE)
            .unwrap_or(usual.push_to_talk_release);
        self.output_volume = number("output_volume")
            .filter(|value| (0.0..=rune_media::MAX_VOLUME).contains(value))
            .unwrap_or(usual.output_volume);
        self.voice_volumes = setting("voice_volumes")
            .map(|kept| restore_voice_volumes(&kept))
            .unwrap_or_default();
        self.hidden_video = setting("hidden_video")
            .map(|kept| kept.split(';').filter_map(|id| id.parse().ok()).collect())
            .unwrap_or_default();
        self.screen_restore = setting("screen_restore").filter(|token| restore_token(token));
        self.desktop_notifications = flag("desktop_notifications", usual.desktop_notifications);
        self.notification_content = flag("notification_content", usual.notification_content);
        self.message_sounds = flag("message_sounds", usual.message_sounds);
        self.video_hardware = flag("video_hardware", usual.video_hardware);
        self.convert_emoticons = flag("convert_emoticons", usual.convert_emoticons);
        crate::desktop::restore(self, setting);
        crate::call_sounds::restore(self, setting);
        crate::language::restore(self, setting);
        crate::reading::restore(self, setting);
        pages::cosmetics::restore(self, setting);
        self.shortcuts = crate::keybinds::restore(setting(crate::keybinds::SETTING));
    }

    /// The volume and silence chosen for `user`: 100 % and heard unless chosen.
    pub fn voice_volume(&self, user: u64) -> (f32, bool) {
        self.voice_volumes
            .get(&user)
            .copied()
            .unwrap_or((1.0, false))
    }

    /// Choose `user`'s volume and silence; the usual ones are forgotten.
    /// Returns what to keep.
    pub(crate) fn set_voice_volume(&mut self, user: u64, volume: f32, muted: bool) -> Intent {
        let full = self.voice_volumes.len() >= rune_media::mixer::MAX_CHOICES;
        if volume == 1.0 && !muted {
            self.voice_volumes.remove(&user);
        } else if !full || self.voice_volumes.contains_key(&user) {
            self.voice_volumes.insert(user, (volume, muted));
        }
        save("voice_volumes", self.voice_volumes_kept())
    }

    /// Whether `user`'s camera is hidden for the account.
    pub fn video_hidden(&self, user: u64) -> bool {
        self.hidden_video.contains(&user)
    }

    /// Hide or show `user`'s camera for the account. Returns what to keep.
    pub(crate) fn set_video_hidden(&mut self, user: u64, hidden: bool) -> Intent {
        if hidden {
            self.hidden_video.insert(user);
        } else {
            self.hidden_video.remove(&user);
        }
        save("hidden_video", self.hidden_video_kept())
    }

    /// The IDs of the cameras hidden, `;` between them.
    fn hidden_video_kept(&self) -> String {
        self.hidden_video
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(";")
    }

    /// `id:volume:muted` for each choice, `;` between them.
    fn voice_volumes_kept(&self) -> String {
        self.voice_volumes
            .iter()
            .map(|(user, (volume, muted))| format!("{user}:{volume}:{}", u8::from(*muted)))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// The release delay of push-to-talk.
    pub fn release_delay(&self) -> Duration {
        Duration::from_millis(self.push_to_talk_release.into())
    }
}

/// A portal restore token as RUNE keeps one: short and printable.
pub(crate) fn restore_token(token: &str) -> bool {
    !token.is_empty() && token.len() <= 256 && token.bytes().all(|byte| byte.is_ascii_graphic())
}

/// The choices [`Prefs::voice_volumes_kept`] wrote; what does not read is left out.
fn restore_voice_volumes(kept: &str) -> BTreeMap<u64, (f32, bool)> {
    kept.split(';')
        .filter_map(|entry| {
            let mut parts = entry.split(':');
            let user = parts.next()?.parse().ok()?;
            let volume: f32 = parts.next()?.parse().ok()?;
            let muted = match parts.next()? {
                "0" => false,
                "1" => true,
                _ => return None,
            };
            (parts.next().is_none() && (0.0..=rune_media::MAX_VOLUME).contains(&volume))
                .then_some((user, (volume, muted)))
        })
        .take(rune_media::mixer::MAX_CHOICES)
        .collect()
}

/// A key name as the host writes one: short, printable, without spaces.
pub(crate) fn valid_key(key: &str) -> bool {
    (1..=48).contains(&key.len()) && key.bytes().all(|byte| byte.is_ascii_graphic())
}

/// What the open settings are doing, apart from the page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettingsState {
    /// What the search box holds.
    pub query: String,
    /// The section being read, as the sidebar marks it.
    pub section: Option<&'static str>,
    /// A section jumped to, marked until the page is scrolled by hand; and
    /// where the page stood once it got there.
    jumped: Option<(&'static str, Option<f32>)>,
    /// The display name being written, while it is.
    pub name: Option<String>,
    /// The list that is open: `mic`, `speaker`, `language` or `role-colors`.
    pub select: Option<&'static str>,
    /// The field for a color of one's own, while it is open.
    pub accent_hex: Option<String>,
    /// The zoom's mark while its slider is dragged: applied when let go,
    /// since the slider would move under the pointer as everything grows.
    pub zoom_drag: Option<usize>,
    /// Since when the push-to-talk key is being recorded, while it is.
    pub recording: Option<Instant>,
    /// Which shortcut that recording is for (P6; any of the table since
    /// atajos-y-navegacion).
    pub recording_for: crate::keybinds::Action,
    /// Why the key last recorded or reset for an action was refused.
    pub key_notice: Option<(crate::keybinds::Action, String)>,
}

/// Whether the host should hand its next key press to [`record_key`].
pub fn recording_key(ui: &Ui) -> bool {
    ui.settings.is_some() && ui.settings_state.recording.is_some()
}

/// End recording the shortcut being recorded: with the key pressed, or
/// `None` when cancelled (Esc, the window lost focus). Nothing else changes
/// the key.
pub fn record_key(ui: &mut Ui, key: Option<String>) -> Vec<Intent> {
    if ui.settings_state.recording.take().is_none() {
        return Vec::new();
    }
    match key.filter(|key| valid_key(key)) {
        Some(key) => crate::keybinds::recorded(ui, key),
        None => Vec::new(),
    }
}

/// Stop recording once it waited too long. True when it stopped.
pub(crate) fn expire_recording(ui: &mut Ui, now: Instant) -> bool {
    let due = ui
        .settings_state
        .recording
        .is_some_and(|since| now.saturating_duration_since(since) >= RECORD_TIMEOUT);
    if due {
        ui.settings_state.recording = None;
    }
    due
}

/// How long until a recording times out, so an idle host wakes then.
pub(crate) fn recording_expiry(ui: &Ui, now: Instant) -> Option<Duration> {
    ui.settings_state
        .recording
        .map(|since| (since + RECORD_TIMEOUT).saturating_duration_since(now))
}

/// Every place the switcher can go that matches its query: servers, then
/// conversations, at most twelve.
pub fn destinations(ui: &Ui, store: &Store) -> Vec<Destination> {
    let query = ui
        .switcher
        .as_ref()
        .map(|switcher| switcher.query.trim().to_lowercase())
        .unwrap_or_default();
    let matches = |name: &str| query.is_empty() || name.to_lowercase().contains(&query);
    let mut found = Vec::new();
    for server in store.servers() {
        if matches(server.shown_name()) {
            found.push(Destination {
                id: format!("server:{}", server.id),
                title: server.shown_name().into(),
                detail: t("Servidor").into(),
                kind: ChannelKind::Category,
            });
        }
        for (_, channels) in store.channel_tree(server.id) {
            for channel in channels
                .into_iter()
                .filter(|channel| channel.kind.has_messages() && matches(&channel.name))
            {
                found.push(Destination {
                    id: format!("channel:{}", channel.id),
                    title: format!("#{}", channel.name),
                    detail: server.shown_name().into(),
                    kind: channel.kind,
                });
            }
        }
    }
    for channel in store.directs() {
        let title = store.channel_title(channel.id);
        if matches(&title) {
            found.push(Destination {
                id: format!("channel:{}", channel.id),
                title,
                detail: t("Mensaje directo").into(),
                kind: channel.kind,
            });
        }
    }
    found.truncate(12);
    found
}

/// Go where the switcher's chosen result leads.
pub fn go_to_selected(ui: &mut Ui, store: &mut Store) -> Vec<Intent> {
    let found = destinations(ui, store);
    let selected = ui.switcher.as_ref().map_or(0, |switcher| switcher.selected);
    ui.switcher = None;
    found
        .get(selected.min(found.len().saturating_sub(1)))
        .map(|destination| go(ui, store, &destination.id))
        .unwrap_or_default()
}

fn go(ui: &mut Ui, store: &mut Store, id: &str) -> Vec<Intent> {
    if let Some(server) = id
        .strip_prefix("server:")
        .and_then(|server| server.parse().ok())
    {
        return open_server(ui, store, Some(ServerId(server)));
    }
    if let Some(channel) = id
        .strip_prefix("channel:")
        .and_then(|channel| channel.parse().ok())
        .map(rune_model::ChannelId)
    {
        let server = store.channel(channel).and_then(|channel| channel.server);
        return open(ui, store, server, channel);
    }
    Vec::new()
}

/// Open the settings at `page`, as they first show.
pub fn open_settings(ui: &mut Ui, store: &Store, page: SettingsPage) -> Vec<Intent> {
    let mut intents = crate::search::dismiss(ui);
    intents.extend(crate::context::cancel(ui));
    // The quick voice panel gives way to the page it is half of.
    ui.profiles.devices = None;
    ui.settings = Some(SettingsPage::default());
    ui.settings_state = SettingsState::default();
    intents.extend(go_to_page(ui, store, page));
    intents
}

/// Close the settings, and the microphone or camera being tested.
pub fn close_settings(ui: &mut Ui) -> Vec<Intent> {
    ui.settings = None;
    ui.settings_state = SettingsState::default();
    stop_meter(ui)
}

pub(crate) fn stop_meter(ui: &mut Ui) -> Vec<Intent> {
    ui.audio.loopback = false;
    ui.audio.output_starting = false;
    ui.video.testing = false;
    ui.audio.starting = false;
    ui.audio.levels.clear();
    ui.audio.raw_levels.clear();
    if ui.audio.testing {
        ui.audio.testing = false;
        return vec![Intent::StopMeter];
    }
    Vec::new()
}

/// Show `page`, from its top or, while searching, from what matches there.
fn go_to_page(ui: &mut Ui, store: &Store, page: SettingsPage) -> Vec<Intent> {
    let mut intents = Vec::new();
    if page != SettingsPage::Voice {
        intents.extend(stop_meter(ui));
    } else {
        // Listed again each time: the app drops a repeat while one is pending.
        intents.push(Intent::RefreshDevices);
    }
    ui.settings = Some(page);
    let state = &mut ui.settings_state;
    state.name = None;
    state.select = None;
    state.accent_hex = None;
    state.recording = None;
    state.jumped = None;
    state.section = None;
    let first = pages::first_match(ui, store, page);
    intents.push(match first {
        Some(key) => jump(ui, page, key),
        None => Intent::Reveal {
            id: pages::top(page),
            margin: 0.0,
        },
    });
    intents
}

/// Scroll the open page to the section `key`, and mark it.
fn jump(ui: &mut Ui, page: SettingsPage, key: &'static str) -> Intent {
    ui.settings_state.section = Some(key);
    ui.settings_state.jumped = Some((key, None));
    Intent::Reveal {
        id: anchor(key),
        margin: pages::jump_margin(page),
    }
}

/// Keep `key` as `value` for the next run.
fn save(key: &str, value: impl ToString) -> Intent {
    Intent::SaveSetting {
        key: key.into(),
        value: Some(value.to_string()),
    }
}

/// Apply an action of the settings; `None` for the rest of the window.
pub fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    match action {
        UiAction::Clicked { id } if id.starts_with("us:") => Some(clicked(ui, store, id, now)),
        UiAction::InputChanged { id, value } if id.starts_with("us:") => {
            let state = &mut ui.settings_state;
            match id.as_str() {
                SEARCH => {
                    state.query = value.clone();
                    state.section = None;
                    state.jumped = None;
                }
                NAME => state.name = Some(value.clone()),
                ACCENT_HEX => state.accent_hex = Some(value.clone()),
                crate::call_sounds::FOLDER => ui.call_sounds.typed = Some(value.clone()),
                _ => {}
            }
            Some(Vec::new())
        }
        UiAction::Submitted { id, .. } if id.starts_with("us:") => Some(match id.as_str() {
            SEARCH => match pages::first_page(ui, store) {
                Some(page) => go_to_page(ui, store, page),
                None => Vec::new(),
            },
            NAME => save_name(ui, store),
            crate::call_sounds::FOLDER => crate::call_sounds::use_folder(ui),
            ACCENT_HEX => {
                let typed = ui.settings_state.accent_hex.take().unwrap_or_default();
                match parse_hex(&typed) {
                    Some(color) => edit_profile(
                        store,
                        ProfileEdit {
                            accent: Some(color),
                            ..ProfileEdit::default()
                        },
                    ),
                    None => {
                        ui.settings_state.accent_hex = Some(typed);
                        Vec::new()
                    }
                }
            }
            _ => Vec::new(),
        }),
        UiAction::SliderChanged {
            id,
            fraction,
            committed,
        } if id.starts_with("us:") => Some(slid(ui, id, *fraction, *committed)),
        _ => None,
    }
}

/// A slider of the settings moved to `fraction`.
fn slid(ui: &mut Ui, id: &str, fraction: f32, committed: bool) -> Vec<Intent> {
    if let Some(intents) = crate::reading::slid(ui, id, fraction, committed) {
        return intents;
    }
    let prefs = &mut ui.prefs;
    let (key, value) = match id {
        "us:text-size" => {
            prefs.text_size = TEXT_SIZES[mark_at(fraction, TEXT_SIZES.len())];
            ("chat_text_size", prefs.text_size.to_string())
        }
        "us:group-gap" => {
            prefs.group_gap = GROUP_GAPS[mark_at(fraction, GROUP_GAPS.len())];
            ("group_gap", prefs.group_gap.to_string())
        }
        "us:zoom" => {
            let mark = mark_at(fraction, ZOOMS.len());
            if !committed {
                ui.settings_state.zoom_drag = Some(mark);
                return Vec::new();
            }
            ui.settings_state.zoom_drag = None;
            prefs.zoom = ZOOMS[mark];
            ("zoom", prefs.zoom.to_string())
        }
        "us:sensitivity" => {
            prefs.sensitivity = fraction.clamp(0.0, 1.0);
            ("sensitivity", prefs.sensitivity.to_string())
        }
        "us:ptt-release" => {
            // Steps of 10 ms, as the label reads them.
            let steps = f32::from(MAX_RELEASE / 10);
            prefs.push_to_talk_release = (fraction.clamp(0.0, 1.0) * steps).round() as u16 * 10;
            (
                "push_to_talk_release",
                prefs.push_to_talk_release.to_string(),
            )
        }
        "us:input-volume" => {
            let percent = (fraction.clamp(0.0, 1.0) * rune_media::MAX_INPUT_VOLUME * 100.0).round();
            prefs.input_volume = percent / 100.0;
            ("input_volume", prefs.input_volume.to_string())
        }
        "us:sound-volume" => {
            prefs.sound_volume = (fraction.clamp(0.0, 1.0) * 100.0).round() / 100.0;
            ("sound_volume", prefs.sound_volume.to_string())
        }
        "us:output-volume" => {
            // Whole percents, so 100 % is easy to land on.
            let percent = (fraction.clamp(0.0, 1.0) * rune_media::MAX_VOLUME * 100.0).round();
            prefs.output_volume = percent / 100.0;
            ("output_volume", prefs.output_volume.to_string())
        }
        _ => return Vec::new(),
    };
    if committed {
        vec![save(key, value)]
    } else {
        Vec::new()
    }
}

/// Send what the display name field holds, if it changed.
fn save_name(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let Some(typed) = ui.settings_state.name.take() else {
        return Vec::new();
    };
    let Some(me) = store.me_user() else {
        return Vec::new();
    };
    let typed = typed.trim();
    let name = (!typed.is_empty() && typed != me.name).then(|| typed.to_owned());
    if name == me.display_name {
        return Vec::new();
    }
    edit_profile(
        store,
        ProfileEdit {
            display_name: Some(name),
            ..ProfileEdit::default()
        },
    )
}

/// Why the profile cannot be edited, when it cannot.
pub(crate) fn profile_locked(store: &Store) -> Option<&'static str> {
    if store.me_user().is_none() {
        return Some(t("La cuenta todavía no se conectó."));
    }
    let editable = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.edit_profile);
    (!editable).then_some(t("Este proveedor no permite editar el perfil."))
}

/// Ask the provider to change the profile; nothing shows until it answers.
fn edit_profile(store: &Store, edit: ProfileEdit) -> Vec<Intent> {
    if profile_locked(store).is_some() {
        return Vec::new();
    }
    vec![Intent::Command(Command::EditProfile(edit))]
}

/// Reset both controlled state and the input retained by limn for a new query.
pub(crate) fn open_switcher(ui: &mut Ui) -> Vec<Intent> {
    ui.switcher = Some(Default::default());
    vec![
        Intent::SetInput {
            id: SWITCHER_QUERY.into(),
            value: String::new(),
        },
        Intent::Focus(SWITCHER_QUERY.into()),
    ]
}

/// Actions of the settings, `us:…` or the older `settings:…`, and of the switcher.
pub fn clicked(ui: &mut Ui, store: &mut Store, id: &str, now: Instant) -> Vec<Intent> {
    if id == "us:sign-out" {
        return if can_sign_out(store) {
            vec![Intent::Command(Command::SignOut)]
        } else {
            Vec::new()
        };
    }
    if id == "us:add-account" || id.starts_with("us:account=") {
        let allowed = can_sign_out(store)
            && store
                .provider
                .as_ref()
                .is_some_and(|info| info.capabilities.saved_accounts);
        let target = crate::auth::remembered(store, id, "us:account=");
        if !allowed || (target.is_none() && id != "us:add-account") {
            return Vec::new();
        }
        // The sign-out that comes first must not open the login window too.
        ui.sign_in_opened = true;
        return vec![Intent::Command(Command::UseAccount(target))];
    }
    if id == "us:forget-session" {
        return if store.remembered == Some(true) && can_sign_out(store) {
            vec![Intent::Command(Command::ForgetSession)]
        } else {
            Vec::new()
        };
    }
    if let Some(target) = id.strip_prefix("switcher:go=") {
        ui.switcher = None;
        return go(ui, store, target);
    }
    match id {
        "switcher:open" => return open_switcher(ui),
        "switcher:close" => {
            ui.switcher = None;
            return Vec::new();
        }
        _ => {}
    }
    let Some(rest) = id
        .strip_prefix("us:")
        .or_else(|| id.strip_prefix("settings:"))
    else {
        return Vec::new();
    };
    if let Some(page) = rest.strip_prefix("page=").and_then(SettingsPage::from_key) {
        if ui.settings.is_none() {
            return open_settings(ui, store, page);
        }
        return go_to_page(ui, store, page);
    }
    if let Some(key) = rest.strip_prefix("jump=") {
        let Some(page) = ui.settings else {
            return Vec::new();
        };
        return pages::section_key(page, key)
            .map(|key| vec![jump(ui, page, key)])
            .unwrap_or_default();
    }
    if let Some(microphone) = rest.strip_prefix("mic=") {
        return choose_microphone(ui, Some(microphone.to_owned()));
    }
    if let Some(speaker) = rest.strip_prefix("speaker=") {
        ui.settings_state.select = None;
        return crate::profile::choose_output(ui, Some(speaker.to_owned()));
    }
    if let Some(camera) = rest.strip_prefix("camera=") {
        return choose_camera(ui, Some(camera.to_owned()));
    }
    if let Some(theme) = rest.strip_prefix("theme=") {
        // Desktop (P6): or the desktop's, as it changes.
        // Own themes (temas-propios): a built-in one leaves a saved one unused.
        let mut intents = crate::themes::built_in_chosen(ui);
        if theme == "system" {
            intents.extend(crate::desktop::follow_system_theme(ui));
            return intents;
        }
        ui.prefs.system_theme = false;
        ui.theme = match theme {
            "light" => Theme::Light,
            "onyx" => Theme::Onyx,
            _ => Theme::Dark,
        };
        intents.push(save("theme", ui.theme.key()));
        return intents;
    }
    if let Some(intents) = crate::desktop::settings_clicked(ui, store, rest, now) {
        return intents;
    }
    if let Some(intents) = crate::call_sounds::clicked(ui, rest) {
        return intents;
    }
    if let Some(intents) = crate::reading::clicked(ui, rest) {
        return intents;
    }
    if let Some(intents) = pages::cosmetics::clicked(ui, rest) {
        return intents;
    }
    if let Some(hex) = rest.strip_prefix("accent:") {
        return accent(ui, store, hex);
    }
    if let Some(privacy) = rest.strip_prefix("privacy:") {
        // A choice closes the list it was made in.
        ui.settings_state.select = None;
        return crate::account::privacy_clicked(ui, store, privacy, now);
    }
    if let Some(name) = rest.strip_prefix("select:") {
        let state = &mut ui.settings_state;
        state.select = match (name, state.select) {
            ("dismiss", _) => None,
            (name, Some(open)) if open == name => None,
            ("mic", _) => Some("mic"),
            ("speaker", _) => Some("speaker"),
            ("camera", _) => Some("camera"),
            ("role-colors", _) => Some("role-colors"),
            ("explicit-servers", _) => Some("explicit-servers"),
            ("explicit-friends", _) => Some("explicit-friends"),
            ("explicit-others", _) => Some("explicit-others"),
            _ => None,
        };
        return Vec::new();
    }
    ui.settings_state.select = None;
    if let Some(intents) = crate::language::clicked(ui, store, rest) {
        return intents;
    }
    let prefs = &mut ui.prefs;
    let flip = |value: &mut bool, key: &str| {
        *value = !*value;
        vec![save(key, *value)]
    };
    match rest {
        "close" => close_settings(ui),
        "search:clear" => {
            ui.settings_state.query.clear();
            vec![
                Intent::SetInput {
                    id: SEARCH.into(),
                    value: String::new(),
                },
                Intent::Focus(SEARCH.into()),
            ]
        }
        "name:edit" => {
            if profile_locked(store).is_some() {
                return Vec::new();
            }
            let shown = store
                .me_user()
                .map(|me| me.shown_name().to_owned())
                .unwrap_or_default();
            ui.settings_state.name = Some(shown.clone());
            vec![
                Intent::SetInput {
                    id: NAME.into(),
                    value: shown,
                },
                Intent::Focus(NAME.into()),
            ]
        }
        "name:save" => save_name(ui, store),
        "name:cancel" => {
            ui.settings_state.name = None;
            Vec::new()
        }
        "mic-default" => choose_microphone(ui, None),
        "speaker-default" => crate::profile::choose_output(ui, None),
        // A call sound: into the call's sound while in one, instead of
        // taking its output, and heard deafened, since it was asked for.
        "speaker-test" => {
            ui.call_sounds.preview = Some(crate::CallSound::Join);
            Vec::new()
        }
        "loopback" => {
            // A call carrying audio holds the output: hearing oneself waits.
            if crate::quick_voice::call_holds_output(store) {
                return Vec::new();
            }
            // Hearing oneself needs the microphone test: it starts it if
            // stopped, and stopping the test stops this too.
            let start = !ui.audio.testing;
            let intents = if start {
                clicked(ui, store, "us:meter", now)
            } else {
                Vec::new()
            };
            ui.audio.loopback = !ui.audio.loopback || start;
            ui.audio.output_starting = ui.audio.loopback;
            intents
        }
        // Serein-style voice (2026-10-18): the volumes as they start, and
        // noise suppression on or off from the quick panel.
        "reset-levels" => {
            prefs.input_volume = 1.0;
            prefs.output_volume = 1.0;
            vec![save("input_volume", 1.0), save("output_volume", 1.0)]
        }
        "noise" => prefs.edit_processing(|it| {
            it.suppression = match it.suppression {
                rune_media::ProcessingMode::Off => rune_media::ProcessingMode::RnNoise,
                _ => rune_media::ProcessingMode::Off,
            }
        }),
        "echo-cancellation" => prefs.edit_processing(|it| it.echo_cancellation ^= true),
        "automatic-gain" => prefs.edit_processing(|it| it.automatic_gain ^= true),
        "suppression=off" | "suppression=webrtc" | "suppression=rnnoise" => {
            let mode = match rest {
                "suppression=webrtc" => rune_media::ProcessingMode::WebRtc,
                "suppression=rnnoise" => rune_media::ProcessingMode::RnNoise,
                _ => rune_media::ProcessingMode::Off,
            };
            prefs.edit_processing(|it| it.suppression = mode)
        }
        "profile=voice-isolation" | "profile=studio" | "profile=custom" => {
            prefs.input_profile = InputProfile::ALL
                .into_iter()
                .find(|profile| rest.ends_with(profile.key()))
                .unwrap_or_default();
            vec![save("input_profile", prefs.input_profile.key())]
        }
        "camera-default" => choose_camera(ui, None),
        "video-test" => {
            ui.video.testing = !ui.video.testing;
            Vec::new()
        }
        "clock=12" | "clock=24" => {
            prefs.twelve_hour = rest == "clock=12";
            vec![save("clock", if prefs.twelve_hour { "12" } else { "24" })]
        }
        "display=compact" | "display=cozy" => {
            prefs.compact = rest == "display=compact";
            vec![save("compact", prefs.compact)]
        }
        "role-colors=on" | "role-colors=off" => {
            prefs.role_colors = rest == "role-colors=on";
            vec![save("role_colors", prefs.role_colors)]
        }
        "reactions" => flip(&mut prefs.reactions, "reactions"),
        "video-hardware" => flip(&mut prefs.video_hardware, "video_hardware"),
        "system-echo" => flip(&mut prefs.system_echo, "system_echo"),
        "image-previews" => flip(&mut prefs.image_previews, "image_previews"),
        "emoticons" => flip(&mut prefs.convert_emoticons, "convert_emoticons"),
        "developer" => flip(&mut prefs.developer, "developer"),
        "unread-badges" => flip(&mut prefs.unread_badges, "unread_badges"),
        "desktop" => flip(&mut prefs.desktop_notifications, "desktop_notifications"),
        "notification-content" => flip(&mut prefs.notification_content, "notification_content"),
        "message-sound" => flip(&mut prefs.message_sounds, "message_sounds"),
        "mention-badges" => flip(&mut prefs.mention_badges, "mention_badges"),
        "auto-sensitivity" => flip(&mut prefs.auto_sensitivity, "auto_sensitivity"),
        "input=voice" | "input=ptt" => {
            prefs.push_to_talk = rest == "input=ptt";
            ui.settings_state.recording = None;
            vec![save("push_to_talk", prefs.push_to_talk)]
        }
        "ptt-hypr-copy" => ui
            .audio
            .hyprland_bind
            .clone()
            .map(Intent::Copy)
            .into_iter()
            .collect(),
        "ptt-record" => {
            let state = &mut ui.settings_state;
            state.recording = match state.recording {
                Some(_) => None,
                None if prefs.push_to_talk => Some(now),
                None => None,
            };
            state.recording_for =
                crate::keybinds::Action::Voice(crate::desktop::Shortcut::PushToTalk);
            state.key_notice = None;
            Vec::new()
        }
        "motion" => {
            ui.reduced_motion = !ui.reduced_motion;
            vec![
                Intent::ReducedMotion(ui.reduced_motion),
                save("reduced_motion", ui.reduced_motion),
            ]
        }
        "metrics" => {
            ui.show_metrics = !ui.show_metrics;
            vec![save("show_metrics", ui.show_metrics)]
        }
        "meter" => {
            ui.audio.testing = !ui.audio.testing;
            ui.audio.starting = ui.audio.testing;
            ui.audio.loopback = false;
            ui.audio.levels.clear();
            ui.audio.raw_levels.clear();
            if ui.audio.testing {
                vec![Intent::StartMeter(ui.audio.input.clone())]
            } else {
                vec![Intent::StopMeter]
            }
        }
        "devices" => vec![Intent::RefreshDevices],
        "fail-send" => {
            ui.notify(
                t("El próximo envío será rechazado"),
                Some(t("Fallo simulado, para ver el reintento.").into()),
                false,
                now,
            );
            let mut intents = close_settings(ui);
            intents.push(Intent::SimulateSendFailure);
            intents
        }
        "silent-send" => {
            ui.notify(
                t("El próximo envío no tendrá respuesta"),
                Some(t("A los 30 s se marcará como de destino incierto.").into()),
                false,
                now,
            );
            let mut intents = close_settings(ui);
            intents.push(Intent::SimulateSendSilence);
            intents
        }
        "forget" => {
            ui.notify(
                t("Borrando datos locales…"),
                Some(t("Esperando a que se confirme el cambio en este equipo.").into()),
                false,
                now,
            );
            ui.clear_drafts();
            vec![Intent::ForgetLocalData]
        }
        // Private calls.
        "ring-me" => {
            let mut intents = close_settings(ui);
            intents.push(Intent::SimulateIncomingCall);
            intents
        }
        "answer-elsewhere" => {
            let mut intents = close_settings(ui);
            intents.push(Intent::SimulateCallElsewhere);
            intents
        }
        "disconnect" => {
            ui.notify(
                t("Conexión simulada cortada"),
                Some(t("El cliente debería reconectar solo.").into()),
                false,
                now,
            );
            let mut intents = close_settings(ui);
            intents.push(Intent::SimulateDisconnect);
            intents
        }
        _ => Vec::new(),
    }
}

/// The account can leave this provider independently of any open conversation.
pub(crate) fn can_sign_out(store: &Store) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.authentication)
        && matches!(
            store.authentication,
            rune_provider::AuthenticationState::SignedIn { .. }
        )
}

/// Use the camera `id`, or the system's own; the host follows at once.
fn choose_camera(ui: &mut Ui, id: Option<String>) -> Vec<Intent> {
    ui.settings_state.select = None;
    ui.video.device = id.clone();
    vec![Intent::SaveSetting {
        key: "camera".into(),
        value: id,
    }]
}

/// Use the microphone `id`, or the system's own; a test going on follows it.
fn choose_microphone(ui: &mut Ui, id: Option<String>) -> Vec<Intent> {
    ui.settings_state.select = None;
    ui.audio.input = id.clone();
    let mut intents = vec![Intent::SaveSetting {
        key: "microphone".into(),
        value: id,
    }];
    if ui.audio.testing {
        ui.audio.starting = true;
        ui.audio.levels.clear();
        ui.audio.raw_levels.clear();
        intents.push(Intent::StartMeter(ui.audio.input.clone()));
    }
    intents
}

/// The avatar's color: a preset, the field for one's own, or closing it.
fn accent(ui: &mut Ui, store: &Store, what: &str) -> Vec<Intent> {
    match what {
        "custom" => {
            let current = store
                .me_user()
                .map(|me| rune_components::server_settings::hex(me.accent))
                .unwrap_or_default();
            ui.settings_state.accent_hex = Some(current.clone());
            vec![
                Intent::SetInput {
                    id: ACCENT_HEX.into(),
                    value: current,
                },
                Intent::Focus(ACCENT_HEX.into()),
            ]
        }
        "dismiss" => {
            ui.settings_state.accent_hex = None;
            Vec::new()
        }
        hex => match parse_hex(hex) {
            Some(color) => {
                ui.settings_state.accent_hex = None;
                edit_profile(
                    store,
                    ProfileEdit {
                        accent: Some(color),
                        ..ProfileEdit::default()
                    },
                )
            }
            None => Vec::new(),
        },
    }
}

/// Letting go of the avatar color's picker (its field or its hue bar) asks
/// for the color it shows, as Enter in its hex field does: the color is the
/// provider's, and each move before would be a request. It stays open.
pub(crate) fn picker_released(ui: &Ui, store: &Store, action: &UiAction) -> Vec<Intent> {
    let (UiAction::AreaSliderChanged {
        id,
        committed: true,
        ..
    }
    | UiAction::SliderChanged {
        id,
        committed: true,
        ..
    }) = action
    else {
        return Vec::new();
    };
    if id.rsplit_once(':').map(|(picker, _)| picker) != Some(ACCENT) {
        return Vec::new();
    }
    match ui.settings_state.accent_hex.as_deref().and_then(parse_hex) {
        Some(color) if store.me_user().is_some_and(|me| me.accent != color) => edit_profile(
            store,
            ProfileEdit {
                accent: Some(color),
                ..ProfileEdit::default()
            },
        ),
        _ => Vec::new(),
    }
}

/// Esc while the settings are open closes what floats in them first: a
/// list, the color field, the name being written; then the settings.
pub fn intercept(ui: &mut Ui, event: &UiEvent) -> Option<Vec<Intent>> {
    ui.settings?;
    if !matches!(
        event,
        UiEvent::Key {
            key: Key::Escape,
            ..
        }
    ) {
        return None;
    }
    let state = &mut ui.settings_state;
    if state.select.take().is_some()
        || state.accent_hex.take().is_some()
        || state.name.take().is_some()
    {
        return Some(Vec::new());
    }
    Some(close_settings(ui))
}

/// After a layout: mark in the sidebar the section being read, the last
/// one whose top passed the top of the page; at the page's end, its last.
pub fn after_layout(ui: &mut Ui, tree: &UiTree) -> Vec<Intent> {
    let Some(page) = ui.settings else {
        return Vec::new();
    };
    let scroll = page.scroll_id();
    let offset = tree.scroll_offset(&scroll);
    let state = &mut ui.settings_state;
    if let Some((key, at)) = state.jumped {
        match at {
            None => state.jumped = Some((key, Some(offset))),
            Some(at) if (at - offset).abs() > 1.0 => state.jumped = None,
            Some(_) => {}
        }
    }
    let shown: Vec<(&'static str, f32)> = pages::section_keys(page)
        .iter()
        .filter_map(|key| tree.anchor_offset(&anchor(key)).map(|top| (*key, top)))
        .collect();
    let reading = match state.jumped {
        Some((key, _)) => Some(key),
        None if tree.is_scroll_at_end(&scroll, 2.0) && offset > 0.0 => {
            shown.last().map(|(key, _)| *key)
        }
        None => shown
            .iter()
            .take_while(|(_, top)| *top <= pages::jump_margin(page) + 48.0)
            .last()
            .or(shown.first())
            .map(|(key, _)| *key),
    };
    if reading == state.section {
        return Vec::new();
    }
    state.section = reading;
    // The sidebar was painted with the section before: paint it again.
    vec![Intent::Repaint]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_come_back_as_they_were_kept_and_nonsense_is_ignored() {
        let prefs = Prefs {
            compact: true,
            text_size: 18,
            group_gap: 4,
            zoom: 125,
            twelve_hour: true,
            developer: true,
            reactions: false,
            image_previews: false,
            role_colors: false,
            unread_badges: false,
            mention_badges: false,
            input_profile: InputProfile::VoiceIsolation,
            processing: Processing {
                suppression: rune_media::ProcessingMode::RnNoise,
                echo_cancellation: true,
                automatic_gain: false,
            },
            // Off is the one that is not the default.
            system_echo: false,
            input_volume: 1.5,
            auto_sensitivity: false,
            sensitivity: 0.3,
            push_to_talk: true,
            push_to_talk_key: Some("Ctrl+Shift+T".into()),
            push_to_talk_release: 350,
            output_volume: 1.35,
            voice_volumes: BTreeMap::from([(7, (1.5, false)), (9, (1.0, true))]),
            hidden_video: BTreeSet::from([3]),
            screen_restore: Some("4f1c-portal-choice".into()),
            desktop_notifications: true,
            notification_content: false,
            message_sounds: false,
            // Desktop (P6); the theme the desktop chooses is kept as the theme's.
            close_to_tray: true,
            system_theme: false,
            ui_font: Some("Inter".into()),
            sidebar_width: 300.0,
            members_width: 260.0,
            call_chat_width: 520.0,
            favorites: vec![ChannelId(3), ChannelId(1)],
            pinned: vec![ChannelId(9)],
            show_hidden: true,
            mute_key: Some("Ctrl+Shift+M".into()),
            deafen_key: Some("Ctrl+Shift+D".into()),
            push_to_mute_key: Some("F8".into()),
            rich_presence: true,
            game_activity: true,
            language: Some(rune_i18n::Language::English),
            sounds_off: vec![crate::CallSound::UserJoin, crate::CallSound::Disconnect],
            sound_volume: 0.6,
            sound_folder: Some("/home/vos/sonidos".into()),
            video_hardware: false,
            convert_emoticons: true,
            // Reading and pictures on disk (imagenes-y-lectura).
            animate_gifs: false,
            hide_media_links: false,
            scroll_speed: 175,
            members_narrow: true,
            media_cache_mib: 256,
            artwork_cache_mib: 512,
            cache_days: 365,
            cosmetics: Cosmetics {
                decorations: false,
                nameplates: true,
                effects: false,
            },
            // Desktop session and games (escritorio-sesion-y-juegos).
            registered_games: vec![crate::games::RegisteredGame {
                executable: "bin/juego".into(),
                name: "Juego".into(),
                application: Some(7),
                hidden: true,
                last_played: Some(1_700_000_000_000),
            }],
            // Editable shortcuts (atajos-y-navegacion).
            shortcuts: vec![(
                crate::keybinds::Action::Back,
                crate::keybinds::Chord::ctrl(limn::Key::J),
            )],
        };
        let kept = prefs.kept();
        let mut back = Prefs::default();
        back.restore(&|key: &str| {
            kept.iter()
                .find(|(kept, _)| *kept == key)
                .and_then(|(_, value)| value.clone())
        });
        assert_eq!(back, prefs);
        let mut odd = Prefs::default();
        odd.restore(&|key: &str| match key {
            "chat_text_size" => Some("13".into()),
            "zoom" => Some("900".into()),
            "sensitivity" => Some("nan".into()),
            "push_to_talk_key" => Some("Ctrl+ T".into()),
            "push_to_talk_release" => Some("90000".into()),
            "output_volume" => Some("3".into()),
            "input_volume" => Some("-1".into()),
            "input_profile" => Some("krisp".into()),
            "noise_suppression" => Some("krisp".into()),
            "voice_volumes" => Some("7:9:0;x:1:1;8:1.5:2;9:0.5:1:4".into()),
            "shortcuts" => Some("back=K;send=Tab;nada=F1".into()),
            _ => None,
        });
        assert_eq!(odd, Prefs::default());
    }

    #[test]
    fn profiles_set_the_processing_and_editing_one_makes_it_one_s_own() {
        let mut prefs = Prefs::default();
        assert_eq!(
            prefs.input_profile,
            InputProfile::VoiceIsolation,
            "as Discord"
        );
        prefs.input_profile = InputProfile::Custom;
        assert_eq!(prefs.effective_processing(), Processing::default(), "off");
        prefs.input_profile = InputProfile::VoiceIsolation;
        let isolation = prefs.effective_processing();
        assert_eq!(isolation.suppression, rune_media::ProcessingMode::RnNoise);
        assert!(isolation.echo_cancellation && isolation.automatic_gain);
        prefs.input_profile = InputProfile::Studio;
        assert_eq!(prefs.effective_processing(), Processing::default());
        // Editing starts from what the preset showed, not hidden values.
        prefs.input_profile = InputProfile::VoiceIsolation;
        let saved = prefs.edit_processing(|it| it.automatic_gain = false);
        assert_eq!(prefs.input_profile, InputProfile::Custom);
        assert_eq!(
            prefs.effective_processing(),
            Processing {
                automatic_gain: false,
                ..isolation
            }
        );
        assert_eq!(saved.len(), 4, "the profile and the three it sets");
        // The switch of older settings meant WebRTC's suppressor.
        let mut old = Prefs::default();
        old.restore(&|key: &str| (key == "noise_suppression").then(|| "true".into()));
        assert_eq!(
            old.effective_processing().suppression,
            rune_media::ProcessingMode::WebRtc
        );
    }

    #[test]
    fn a_threshold_of_one_s_own_moves_where_speech_starts() {
        let start = Instant::now();
        let mut voice = rune_media::VoiceActivity::default();
        let mut prefs = Prefs::default();
        // By default RNNoise's gate decides; what it let through is speech.
        assert!(prefs.speech_gate());
        prefs.tune(&mut voice);
        assert!(!voice.automatic && voice.on == rune_media::VoiceActivity::GATED_ON);
        assert!(voice.update(0.4, start), "let through by the gate");
        voice.reset();
        // Without RNNoise, the room decides.
        prefs.input_profile = InputProfile::Studio;
        assert!(!prefs.speech_gate());
        prefs.tune(&mut voice);
        assert!(voice.automatic, "the room decides");
        assert!(!voice.update(0.4, start), "under RUNE's own level");
        // Nor with push-to-talk, the key decides.
        prefs.input_profile = InputProfile::VoiceIsolation;
        prefs.push_to_talk = true;
        assert!(!prefs.speech_gate());
        prefs.push_to_talk = false;
        prefs.auto_sensitivity = false;
        assert!(!prefs.speech_gate(), "a threshold of one's own");
        prefs.sensitivity = 0.3;
        prefs.tune(&mut voice);
        assert!(voice.update(0.4, start), "over the account's");
        assert!(voice.off < voice.on);
    }
}
