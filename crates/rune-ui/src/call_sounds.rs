//! The call's sounds as the settings choose them: which events sound, how
//! loud, and a folder of one's own files that play instead of RUNE's
//! (`join.ogg`, `mute.wav`…). The host plays them and reads the folder off
//! the interface's thread; this says what is wanted and shows what it found.

use rune_i18n::{es, t};

use crate::settings::Prefs;
use crate::{Intent, Ui};

/// The folder's field in the settings.
pub(crate) const FOLDER: &str = "us:sound-folder";

/// Something that happens in a call, or a message, with a sound of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CallSound {
    /// The account's call connects.
    Join,
    /// The account leaves its call.
    Leave,
    /// Someone else comes into the account's call, or leaves it.
    UserJoin,
    UserLeave,
    Mute,
    Unmute,
    Deafen,
    Undeafen,
    /// The account's microphone opens or closes by push-to-talk, as
    /// Discord: only in that mode, and not while self-muted.
    PttStart,
    PttStop,
    /// The account's camera starts or stops sending in its call.
    CameraOn,
    CameraOff,
    /// The account's own stream starts and ends.
    StreamStart,
    StreamStop,
    /// Someone else in the call starts streaming.
    UserStream,
    /// The call drops or fails.
    Disconnect,
    /// A mention or a direct message: see [`crate::message_alert`].
    Message,
    // Private calls.
    /// A call coming in rings, again and again: see [`crate::call_ringing`].
    Ring,
    /// The account's call rings the others.
    RingOut,
}

impl CallSound {
    pub const ALL: [Self; 19] = [
        Self::Join,
        Self::Leave,
        Self::UserJoin,
        Self::UserLeave,
        Self::Mute,
        Self::Unmute,
        Self::Deafen,
        Self::Undeafen,
        Self::PttStart,
        Self::PttStop,
        Self::CameraOn,
        Self::CameraOff,
        Self::StreamStart,
        Self::StreamStop,
        Self::UserStream,
        Self::Disconnect,
        Self::Message,
        Self::Ring,
        Self::RingOut,
    ];

    /// Its file's name in the sounds folder, without the extension; also
    /// how the settings keep it.
    pub fn key(self) -> &'static str {
        match self {
            Self::Join => "join",
            Self::Leave => "leave",
            Self::UserJoin => "user_join",
            Self::UserLeave => "user_leave",
            Self::Mute => "mute",
            Self::Unmute => "unmute",
            Self::Deafen => "deafen",
            Self::Undeafen => "undeafen",
            Self::PttStart => "ptt_start",
            Self::PttStop => "ptt_stop",
            Self::CameraOn => "camera_on",
            Self::CameraOff => "camera_off",
            Self::StreamStart => "stream_start",
            Self::StreamStop => "stream_stop",
            Self::UserStream => "user_stream",
            Self::Disconnect => "disconnect",
            Self::Message => "message",
            Self::Ring => "ring",
            Self::RingOut => "ring_out",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|sound| sound.key() == key)
    }

    /// What the settings call it.
    pub fn label(self) -> &'static str {
        t(match self {
            Self::Join => es("Entrar a la llamada"),
            Self::Leave => es("Salir de la llamada"),
            Self::UserJoin => es("Alguien entra"),
            Self::UserLeave => es("Alguien sale"),
            Self::Mute => es("Silenciarte"),
            Self::Unmute => es("Dejar de silenciarte"),
            Self::Deafen => es("Ensordecerte"),
            Self::Undeafen => es("Dejar de ensordecerte"),
            Self::PttStart => es("Pulsar para hablar: activar"),
            Self::PttStop => es("Pulsar para hablar: soltar"),
            Self::CameraOn => es("Encender la cámara"),
            Self::CameraOff => es("Apagar la cámara"),
            Self::StreamStart => es("Empezar a transmitir"),
            Self::StreamStop => es("Dejar de transmitir"),
            Self::UserStream => es("Alguien empieza a transmitir"),
            Self::Disconnect => es("Desconexión o error"),
            Self::Message => es("Mensaje nuevo"),
            Self::Ring => es("Llamada entrante"),
            Self::RingOut => es("Llamando a alguien"),
        })
    }

    /// Heard while deafened: only deafening and undeafening, as in Discord,
    /// and a call coming in, which nothing else would announce.
    pub fn heard_deafened(self) -> bool {
        matches!(self, Self::Deafen | Self::Undeafen | Self::Ring)
    }

    /// How often it plays again while its event lasts: the rings.
    pub fn repeats(self) -> Option<std::time::Duration> {
        match self {
            Self::Ring => Some(std::time::Duration::from_millis(2_500)),
            Self::RingOut => Some(std::time::Duration::from_secs(3)),
            _ => None,
        }
    }
}

/// What the page shows of the sounds and what it asks of the host.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CallSounds {
    /// What the folder's field holds while it is typed in.
    pub typed: Option<String>,
    /// What the host found in the folder in use.
    pub found: Option<FolderReport>,
    /// A sound to play once, asked for by «Probar».
    pub preview: Option<CallSound>,
    /// The service ended the call, with why: its sound is the
    /// disconnection's, not leaving's. The host takes it.
    pub dropped: bool,
    // Linux desktop (escritorio-linux).
    /// The system's dialog is choosing the folder, for this request.
    pub choosing: Option<u64>,
    /// Why the system's dialog could not open, apart from the folder in use.
    pub dialog_problem: Option<String>,
}

/// What the host found in a folder of sounds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FolderReport {
    /// The folder, as the settings keep it.
    pub folder: String,
    /// The sounds it gives, by event, with their file's name.
    pub taken: Vec<(CallSound, String)>,
    /// Its files that do not do, with why.
    pub refused: Vec<(String, String)>,
    /// Why the folder itself could not be read.
    pub error: Option<String>,
}

impl Prefs {
    /// Whether `sound` plays when its event happens.
    pub fn sound_on(&self, sound: CallSound) -> bool {
        match sound {
            CallSound::Message => self.message_sounds,
            sound => !self.sounds_off.contains(&sound),
        }
    }
}

/// A folder as RUNE keeps it: absolute, or from the home folder (`~/`),
/// printable and short.
pub(crate) fn valid_folder(folder: &str) -> bool {
    (folder.starts_with('/') || folder.starts_with("~/"))
        && folder.len() <= 4096
        && !folder.chars().any(char::is_control)
}

/// The preferences of these sounds as settings to keep.
pub(crate) fn kept(prefs: &Prefs) -> Vec<(&'static str, Option<String>)> {
    let off: Vec<&str> = prefs.sounds_off.iter().map(|sound| sound.key()).collect();
    vec![
        ("sounds_off", Some(off.join(","))),
        ("sound_volume", Some(prefs.sound_volume.to_string())),
        ("sound_folder", prefs.sound_folder.clone()),
    ]
}

/// Take back what [`kept`] kept.
pub(crate) fn restore(prefs: &mut Prefs, setting: &impl Fn(&str) -> Option<String>) {
    use CallSound::*;
    prefs.sounds_off = match setting("sounds_off") {
        Some(kept) => {
            let mut off: Vec<CallSound> = kept
                .split(',')
                .filter_map(CallSound::from_key)
                .filter(|sound| *sound != Message)
                .collect();
            off.sort_unstable();
            off.dedup();
            off
        }
        // Before each event had its switch, families had one.
        None => [
            ("sound_people", &[UserJoin, UserLeave][..]),
            ("sound_self", &[Mute, Unmute, Deafen, Undeafen][..]),
            ("sound_disconnect", &[Leave, Disconnect][..]),
        ]
        .into_iter()
        .filter(|(key, _)| setting(key).as_deref() == Some("false"))
        .flat_map(|(_, sounds)| sounds.iter().copied())
        .collect(),
    };
    prefs.sound_volume = setting("sound_volume")
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| (0.0..=1.0).contains(value))
        .unwrap_or(Prefs::default().sound_volume);
    prefs.sound_folder = setting("sound_folder").filter(|folder| valid_folder(folder));
}

fn save_off(prefs: &Prefs) -> Intent {
    let off: Vec<&str> = prefs.sounds_off.iter().map(|sound| sound.key()).collect();
    Intent::SaveSetting {
        key: "sounds_off".into(),
        value: Some(off.join(",")),
    }
}

/// A click of these settings (`id` without its `us:`); `None` for others.
pub(crate) fn clicked(ui: &mut Ui, id: &str) -> Option<Vec<Intent>> {
    if let Some(key) = id.strip_prefix("sound=") {
        let sound = CallSound::from_key(key)?;
        let prefs = &mut ui.prefs;
        if sound == CallSound::Message {
            prefs.message_sounds = !prefs.message_sounds;
            return Some(vec![Intent::SaveSetting {
                key: "message_sounds".into(),
                value: Some(prefs.message_sounds.to_string()),
            }]);
        }
        match prefs.sounds_off.iter().position(|off| *off == sound) {
            Some(at) => {
                prefs.sounds_off.remove(at);
            }
            None => {
                prefs.sounds_off.push(sound);
                prefs.sounds_off.sort_unstable();
            }
        }
        return Some(vec![save_off(prefs)]);
    }
    if let Some(key) = id.strip_prefix("sound-test=") {
        ui.call_sounds.preview = CallSound::from_key(key);
        return Some(Vec::new());
    }
    match id {
        "sound-folder:use" => Some(use_folder(ui)),
        // Linux desktop (escritorio-linux).
        "sound-folder:choose" => {
            if ui.call_sounds.choosing.is_some() {
                return Some(Vec::new());
            }
            let request = ui.fresh_request();
            ui.call_sounds.choosing = Some(request);
            ui.call_sounds.dialog_problem = None;
            Some(vec![Intent::ChooseSoundFolder { request }])
        }
        "sound-folder:clear" => {
            ui.call_sounds.typed = None;
            ui.call_sounds.found = None;
            ui.prefs.sound_folder = None;
            Some(vec![Intent::SaveSetting {
                key: "sound_folder".into(),
                value: None,
            }])
        }
        _ => None,
    }
}

/// «Usar»: the folder typed, if it reads as one, replaces RUNE's sounds.
pub(crate) fn use_folder(ui: &mut Ui) -> Vec<Intent> {
    let Some(typed) = ui.call_sounds.typed.take() else {
        return Vec::new();
    };
    let folder = typed.trim();
    if folder.is_empty() {
        return Vec::new();
    }
    if !valid_folder(folder) {
        ui.call_sounds.found = Some(FolderReport {
            folder: folder.to_owned(),
            error: Some(t("Escribe la ruta completa de la carpeta, desde / o ~/.").into()),
            ..FolderReport::default()
        });
        ui.call_sounds.typed = Some(typed);
        return Vec::new();
    }
    set_folder(ui, folder.to_owned())
}

/// `folder` replaces RUNE's sounds; the host reads it again and says what
/// it found.
fn set_folder(ui: &mut Ui, folder: String) -> Vec<Intent> {
    ui.call_sounds.found = None;
    ui.call_sounds.dialog_problem = None;
    ui.prefs.sound_folder = Some(folder.clone());
    vec![Intent::SaveSetting {
        key: "sound_folder".into(),
        value: Some(folder),
    }]
}

/// The dialog under way closes with the account it was opened for.
pub(crate) fn stop_choosing(ui: &mut Ui) -> Option<Intent> {
    ui.call_sounds
        .choosing
        .take()
        .map(|_| Intent::CancelSoundFolder)
}

/// What the system's dialog chose for `request` (escritorio-linux): a
/// folder that replaces RUNE's sounds, as «Usar» with it typed; nothing
/// when the person cancelled; or why it could not open.
pub fn sound_folder_chosen(
    ui: &mut Ui,
    request: u64,
    chosen: Result<Option<String>, String>,
) -> Vec<Intent> {
    if ui.call_sounds.choosing != Some(request) {
        return Vec::new();
    }
    ui.call_sounds.choosing = None;
    match chosen {
        // Exactly as chosen: a name may begin or end with a space.
        Ok(Some(folder)) if valid_folder(&folder) => {
            ui.call_sounds.typed = None;
            set_folder(ui, folder)
        }
        Ok(Some(folder)) => {
            ui.call_sounds.typed = Some(folder);
            use_folder(ui)
        }
        Ok(None) => Vec::new(),
        // The folder in use still plays: its report stays.
        Err(reason) => {
            ui.call_sounds.dialog_problem = Some(reason);
            Vec::new()
        }
    }
}

/// The sound «Probar» asked for, once.
pub fn take_preview(ui: &mut Ui) -> Option<CallSound> {
    ui.call_sounds.preview.take()
}

/// The host says what it found in the folder in use.
pub fn folder_read(ui: &mut Ui, report: FolderReport) {
    ui.call_sounds.found = Some(report);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_has_a_key_a_label_and_only_deafening_is_heard_deafened() {
        for sound in CallSound::ALL {
            assert_eq!(CallSound::from_key(sound.key()), Some(sound));
            assert!(!sound.label().is_empty());
        }
        let heard: Vec<_> = CallSound::ALL
            .into_iter()
            .filter(|sound| sound.heard_deafened())
            .collect();
        assert_eq!(
            heard,
            [CallSound::Deafen, CallSound::Undeafen, CallSound::Ring]
        );
        assert_eq!(CallSound::from_key("nope"), None);
    }

    #[test]
    fn the_old_families_become_events_and_what_is_kept_comes_back() {
        let old = |key: &str| {
            matches!(key, "sound_people" | "sound_disconnect").then(|| "false".to_owned())
        };
        let mut prefs = Prefs::default();
        restore(&mut prefs, &old);
        use CallSound::*;
        assert_eq!(prefs.sounds_off, [UserJoin, UserLeave, Leave, Disconnect]);
        assert!(prefs.sound_on(Mute) && !prefs.sound_on(UserLeave));
        prefs.sounds_off = vec![Deafen, StreamStart];
        prefs.sound_volume = 0.4;
        prefs.sound_folder = Some("~/sonidos".into());
        let kept = kept(&prefs);
        let mut back = Prefs::default();
        restore(&mut back, &|key: &str| {
            kept.iter()
                .find(|(kept, _)| *kept == key)
                .and_then(|(_, value)| value.clone())
        });
        assert_eq!(back.sounds_off, [Deafen, StreamStart]);
        assert_eq!(
            (back.sound_volume, back.sound_folder.as_deref()),
            (0.4, Some("~/sonidos"))
        );
        // Nonsense goes back to the usual.
        let mut odd = Prefs::default();
        restore(&mut odd, &|key: &str| match key {
            "sound_volume" => Some("7".into()),
            "sound_folder" => Some("relative/path".into()),
            "sounds_off" => Some("mute,nope,message,mute".into()),
            _ => None,
        });
        assert_eq!(odd.sounds_off, [Mute], "the message has its own switch");
        assert_eq!((odd.sound_volume, odd.sound_folder), (1.0, None));
    }

    // Linux desktop (escritorio-linux).
    #[test]
    fn the_dialog_chooses_the_folder_once_and_only_for_its_request() {
        let mut ui = Ui::new();
        let asked = clicked(&mut ui, "sound-folder:choose").unwrap();
        let request = ui.call_sounds.choosing.unwrap();
        assert_eq!(asked, [Intent::ChooseSoundFolder { request }]);
        assert!(
            clicked(&mut ui, "sound-folder:choose").unwrap().is_empty(),
            "one dialog at a time"
        );
        // Another request's answer changes nothing.
        assert!(sound_folder_chosen(&mut ui, request + 1, Ok(Some("/otra".into()))).is_empty());
        assert_eq!(ui.prefs.sound_folder, None);
        assert_eq!(
            sound_folder_chosen(&mut ui, request, Ok(Some("/home/vos/sonidos".into()))),
            [Intent::SaveSetting {
                key: "sound_folder".into(),
                value: Some("/home/vos/sonidos".into())
            }]
        );
        assert_eq!(ui.prefs.sound_folder.as_deref(), Some("/home/vos/sonidos"));
        assert_eq!(ui.call_sounds.choosing, None);
        // Cancelled: nothing changes; refused: it says why.
        clicked(&mut ui, "sound-folder:choose");
        let request = ui.call_sounds.choosing.unwrap();
        assert!(sound_folder_chosen(&mut ui, request, Ok(None)).is_empty());
        assert_eq!(ui.prefs.sound_folder.as_deref(), Some("/home/vos/sonidos"));
        // The folder in use keeps its report beside the dialog's problem.
        let report = FolderReport {
            folder: "/home/vos/sonidos".into(),
            taken: vec![(CallSound::Join, "join.ogg".into())],
            ..FolderReport::default()
        };
        folder_read(&mut ui, report.clone());
        clicked(&mut ui, "sound-folder:choose");
        let request = ui.call_sounds.choosing.unwrap();
        assert!(sound_folder_chosen(&mut ui, request, Err("ocupado".into())).is_empty());
        assert_eq!(ui.call_sounds.dialog_problem.as_deref(), Some("ocupado"));
        assert_eq!(ui.call_sounds.found.as_ref(), Some(&report));
        assert_eq!(ui.prefs.sound_folder.as_deref(), Some("/home/vos/sonidos"));
        // A folder whose name ends in a space is kept as chosen, not trimmed.
        clicked(&mut ui, "sound-folder:choose");
        assert_eq!(ui.call_sounds.dialog_problem, None);
        let request = ui.call_sounds.choosing.unwrap();
        assert_eq!(
            sound_folder_chosen(&mut ui, request, Ok(Some("/home/vos/sonidos ".into()))),
            [Intent::SaveSetting {
                key: "sound_folder".into(),
                value: Some("/home/vos/sonidos ".into())
            }]
        );
        assert_eq!(ui.prefs.sound_folder.as_deref(), Some("/home/vos/sonidos "));
    }
}
