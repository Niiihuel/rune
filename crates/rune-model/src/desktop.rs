//! What this computer says the person is doing (P6): another program's Rich
//! Presence or a game recognized by its process, to publish as the
//! account's activity; and the games a provider can recognize.

use serde::{Deserialize, Serialize};

use crate::ActivityKind;

/// An activity of this device to publish. Text is already bounded and
/// without control characters; the provider decides which images it can
/// publish and names an application it knows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalActivity {
    /// The application it belongs to, when there is one.
    pub application: Option<u64>,
    pub kind: LocalActivityKind,
    /// The name to show; `None` lets the provider name the application.
    pub name: Option<String>,
    pub details: Option<String>,
    pub state: Option<String>,
    /// Unix milliseconds.
    pub start: Option<i64>,
    pub end: Option<i64>,
    /// An application's asset key, an `mp:` path or an `https` address.
    pub large_image: Option<String>,
    pub large_text: Option<String>,
    pub small_image: Option<String>,
    pub small_text: Option<String>,
    /// Where it comes from, for the interface.
    pub source: ActivitySource,
}

impl LocalActivity {
    /// How it shows in a presence, under `name`; its pictures stay the
    /// provider's to resolve.
    pub fn shown(&self, name: String) -> crate::Activity {
        crate::Activity {
            kind: self.kind.activity_kind(),
            name,
            detail: self.details.clone(),
            state: self.state.clone(),
            image: None,
            small_image: None,
            started: self.start.map(crate::Timestamp),
            ends: self.end.map(crate::Timestamp),
            application: self.application,
            large_text: self.large_text.clone(),
            small_text: self.small_text.clone(),
            ..Default::default()
        }
    }
}

/// What a program may say it is doing: not streaming, which is the
/// provider's own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalActivityKind {
    #[default]
    Playing,
    Listening,
    Watching,
    Competing,
}

impl LocalActivityKind {
    /// Discord's activity type numbers, which the local protocol uses too.
    pub fn from_number(number: u8) -> Option<Self> {
        Some(match number {
            0 => Self::Playing,
            2 => Self::Listening,
            3 => Self::Watching,
            5 => Self::Competing,
            _ => return None,
        })
    }

    pub fn number(self) -> u8 {
        match self {
            Self::Playing => 0,
            Self::Listening => 2,
            Self::Watching => 3,
            Self::Competing => 5,
        }
    }

    /// How the interface shows it.
    pub fn activity_kind(self) -> ActivityKind {
        match self {
            Self::Playing => ActivityKind::Playing,
            Self::Listening => ActivityKind::Listening,
            Self::Watching => ActivityKind::Watching,
            Self::Competing => ActivityKind::Competing,
        }
    }
}

/// Whence a [`LocalActivity`] came.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivitySource {
    /// A program on this computer, through the local Rich Presence socket.
    #[default]
    Program,
    /// A game recognized by its process.
    Game,
}

/// A game a provider can recognize by its program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectableGame {
    pub application: Option<u64>,
    pub name: String,
    /// Executable names or path tails, such as `game.exe` or `bin/game`.
    pub executables: Vec<String>,
}

// Desktop session and games (escritorio-sesion-y-juegos).
/// Whether others see the activity this device published, as the
/// account's sessions say it (Discord's `SESSIONS_REPLACE`, as Serein reads
/// it). Never proof that every person sees it: their privacy decides too.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityVisibility {
    /// Nothing says it yet.
    Unconfirmed,
    /// This session has it; whether the account lists it is not known.
    Received,
    /// The account lists it for others.
    Listed,
    /// The account has it hidden: its activity sharing is off.
    Hidden,
    /// The account lists other activities, not this one.
    Missing,
}
