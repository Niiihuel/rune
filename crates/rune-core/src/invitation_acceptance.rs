//! One bounded invitation write, independent of the preview dialog's lifetime.

use std::time::{Duration, Instant};

use rune_provider::InvitationDestination;
use rune_provider::{AuthenticationState, Command, Event, Failure, InvitationJoinError, RequestId};

const TIMEOUT: Duration = Duration::from_secs(60);
/// A person solving a challenge, then the one write it allows.
const VERIFY_TIMEOUT: Duration = Duration::from_secs(360);

/// The longest an invitation command may stay pending; `None` for others.
pub fn limit(command: &Command) -> Option<Duration> {
    match command {
        Command::AcceptInvitation { .. } => Some(TIMEOUT),
        Command::VerifyInvitation { .. } => Some(VERIFY_TIMEOUT),
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct PendingAcceptance(Option<Pending>);

struct Pending {
    request: RequestId,
    destination: InvitationDestination,
    deadline: Instant,
    expired: bool,
}

impl PendingAcceptance {
    pub fn clear(&mut self) {
        self.0 = None;
    }

    pub fn begin(
        &mut self,
        command: &Command,
        supported: bool,
        now: Instant,
    ) -> Result<bool, InvitationJoinError> {
        let (Command::AcceptInvitation {
            request,
            destination,
            expires_at,
            ..
        }
        | Command::VerifyInvitation {
            request,
            destination,
            expires_at,
            ..
        }) = command
        else {
            return Ok(true);
        };
        // A duplicated delivery of the same command is not a new outcome.
        // Reporting a rejection with its ID would finish the original in the UI.
        if self.0.as_ref().is_some_and(|pending| {
            pending.request == *request && pending.destination == *destination
        }) {
            return Ok(false);
        }
        let error = if !supported {
            Some(InvitationJoinError::Unsupported)
        } else if *expires_at <= now || self.0.is_some() {
            Some(InvitationJoinError::Unavailable)
        } else {
            None
        };
        if let Some(error) = error {
            return Err(error);
        }
        self.0 = Some(Pending {
            request: *request,
            destination: *destination,
            deadline: (*expires_at).min(now + limit(command).unwrap_or(TIMEOUT)),
            expired: false,
        });
        Ok(true)
    }

    /// A completion never creates membership and must belong to this request.
    pub fn observe(&mut self, event: &Event) -> bool {
        let (request, destination) = match event {
            Event::InvitationAcceptance {
                request,
                destination,
                ..
            }
            | Event::Failed {
                failure:
                    Failure::Invitation {
                        request,
                        destination,
                    },
                ..
            } => (*request, *destination),
            Event::Authentication(
                AuthenticationState::SignedOut | AuthenticationState::Failed { .. },
            ) => {
                self.clear();
                return true;
            }
            _ => return true,
        };
        if self
            .0
            .as_ref()
            .is_some_and(|pending| pending.request == request && pending.destination == destination)
        {
            self.clear();
            true
        } else {
            false
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.0
            .as_ref()
            .filter(|pending| !pending.expired)
            .map(|pending| pending.deadline)
    }

    pub fn expire(&mut self, now: Instant) -> Option<Event> {
        let pending = self
            .0
            .as_mut()
            .filter(|pending| !pending.expired && now >= pending.deadline)?;
        // Timeout is not proof that a write did not happen. Keep the gate
        // until the provider resolves it or the session is discarded.
        pending.expired = true;
        Some(Event::Failed {
            failure: Failure::Invitation {
                request: pending.request,
                destination: pending.destination,
            },
            reason: InvitationJoinError::Ambiguous.label().into(),
            ambiguous: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{ServerId, UserId};
    use rune_provider::InvitationTarget;

    fn join(request: RequestId) -> Command {
        Command::AcceptInvitation {
            request,
            destination: InvitationDestination::Server(ServerId(7)),
            account: UserId(1),
            expires_at: Instant::now() + TIMEOUT,
            target: InvitationTarget {
                key: "fixture".into(),
                url: "https://example.test/invite/fixture".into(),
            },
        }
    }

    fn accepted(request: RequestId) -> Event {
        Event::InvitationAcceptance {
            request,
            destination: InvitationDestination::Server(ServerId(7)),
            result: Ok(()),
        }
    }

    #[test]
    fn timeout_reports_once_without_releasing_or_resending_the_write() {
        let now = Instant::now();
        let mut pending = PendingAcceptance::default();
        pending.begin(&join(1), true, now).unwrap();
        assert!(!pending.begin(&join(1), true, now).unwrap());
        assert_eq!(pending.deadline(), Some(now + TIMEOUT));
        assert!(pending
            .expire(now + TIMEOUT - Duration::from_millis(1))
            .is_none());
        assert!(matches!(
            pending.expire(now + TIMEOUT),
            Some(Event::Failed {
                failure: Failure::Invitation { .. },
                ambiguous: true,
                ..
            })
        ));
        assert!(pending.expire(now + TIMEOUT).is_none());
        assert!(pending.deadline().is_none());
        assert!(pending.begin(&join(2), true, now).is_err());
        assert!(!pending.observe(&accepted(2)));
        assert!(pending.observe(&accepted(1)));
        assert!(pending.begin(&join(2), true, now).is_ok());
    }

    #[test]
    fn a_verification_has_time_to_be_solved_and_shares_the_single_gate() {
        let now = Instant::now();
        let mut pending = PendingAcceptance::default();
        let verify = Command::VerifyInvitation {
            request: 1,
            destination: InvitationDestination::Server(ServerId(7)),
            account: UserId(1),
            expires_at: now + Duration::from_secs(3600),
        };
        assert_eq!(
            pending.begin(&verify, false, now),
            Err(InvitationJoinError::Unsupported)
        );
        assert!(pending.begin(&verify, true, now).unwrap());
        assert_eq!(pending.deadline(), Some(now + VERIFY_TIMEOUT));
        assert!(pending.begin(&join(2), true, now).is_err());
        assert!(pending.observe(&accepted(1)));
        assert!(pending.begin(&join(2), true, now).unwrap());
    }

    #[test]
    fn unsupported_sessions_and_stale_replies_do_not_open_the_gate() {
        let now = Instant::now();
        let mut pending = PendingAcceptance::default();
        assert_eq!(
            pending.begin(&join(1), false, now),
            Err(InvitationJoinError::Unsupported)
        );
        pending.begin(&join(2), true, now).unwrap();
        assert!(!pending.observe(&accepted(1)));
        assert!(pending.begin(&join(3), true, now).is_err());
        pending.observe(&Event::Authentication(AuthenticationState::SignedOut));
        assert!(!pending.observe(&accepted(2)));
        pending.begin(&join(3), true, now).unwrap();
        assert!(pending.observe(&Event::Failed {
            failure: Failure::Invitation {
                request: 3,
                destination: InvitationDestination::Server(ServerId(7))
            },
            reason: "queue full".into(),
            ambiguous: false
        }));
        assert!(pending.deadline().is_none());
    }
}
