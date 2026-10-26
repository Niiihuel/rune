//! The keyboard shortcuts RUNE answers, as one table (atajos-y-navegacion):
//! each action, the key it has by default, the one the person chose, and
//! what makes a key unfit for it. The list of shortcuts and the settings
//! read it; the host and [`crate::intercept`] dispatch by it, as Serein's
//! `keybinds.rs` does.
//!
//! The voice shortcuts keep the settings they always had
//! (`push_to_talk_key`, `mute_key`…): the host holds them and the
//! desktop's portal binds them. The rest keep only what differs from the
//! default, in the setting [`SETTING`].

use std::time::Instant;

use limn::{Key, UiEvent};
use rune_i18n::{es, t, tr};
use rune_state::Store;

use crate::desktop::Shortcut;
use crate::settings::Prefs;
use crate::{Intent, Ui};

/// The setting that keeps the shortcuts changed, `action=Ctrl+K;…`.
pub(crate) const SETTING: &str = "shortcuts";

/// The parts of the list, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Navigation,
    Messages,
    Formatting,
    Voice,
}

impl Group {
    pub const ALL: [Self; 4] = [
        Self::Navigation,
        Self::Messages,
        Self::Formatting,
        Self::Voice,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Navigation => t("Navegación"),
            Self::Messages => t("Mensajes"),
            Self::Formatting => t("Formato"),
            Self::Voice => t("Voz"),
        }
    }
}

/// What a shortcut does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Shortcuts,
    Switcher,
    Search,
    PreviousChannel,
    NextChannel,
    Back,
    Forward,
    NextRegion,
    PreviousRegion,
    Metrics,
    Send,
    NewLine,
    EditLast,
    Bold,
    Italic,
    Underline,
    Strikethrough,
    InlineCode,
    CodeBlock,
    Spoiler,
    /// Held or toggled by the host, never dispatched from an event.
    Voice(Shortcut),
}

impl Default for Action {
    /// What the settings record when nothing else was asked: push to talk.
    fn default() -> Self {
        Self::Voice(Shortcut::PushToTalk)
    }
}

/// Every action, in the list's order.
pub const ACTIONS: [Action; 24] = [
    Action::Shortcuts,
    Action::Switcher,
    Action::Search,
    Action::PreviousChannel,
    Action::NextChannel,
    Action::Back,
    Action::Forward,
    Action::NextRegion,
    Action::PreviousRegion,
    Action::Metrics,
    Action::Send,
    Action::NewLine,
    Action::EditLast,
    Action::Bold,
    Action::Italic,
    Action::Underline,
    Action::Strikethrough,
    Action::InlineCode,
    Action::CodeBlock,
    Action::Spoiler,
    Action::Voice(Shortcut::PushToTalk),
    Action::Voice(Shortcut::Mute),
    Action::Voice(Shortcut::Deafen),
    Action::Voice(Shortcut::PushToMute),
];

impl Action {
    /// Its name in the setting and in its controls' IDs
    /// (`us:key-record={key}`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Shortcuts => "shortcuts",
            Self::Switcher => "switcher",
            Self::Search => "search",
            Self::PreviousChannel => "channel-up",
            Self::NextChannel => "channel-down",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::NextRegion => "region-next",
            Self::PreviousRegion => "region-previous",
            Self::Metrics => "metrics",
            Self::Send => "send",
            Self::NewLine => "new-line",
            Self::EditLast => "edit-last",
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::Underline => "underline",
            Self::Strikethrough => "strikethrough",
            Self::InlineCode => "code",
            Self::CodeBlock => "code-block",
            Self::Spoiler => "spoiler",
            Self::Voice(shortcut) => shortcut.key(),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        ACTIONS.into_iter().find(|action| action.key() == key)
    }

    /// What it does, as the list says it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Shortcuts => t("Mostrar esta lista de atajos"),
            Self::Switcher => t("Selector rápido de conversaciones"),
            Self::Search => t("Buscar en la conversación"),
            Self::PreviousChannel => t("Canal anterior"),
            Self::NextChannel => t("Canal siguiente"),
            Self::Back => t("Atrás en el historial"),
            Self::Forward => t("Adelante en el historial"),
            Self::NextRegion => t("Región siguiente de la ventana"),
            Self::PreviousRegion => t("Región anterior de la ventana"),
            Self::Metrics => t("Mostrar u ocultar las métricas"),
            Self::Send => t("Enviar el mensaje"),
            Self::NewLine => t("Escribir en una línea nueva"),
            Self::EditLast => t("Editar tu último mensaje, con el cuadro vacío"),
            Self::Bold => t("Negrita"),
            Self::Italic => t("Cursiva"),
            Self::Underline => t("Subrayado"),
            Self::Strikethrough => t("Tachado"),
            Self::InlineCode => t("Código"),
            Self::CodeBlock => t("Bloque de código"),
            Self::Spoiler => t("Spoiler"),
            Self::Voice(shortcut) => shortcut.label(),
        }
    }

    pub fn group(self) -> Group {
        match self {
            Self::Send | Self::NewLine | Self::EditLast => Group::Messages,
            Self::Bold
            | Self::Italic
            | Self::Underline
            | Self::Strikethrough
            | Self::InlineCode
            | Self::CodeBlock
            | Self::Spoiler => Group::Formatting,
            Self::Voice(_) => Group::Voice,
            _ => Group::Navigation,
        }
    }

    /// Its key until the person chooses another; the voice shortcuts have
    /// none, as before.
    pub fn default_chord(self) -> Option<Chord> {
        Some(match self {
            Self::Shortcuts => Chord::ctrl(Key::Slash),
            Self::Switcher => Chord::ctrl(Key::K),
            Self::Search => Chord::ctrl(Key::F),
            Self::PreviousChannel => Chord::alt(Key::ArrowUp),
            Self::NextChannel => Chord::alt(Key::ArrowDown),
            Self::Back => Chord::alt(Key::ArrowLeft),
            Self::Forward => Chord::alt(Key::ArrowRight),
            Self::NextRegion => Chord::plain(Key::F6),
            Self::PreviousRegion => Chord::shift(Key::F6),
            Self::Metrics => Chord::plain(Key::F12),
            Self::Send => Chord::plain(Key::Enter),
            Self::NewLine => Chord::shift(Key::Enter),
            Self::EditLast => Chord::plain(Key::ArrowUp),
            // As the official client and Serein bind them.
            Self::Bold => Chord::ctrl(Key::B),
            Self::Italic => Chord::ctrl(Key::I),
            Self::Underline => Chord::ctrl(Key::U),
            Self::Strikethrough => Chord::ctrl_shift(Key::X),
            Self::InlineCode => Chord::ctrl(Key::E),
            Self::CodeBlock => Chord::ctrl_shift(Key::C),
            Self::Spoiler => Chord::ctrl_shift(Key::P),
            Self::Voice(_) => return None,
        })
    }

    /// Whether the window reads it from its key events, rather than the
    /// host holding it.
    fn dispatched(self) -> bool {
        !matches!(self, Self::Voice(_))
    }
}

/// A key and the modifiers held with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Chord {
    pub const fn plain(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    pub const fn ctrl(key: Key) -> Self {
        Self {
            ctrl: true,
            ..Self::plain(key)
        }
    }

    pub const fn alt(key: Key) -> Self {
        Self {
            alt: true,
            ..Self::plain(key)
        }
    }

    pub const fn shift(key: Key) -> Self {
        Self {
            shift: true,
            ..Self::plain(key)
        }
    }

    pub const fn ctrl_shift(key: Key) -> Self {
        Self {
            ctrl: true,
            shift: true,
            ..Self::plain(key)
        }
    }

    /// The chord a host wrote, `Ctrl+Alt+Shift+Slash`: modifiers, then the
    /// key's name as [`Key::name`] says it. `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        let (modifiers, key) = text.rsplit_once('+').unwrap_or(("", text));
        let mut chord = Self::plain(Key::from_name(key)?);
        for modifier in modifiers.split('+').filter(|part| !part.is_empty()) {
            match modifier {
                "Ctrl" => chord.ctrl = true,
                "Alt" => chord.alt = true,
                "Shift" => chord.shift = true,
                _ => return None,
            }
        }
        Some(chord)
    }

    /// As the settings keep it and [`Self::parse`] reads it.
    pub fn name(self) -> String {
        self.joined(self.key.name())
    }

    /// As the key caps show it: `Ctrl+/`, `Alt+←`.
    pub fn label(self) -> String {
        self.joined(self.shown())
    }

    /// One cap each: `["Ctrl", "/"]`.
    pub fn caps(self) -> Vec<String> {
        [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
        ]
        .into_iter()
        .filter(|(held, _)| *held)
        .map(|(_, modifier)| modifier)
        .chain([self.shown()])
        .map(str::to_owned)
        .collect()
    }

    /// The key as its cap shows it.
    fn shown(self) -> &'static str {
        match self.key {
            Key::ArrowLeft => "←",
            Key::ArrowRight => "→",
            Key::ArrowUp => "↑",
            Key::ArrowDown => "↓",
            Key::Escape => "Esc",
            Key::Space => t("Espacio"),
            Key::PageUp => t("RePág"),
            Key::PageDown => t("AvPág"),
            key => typed(key).unwrap_or(key.name()),
        }
    }

    fn joined(self, key: &str) -> String {
        let mut text = String::new();
        for (held, modifier) in [
            (self.ctrl, "Ctrl+"),
            (self.alt, "Alt+"),
            (self.shift, "Shift+"),
        ] {
            if held {
                text.push_str(modifier);
            }
        }
        text.push_str(key);
        text
    }

    /// The same chord without the Shift a typed character may need: `/` is
    /// Shift+7 on Spanish and German layouts, and a digit Shift on French
    /// ones. Ctrl+/ stays Ctrl+/ whichever layout types it.
    fn normalized(self) -> Self {
        Self {
            shift: self.shift && typed(self.key).is_none(),
            ..self
        }
    }

    /// Whether the key event is this chord: its key and exactly its
    /// modifiers, Shift aside for a typed character.
    pub fn matches(self, key: Key, ctrl: bool, alt: bool, shift: bool) -> bool {
        Self {
            key,
            ctrl,
            alt,
            shift,
        }
        .normalized()
            == self.normalized()
    }

    fn same(self, other: Self) -> bool {
        self.normalized() == other.normalized()
    }
}

/// The character a punctuation or digit key types, which a layout may put
/// behind Shift.
fn typed(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::Slash => "/",
        Key::Colon => ":",
        Key::Comma => ",",
        Key::Minus => "-",
        Key::Period => ".",
        Key::Plus => "+",
        Key::Equals => "=",
        Key::Semicolon => ";",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::OpenCurlyBracket => "{",
        Key::CloseCurlyBracket => "}",
        Key::Backtick => "`",
        Key::Backslash => "\\",
        Key::Pipe => "|",
        Key::Questionmark => "?",
        Key::Exclamationmark => "!",
        Key::Quote => "'",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        _ => return None,
    })
}

/// The chord `action` answers to now; `None` when it has none.
pub fn binding(prefs: &Prefs, action: Action) -> Option<Chord> {
    match action {
        Action::Voice(shortcut) => shortcut.binding(prefs).and_then(Chord::parse),
        _ => prefs
            .shortcuts
            .iter()
            .find(|(chosen, _)| *chosen == action)
            .map(|(_, chord)| *chord)
            .or(action.default_chord()),
    }
}

/// What `action`'s key caps say, for the list and the hints; `None`
/// without a key.
pub fn keys(prefs: &Prefs, action: Action) -> Option<String> {
    match action {
        // A key the host named that limn has no name for still shows.
        Action::Voice(shortcut) => shortcut
            .binding(prefs)
            .map(|text| Chord::parse(text).map_or_else(|| text.to_owned(), Chord::label)),
        _ => binding(prefs, action).map(Chord::label),
    }
}

/// `action`'s keys, one cap each, for the settings; `None` without a key.
pub fn caps(prefs: &Prefs, action: Action) -> Option<Vec<String>> {
    match action {
        Action::Voice(shortcut) => shortcut.binding(prefs).map(|text| {
            Chord::parse(text)
                .map_or_else(|| text.split('+').map(str::to_owned).collect(), Chord::caps)
        }),
        _ => binding(prefs, action).map(Chord::caps),
    }
}

/// The action a key event asks for, if one is bound to it.
pub fn pressed(prefs: &Prefs, event: &UiEvent) -> Option<Action> {
    let UiEvent::Key {
        key,
        shift,
        ctrl,
        alt,
    } = *event
    else {
        return None;
    };
    ACTIONS.into_iter().find(|action| {
        action.dispatched()
            && binding(prefs, *action).is_some_and(|chord| chord.matches(key, ctrl, alt, shift))
    })
}

/// Why `chord` cannot be `action`'s, if it cannot: keys the fields, the
/// lists and the focus need. The voice keys are held, not typed, so only
/// another shortcut keeps one from them.
fn refused(action: Action, chord: Chord) -> Option<&'static str> {
    if !action.dispatched() {
        return None;
    }
    let modified = chord.ctrl || chord.alt;
    if chord.key == Key::Tab && !chord.ctrl {
        return Some(es("Tab mueve el foco entre los controles: elige otra."));
    }
    if chord.key == Key::F10 && chord.shift && !modified {
        return Some(es("Shift+F10 abre el menú contextual: elige otra."));
    }
    if chord.ctrl
        && !chord.alt
        && matches!(
            chord.key,
            Key::A | Key::C | Key::V | Key::X | Key::Z | Key::Y
        )
        && !(chord.shift && matches!(chord.key, Key::C | Key::X))
    {
        return Some(es("La usa la edición de texto: elige otra."));
    }
    let name = chord.key.name();
    let letter = name.len() == 1 && name.as_bytes()[0].is_ascii_alphabetic();
    let editing = matches!(
        chord.key,
        Key::ArrowLeft
            | Key::ArrowRight
            | Key::ArrowUp
            | Key::ArrowDown
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
            | Key::Backspace
            | Key::Delete
            | Key::Enter
            | Key::Space
    );
    let typing = letter || typed(chord.key).is_some() || chord.key == Key::Space;
    // The composer's own keys may be the editor's: Enter, the arrows.
    let composer = action.group() == Group::Messages && !typing;
    if !modified && (typing || editing) && !composer {
        return Some(es("Sola escribe o mueve el cursor: súmale Ctrl o Alt."));
    }
    None
}

/// The other action `chord` is already `action`'s rival for, if any.
fn taken(prefs: &Prefs, action: Action, chord: Chord) -> Option<Action> {
    ACTIONS.into_iter().find(|other| {
        *other != action && binding(prefs, *other).is_some_and(|bound| bound.same(chord))
    })
}

fn conflict(other: Action) -> String {
    tr!("Ya lo usa «{action}».", action = other.label())
}

/// The key just recorded, `text` as the host named it, for the action
/// being recorded: refused with the reason on its row, or kept.
pub(crate) fn recorded(ui: &mut Ui, text: String) -> Vec<Intent> {
    let action = ui.settings_state.recording_for;
    ui.settings_state.key_notice = None;
    let parsed = Chord::parse(&text);
    let chord = match (action, parsed) {
        (_, Some(chord)) => chord,
        // A key limn has no name for still holds a voice shortcut.
        (Action::Voice(shortcut), None) => return voice(ui, shortcut, Some(text)),
        (_, None) => return Vec::new(),
    };
    let problem = refused(action, chord)
        .map(|reason| t(reason).to_owned())
        .or_else(|| taken(&ui.prefs, action, chord).map(conflict));
    if let Some(problem) = problem {
        ui.settings_state.key_notice = Some((action, problem));
        return Vec::new();
    }
    match action {
        Action::Voice(shortcut) => voice(ui, shortcut, Some(text)),
        action => {
            let chord = chord.normalized();
            ui.prefs.shortcuts.retain(|(chosen, _)| *chosen != action);
            if action.default_chord() != Some(chord) {
                ui.prefs.shortcuts.push((action, chord));
            }
            vec![save(&ui.prefs)]
        }
    }
}

/// Back to its default key, or none for a voice shortcut; refused while
/// another action has taken that key since.
pub(crate) fn reset(ui: &mut Ui, action: Action) -> Vec<Intent> {
    ui.settings_state.key_notice = None;
    if ui.settings_state.recording_for == action {
        ui.settings_state.recording = None;
    }
    if let Action::Voice(shortcut) = action {
        return voice(ui, shortcut, None);
    }
    if let Some(other) = action
        .default_chord()
        .and_then(|chord| taken(&ui.prefs, action, chord))
    {
        ui.settings_state.key_notice = Some((action, conflict(other)));
        return Vec::new();
    }
    ui.prefs.shortcuts.retain(|(chosen, _)| *chosen != action);
    vec![save(&ui.prefs)]
}

fn voice(ui: &mut Ui, shortcut: Shortcut, key: Option<String>) -> Vec<Intent> {
    let (setting, slot) = shortcut.slot(&mut ui.prefs);
    *slot = key.clone();
    vec![Intent::SaveSetting {
        key: setting.into(),
        value: key,
    }]
}

fn save(prefs: &Prefs) -> Intent {
    Intent::SaveSetting {
        key: SETTING.into(),
        value: kept(prefs),
    }
}

/// The shortcuts changed, as [`restore`] reads them; `None` when none is.
pub(crate) fn kept(prefs: &Prefs) -> Option<String> {
    (!prefs.shortcuts.is_empty()).then(|| {
        prefs
            .shortcuts
            .iter()
            .map(|(action, chord)| format!("{}={}", action.key(), chord.name()))
            .collect::<Vec<_>>()
            .join(";")
    })
}

/// Take back what [`kept`] kept: what does not read, is a voice shortcut,
/// repeats an action or is unfit for it is left out.
pub(crate) fn restore(kept: Option<String>) -> Vec<(Action, Chord)> {
    let mut shortcuts: Vec<(Action, Chord)> = Vec::new();
    for entry in kept.unwrap_or_default().split(';') {
        let Some((action, chord)) = entry.split_once('=').and_then(|(action, chord)| {
            Some((Action::from_key(action)?, Chord::parse(chord)?.normalized()))
        }) else {
            continue;
        };
        if action.dispatched()
            && refused(action, chord).is_none()
            && !shortcuts.iter().any(|(chosen, _)| *chosen == action)
        {
            shortcuts.push((action, chord));
        }
    }
    shortcuts
}

/// The composer's shortcuts, while it has the focus: formatting, editing
/// the last message, and the keys of sending that differ from limn's own,
/// which sends on Enter and breaks the line on Shift+Enter: a send or a
/// new line bound elsewhere, and Enter breaking the line once it no longer
/// sends.
pub(crate) fn composer(
    ui: &mut Ui,
    store: &Store,
    event: &UiEvent,
    focused: Option<&str>,
) -> Option<Vec<Intent>> {
    let channel = ui.channel()?;
    let UiEvent::Key {
        key,
        shift,
        ctrl,
        alt,
    } = *event
    else {
        return None;
    };
    if focused != Some(crate::editor_id(channel).as_str()) {
        return None;
    }
    let now = Instant::now();
    let action = pressed(&ui.prefs, event);
    if let Some(intents) =
        action.and_then(|action| crate::message_actions::format(ui, channel, action, now))
    {
        return Some(intents);
    }
    if action == Some(Action::EditLast) {
        return crate::messages::edit_last(ui, store, channel);
    }
    let moved = |action: Action| binding(&ui.prefs, action) != action.default_chord();
    if action == Some(Action::Send) && moved(Action::Send) {
        let draft = ui.drafts.get(&channel).cloned().unwrap_or_default();
        return Some(crate::messages::send_draft(ui, store, channel, &draft, now));
    }
    let enter = Chord::plain(Key::Enter).matches(key, ctrl, alt, shift);
    if (action == Some(Action::NewLine) && moved(Action::NewLine)) || (enter && moved(Action::Send))
    {
        return Some(crate::composer_selection::insert(
            ui, channel, "\n", false, now,
        ));
    }
    None
}

// Shortcuts that are not in the table: limn's, egui's and the lists' own.
/// What they do and their keys, alternatives separated by a space.
pub const FIXED: &[(&str, &str)] = &[
    (es("Cerrar, o cancelar la respuesta o la edición"), "Esc"),
    (es("Completar una @mención"), "Tab Enter"),
    (es("Elegir en las listas y sugerencias"), "↑ ↓"),
    (
        es("Desplazar el chat hacia arriba o hacia abajo"),
        "RePág AvPág",
    ),
    (es("Menú contextual"), "Shift+F10"),
    (es("Acercar o alejar la interfaz"), "Ctrl+= Ctrl+-"),
    (es("Volver al tamaño normal"), "Ctrl+0"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chord_reads_back_what_it_writes() {
        for action in ACTIONS {
            if let Some(chord) = action.default_chord() {
                assert_eq!(Chord::parse(&chord.name()), Some(chord), "{action:?}");
            }
        }
        assert_eq!(
            Chord::parse("Ctrl+Shift+T"),
            Some(Chord::ctrl_shift(Key::T))
        );
        assert_eq!(Chord::parse("F8"), Some(Chord::plain(Key::F8)));
        assert_eq!(Chord::parse("Ctrl+/"), Some(Chord::ctrl(Key::Slash)));
        assert_eq!(Chord::parse("Meta+K"), None);
        assert_eq!(Chord::parse("Ctrl+"), None);
        assert_eq!(Chord::ctrl(Key::Slash).label(), "Ctrl+/");
        assert_eq!(Chord::alt(Key::ArrowLeft).label(), "Alt+←");
    }

    #[test]
    fn shift_does_not_keep_a_typed_character_from_matching() {
        let slash = Chord::ctrl(Key::Slash);
        // Ctrl+Shift+7 on a Spanish layout: egui names the key it typed.
        assert!(slash.matches(Key::Slash, true, false, true));
        assert!(slash.matches(Key::Slash, true, false, false));
        assert!(!slash.matches(Key::Slash, false, false, true));
        assert!(!slash.matches(Key::Slash, true, true, false));
        // Shift still counts for the other keys.
        assert!(!Chord::plain(Key::F6).matches(Key::F6, false, false, true));
        assert!(Chord::shift(Key::F6).matches(Key::F6, false, false, true));
    }

    #[test]
    fn every_default_is_unique_and_fit() {
        for action in ACTIONS {
            let Some(chord) = action.default_chord() else {
                continue;
            };
            assert_eq!(refused(action, chord), None, "{action:?}");
            assert_eq!(taken(&Prefs::default(), action, chord), None, "{action:?}");
        }
    }

    #[test]
    fn the_keys_the_fields_need_are_refused() {
        let back = Action::Back;
        assert!(refused(back, Chord::plain(Key::K)).is_some());
        assert!(refused(back, Chord::shift(Key::K)).is_some());
        assert!(refused(back, Chord::plain(Key::ArrowLeft)).is_some());
        assert!(refused(back, Chord::plain(Key::Tab)).is_some());
        assert!(refused(back, Chord::ctrl(Key::V)).is_some());
        assert!(refused(back, Chord::shift(Key::F10)).is_some());
        assert_eq!(refused(back, Chord::ctrl(Key::B)), None);
        assert_eq!(refused(back, Chord::plain(Key::F9)), None);
        assert_eq!(refused(Action::Send, Chord::ctrl(Key::Enter)), None);
        assert!(refused(Action::Send, Chord::plain(Key::S)).is_some());
        // A voice key is held: a letter is as good as any.
        assert_eq!(
            refused(Action::Voice(Shortcut::PushToTalk), Chord::plain(Key::V)),
            None
        );
    }

    #[test]
    fn what_is_kept_comes_back_and_nonsense_does_not() {
        let prefs = Prefs {
            shortcuts: vec![
                (Action::Back, Chord::ctrl(Key::J)),
                (Action::Send, Chord::ctrl(Key::Enter)),
            ],
            ..Prefs::default()
        };
        assert_eq!(restore(kept(&prefs)), prefs.shortcuts);
        assert_eq!(kept(&Prefs::default()), None);
        let restored = restore(Some(
            "back=Ctrl+J;back=Ctrl+L;nada=F1;mute=F8;switcher=K;forward=Alt+Shift+Slash;bad".into(),
        ));
        assert_eq!(
            restored,
            [
                (Action::Back, Chord::ctrl(Key::J)),
                (Action::Forward, Chord::alt(Key::Slash)),
            ]
        );
    }
}
