//! The voice message, audio or video file being played: one at a time, in
//! the open conversation. The host fetches its file and plays it
//! ([`Intent::PlayVoice`]); this layer keeps what its player shows and what
//! the account asked of it, where each clip was left, the speed of voice
//! messages and the volume.

use limn::{Component, Element, ImageViewer, Key, UiAction, UiEvent, UiTree};
use rune_components::message::{
    attachment_ids, media_controls, media_surface, playable, Playable, VoiceNow, VOICE_SPEEDS,
};
use rune_components::Look;
use rune_model::{Attachment, AttachmentId, ChannelId, Message, MessageId, UserId};
use rune_state::Store;

use crate::{timeline_id, Intent, Ui, ViewContext};

/// What is played, which picks the sample a simulated one plays.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Played {
    Voice,
    Audio,
    Video,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VoiceState {
    /// Its audio is on its way.
    Loading,
    Playing,
    Paused,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct VoicePlayer {
    pub account: Option<UserId>,
    pub channel: ChannelId,
    pub message: MessageId,
    pub attachment: AttachmentId,
    /// Ties the host's audio to this player.
    pub request: u64,
    pub state: VoiceState,
    /// Seconds into the clip; the host moves it while it plays.
    pub position: f32,
    /// A jump the host has not made yet, in seconds.
    pub seek: Option<f32>,
    pub kind: Played,
    /// How long it lasts, once the host read its file.
    pub duration: Option<f32>,
    /// Its sound's gain, from 0 to 1.
    pub volume: f32,
    /// The host presented a picture of its video on its surface.
    pub picture: bool,
    /// Its video fills the window.
    pub fullscreen: bool,
}

impl VoicePlayer {
    pub(crate) fn now(&self) -> VoiceNow<'_> {
        VoiceNow {
            attachment: self.attachment,
            loading: self.state == VoiceState::Loading,
            playing: self.state == VoiceState::Playing,
            position: self.position,
            error: match &self.state {
                VoiceState::Failed(error) => Some(error),
                _ => None,
            },
            duration: self.duration,
            volume: self.volume,
            picture: self.picture,
        }
    }
}

/// Clips whose place is remembered; the oldest is forgotten first.
const REMEMBERED: usize = 64;
const CLOSE: &str = "media-viewer:close";

/// Where the account left each clip, the speed voice messages play at and
/// the volume of the rest.
#[derive(Debug)]
pub(crate) struct VoiceMemory {
    account: Option<UserId>,
    /// Seconds into each clip, the latest last.
    positions: Vec<(AttachmentId, f32)>,
    /// An index into [`VOICE_SPEEDS`].
    pub(crate) speed: usize,
    /// The volume of audio and video files, and the one before a mute.
    volume: f32,
    unmuted: f32,
}

impl Default for VoiceMemory {
    fn default() -> Self {
        Self {
            account: None,
            positions: Vec::new(),
            speed: 0,
            volume: 1.0,
            unmuted: 1.0,
        }
    }
}

impl VoiceMemory {
    /// Another account's places are not this one's.
    fn sync(&mut self, account: Option<UserId>) {
        if self.account != account {
            self.account = account;
            self.positions.clear();
        }
    }

    fn remember(&mut self, player: &VoicePlayer) {
        self.sync(player.account);
        self.positions.retain(|(id, _)| *id != player.attachment);
        // At the start or played to the end (the host rewinds it): nothing to keep.
        if player.position > 0.0 {
            if self.positions.len() >= REMEMBERED {
                self.positions.remove(0);
            }
            self.positions.push((player.attachment, player.position));
        }
    }

    /// Every clip left partway, and where, in seconds.
    pub(crate) fn left(&self) -> &[(AttachmentId, f32)] {
        &self.positions
    }

    pub(crate) fn recall(&self, attachment: AttachmentId) -> f32 {
        self.positions
            .iter()
            .find(|(id, _)| *id == attachment)
            .map_or(0.0, |(_, position)| *position)
    }
}

/// How fast voice messages play, 1 for as recorded.
pub fn voice_speed(ui: &Ui) -> f32 {
    VOICE_SPEEDS[ui.voice_memory.speed % VOICE_SPEEDS.len()]
}

/// What `attachment` plays as, when RUNE plays it.
fn kind(attachment: &Attachment) -> Option<Played> {
    if attachment.voice.is_some() {
        return Some(Played::Voice);
    }
    match playable(attachment)? {
        Playable::Audio => Some(Played::Audio),
        Playable::Video => Some(Played::Video),
        Playable::Not(_) => None,
    }
}

fn message(store: &Store, channel: ChannelId, message: MessageId) -> Option<&Message> {
    store
        .channel(channel)
        .filter(|channel| Store::visible(channel))?;
    store.timeline(channel)?.get(message).map(|found| &**found)
}

fn clip<'a>(
    ui: &Ui,
    store: &'a Store,
    channel: ChannelId,
    id: MessageId,
    attachment: AttachmentId,
) -> Option<(&'a Attachment, Played)> {
    let found = message(store, channel, id)?
        .files()
        .find(|found| found.id == attachment)?;
    // A spoiler plays only once revealed.
    (!crate::attachments::hidden(ui, found)).then_some(())?;
    Some((found, kind(found)?))
}

/// Stop the clip playing, if any, remembering where it was.
pub(crate) fn stop(ui: &mut Ui) -> Vec<Intent> {
    ui.voice_player
        .take()
        .map(|player| {
            ui.voice_memory.remember(&player);
            vec![Intent::StopVoice {
                request: player.request,
            }]
        })
        .unwrap_or_default()
}

/// Leaving its conversation or account, or losing the message, stops it.
/// Another account, or none, forgets where clips were left.
pub(crate) fn refresh(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let gone = ui.voice_player.as_ref().is_some_and(|player| {
        player.account != store.me()
            || ui.channel() != Some(player.channel)
            || clip(ui, store, player.channel, player.message, player.attachment).is_none()
    });
    let intents = if gone { stop(ui) } else { Vec::new() };
    ui.voice_memory.sync(store.me());
    intents
}

/// A video scrolled out of view pauses, unless it fills the window.
pub(crate) fn after_layout(ui: &mut Ui, store: &Store, tree: &UiTree) {
    let Some(player) = ui.voice_player.as_mut().filter(|player| {
        player.kind == Played::Video && player.state == VoiceState::Playing && !player.fullscreen
    }) else {
        return;
    };
    let shown = store.timeline(player.channel).is_some_and(|timeline| {
        tree.visible_rows(&timeline_id(player.channel))
            .is_some_and(|rows| {
                timeline
                    .messages()
                    .iter()
                    .take(rows.end)
                    .skip(rows.start)
                    .any(|found| found.id == player.message)
            })
    });
    if !shown {
        player.state = VoiceState::Paused;
    }
}

/// The host has the audio of `request`, or could not get it.
pub fn voice_loaded(ui: &mut Ui, request: u64, result: Result<(), String>) {
    if let Some(player) = ui
        .voice_player
        .as_mut()
        .filter(|player| player.request == request && player.state == VoiceState::Loading)
    {
        player.state = match result {
            Ok(()) => VoiceState::Playing,
            Err(error) => VoiceState::Failed(error),
        };
    }
}

pub(crate) fn update(ui: &mut Ui, store: &Store, action: &UiAction) -> Option<Vec<Intent>> {
    let (id, slid) = match action {
        UiAction::Clicked { id } => (id, None),
        UiAction::SliderChanged {
            id,
            fraction,
            committed,
        } => (id, Some((*fraction, *committed))),
        _ => return None,
    };
    if id == CLOSE {
        if let Some(player) = ui.voice_player.as_mut() {
            player.fullscreen = false;
        }
        return Some(Vec::new());
    }
    // The viewer's zoom buttons: it zooms itself.
    if id.starts_with(CLOSE) {
        return Some(Vec::new());
    }
    let (message, attachment, verb) = attachment_ids::parse(id)?;
    if !matches!(
        (verb, slid),
        ("voice" | "play" | "speed" | "mute" | "fullscreen", None) | ("seek" | "volume", Some(_))
    ) {
        return None;
    }
    let Some(channel) = ui.channel() else {
        return Some(Vec::new());
    };
    let Some((found, kind)) = clip(ui, store, channel, message, attachment) else {
        return Some(Vec::new());
    };
    let current =
        |player: &VoicePlayer| player.message == message && player.attachment == attachment;
    match verb {
        "speed" => {
            let speed = &mut ui.voice_memory.speed;
            *speed = (*speed + 1) % VOICE_SPEEDS.len();
            return Some(Vec::new());
        }
        "volume" | "mute" => {
            let memory = &mut ui.voice_memory;
            let volume = match slid {
                Some((fraction, _)) => fraction.clamp(0.0, 1.0),
                None if memory.volume > 0.0 => 0.0,
                None => memory.unmuted.max(0.1),
            };
            if memory.volume > 0.0 {
                memory.unmuted = memory.volume;
            }
            memory.volume = volume;
            if let Some(player) = ui.voice_player.as_mut().filter(|player| current(player)) {
                player.volume = volume;
            }
            return Some(Vec::new());
        }
        "fullscreen" => {
            let mut intents = Vec::new();
            if let Some(player) = ui.voice_player.as_mut().filter(|player| current(player)) {
                player.fullscreen = !player.fullscreen;
                intents.push(Intent::Focus(if player.fullscreen {
                    CLOSE.into()
                } else {
                    attachment_ids::action(message, attachment, "fullscreen")
                }));
            }
            return Some(intents);
        }
        _ => {}
    }
    let announced = found.voice.as_ref().map(|clip| clip.duration_secs);
    ui.voice_memory.sync(store.me());
    let mut position = ui.voice_memory.recall(attachment);
    if let Some(player) = ui.voice_player.as_mut().filter(|player| current(player)) {
        match slid {
            Some((fraction, committed)) => {
                if let Some(duration) = player.duration.or(announced) {
                    player.position = fraction.clamp(0.0, 1.0) * duration;
                    if committed {
                        player.seek = Some(player.position);
                    }
                }
                return Some(Vec::new());
            }
            None => match player.state {
                VoiceState::Playing => player.state = VoiceState::Paused,
                VoiceState::Paused => player.state = VoiceState::Playing,
                VoiceState::Loading => {}
                // Fetched again: its link may have been the trouble.
                VoiceState::Failed(_) => position = player.position,
            },
        }
        if !matches!(player.state, VoiceState::Failed(_)) {
            return Some(Vec::new());
        }
    } else if let Some((fraction, committed)) = slid {
        // Dragging a clip not started yet starts it where it lands, when
        // its length is known before its file is.
        let Some(duration) = announced.filter(|_| committed) else {
            return Some(Vec::new());
        };
        position = fraction.clamp(0.0, 1.0) * duration;
    }
    let mut intents = stop(ui);
    let request = ui.fresh_request();
    ui.voice_player = Some(VoicePlayer {
        account: store.me(),
        channel,
        message,
        attachment,
        request,
        state: VoiceState::Loading,
        position,
        seek: None,
        kind,
        duration: announced,
        volume: ui.voice_memory.volume,
        picture: false,
        fullscreen: false,
    });
    intents.push(Intent::PlayVoice {
        request,
        source: found.download.clone(),
        size: found.size_bytes,
        kind,
    });
    Some(intents)
}

/// How far Left and Right seek a video in its viewer, in seconds.
const SEEK_STEP: f32 = 5.0;

/// Esc leaves the full window; the viewer's own keys stay with it.
pub(crate) fn intercept(ui: &mut Ui, event: &UiEvent) -> Option<Vec<Intent>> {
    let player = ui
        .voice_player
        .as_mut()
        .filter(|player| player.fullscreen)?;
    let UiEvent::Key { key, ctrl, .. } = event else {
        return None;
    };
    match key {
        // Back or on five seconds, as a video player does.
        Key::ArrowLeft | Key::ArrowRight if !ctrl => {
            let by = if *key == Key::ArrowLeft {
                -SEEK_STEP
            } else {
                SEEK_STEP
            };
            let end = player.duration.unwrap_or(f32::MAX);
            player.position = (player.position + by).clamp(0.0, end);
            player.seek = Some(player.position);
            Some(Vec::new())
        }
        // The viewer's own keys zoom and move the picture (limn's zoom view).
        Key::Plus | Key::Equals | Key::Minus | Key::Num0 | Key::Num1 | Key::Num2 | Key::Num3 => {
            None
        }
        Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight if *ctrl => None,
        Key::Escape => {
            player.fullscreen = false;
            Some(vec![Intent::Focus(attachment_ids::action(
                player.message,
                player.attachment,
                "fullscreen",
            ))])
        }
        Key::Space => {
            player.state = match player.state {
                VoiceState::Playing => VoiceState::Paused,
                VoiceState::Paused => VoiceState::Playing,
                ref other => other.clone(),
            };
            Some(Vec::new())
        }
        Key::Tab | Key::Enter => None,
        _ => Some(Vec::new()),
    }
}

/// A video over the whole window, with its controls under it.
pub(crate) fn overlay(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let Some(player) = ui.voice_player.as_ref().filter(|player| player.fullscreen) else {
        return root;
    };
    let Some((found, _)) = clip(ui, store, player.channel, player.message, player.attachment)
    else {
        return root;
    };
    // The viewer's margins and padding, around the controls.
    let inner = (context.window.width - 40.0).max(240.0);
    let mut details = Vec::new();
    if let Some((width, height)) = found.dimensions {
        details.push(format!("{width} × {height}"));
    }
    details.push(rune_components::file_size(found.size_bytes));
    let viewer = ImageViewer::new(root, true, &found.filename, CLOSE, look.menu_palette())
        .viewport(context.window)
        .subtitle(details.join(" · "))
        .surface(media_surface(player.attachment), &found.filename)
        .footer(media_controls(
            found,
            player.message,
            Some(player.now()),
            true,
            true,
            inner,
            look,
        ))
        .initial_focus(attachment_ids::action(
            player.message,
            player.attachment,
            "play",
        ))
        .return_focus(attachment_ids::action(
            player.message,
            player.attachment,
            "fullscreen",
        ));
    // Without its size it fills the window at its proportions, as the full
    // window asked for: a small video is not left small.
    viewer.render()
}
