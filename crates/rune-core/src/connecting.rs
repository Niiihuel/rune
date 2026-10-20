//! Reads asked before the provider's session is up (arranque-sin-avisos).
//! RUNE opens from its local store, and the interface asks at once for what
//! it shows: the open conversation's history, the server's events, a
//! profile. A session still connecting would refuse each, and each refusal
//! would be a notice about something that loads a moment later. They wait
//! here instead, and go out once the session is up, after a reconnection
//! too. Only a session that gave up fails them.

use rune_provider::{AuthenticationState, Command, ConnectionState, Event};

/// At most this many reads wait; more go to the provider, which refuses them.
const HELD: usize = 64;

#[derive(Default)]
pub(crate) struct Connecting {
    /// Since the session's `Ready` or `Connected`, until it connects again.
    connected: bool,
    held: Vec<Command>,
}

impl Connecting {
    /// Keep `command` while the session is not up, if it is a read that may
    /// wait; otherwise it is given back, to go out now.
    pub(crate) fn hold(&mut self, command: Command) -> Option<Command> {
        if self.connected || !waits(&command) || self.held.len() >= HELD {
            return Some(command);
        }
        self.held.push(command);
        None
    }

    /// What `event` says of the session. The reads held come back once it
    /// is up, to go out, or once it gave up, to fail.
    pub(crate) fn observe(&mut self, event: &Event) -> Vec<Command> {
        match event {
            Event::Ready(_) | Event::Connection(ConnectionState::Connected) => {
                self.connected = true;
                std::mem::take(&mut self.held)
            }
            Event::Connection(ConnectionState::Disconnected { .. }) => {
                self.connected = false;
                std::mem::take(&mut self.held)
            }
            Event::Connection(_) => {
                self.connected = false;
                Vec::new()
            }
            // Another account asks nothing of this one's.
            Event::Authentication(
                AuthenticationState::SignedOut | AuthenticationState::Failed { .. },
            ) => {
                self.connected = false;
                self.held.clear();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    pub(crate) fn connected(&self) -> bool {
        self.connected
    }

    /// Signing out: what waited for this session is not asked.
    pub(crate) fn clear(&mut self) {
        self.held.clear();
    }
}

/// A read the interface may ask before the session is up. Pins and a
/// message's context are not: their own views say they could not load.
fn waits(command: &Command) -> bool {
    matches!(
        command,
        Command::LoadHistory { .. }
            | Command::LoadThreads { .. }
            | Command::LoadScheduledEvents { .. }
            | Command::LoadEventInterested { .. }
            | Command::LoadMembers { .. }
            | Command::LoadProfile { .. }
            | Command::LoadNote { .. }
            | Command::LoadServerProfile { .. }
            | Command::LoadServerRules { .. }
            | Command::LoadOnboarding { .. }
            | Command::LoadChannel { .. }
            | Command::LoadReactions { .. }
            | Command::LoadPollVoters { .. }
            | Command::LoadRoleCounts { .. }
            | Command::LoadAutoModRules { .. }
            | Command::LoadBans { .. }
            | Command::LoadInvites { .. }
            | Command::LoadAudit { .. }
            | Command::LoadWebhooks { .. }
            | Command::LoadIntegrations { .. }
            | Command::LoadAppCommands { .. }
            | Command::LoadStreamPreview { .. }
            | Command::LoadGifFavorites
            | Command::LoadStickerPacks
            | Command::LoadAccountSettings
            | Command::LoadDetectableGames
    )
}
