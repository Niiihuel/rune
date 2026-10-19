//! The hCaptcha that stopped one write, while the person may solve it (the
//! owner's decision of 2026-10-17, docs/SECURITY.md): offered with
//! [`Event::ChallengeRequired`], shown in the host's window only when the
//! person asks, and answered once, with that same write. Never solved here
//! and never sent elsewhere. Invitations keep their own flow.

use rune_i18n::es;
use std::{sync::Arc, time::Duration};

use rune_provider::{BoxFuture, Command, Event, Failure, RequestId, CHALLENGE_LIFETIME};
use tokio::time::Instant;
use zeroize::Zeroizing;

use crate::{
    http::{Answer, Issued, Solved},
    ChallengeAnswer, LoginError, SessionLogin,
};

pub(crate) const CANCELLED: LoginError =
    LoginError::new(es("Cancelaste la verificación; no se envió nada."));
pub(crate) const EXPIRED: LoginError =
    LoginError::new(es("Venció el plazo de la verificación; no se envió nada."));
pub(crate) const OFFLINE: LoginError = LoginError::new(es(
    "Se perdió la conexión durante la verificación; no se envió nada.",
));
pub(crate) const AGAIN: LoginError = LoginError::new(es(
    "Discord volvió a pedir una verificación; la acción no se repite otra vez.",
));
pub(crate) const BUSY: LoginError = LoginError::new(es(
    "Discord pidió una verificación mientras otra esperaba; volvé a intentarlo después.",
));

/// One write stopped by a challenge, and how far its answer got.
pub(crate) struct Waiting {
    pub id: RequestId,
    pub command: Command,
    pub until: Instant,
    /// An upload's time left when it stopped, given back to its retry.
    pub left: Duration,
    pub stage: Stage,
}

pub(crate) enum Stage {
    /// Offered to the person; nothing shown yet.
    Offered(Box<Issued>),
    /// The host's window shows it.
    Shown {
        route: String,
        rqtoken: Option<Zeroizing<String>>,
        session: Option<Zeroizing<String>>,
        future: BoxFuture<'static, Result<ChallengeAnswer, LoginError>>,
    },
    /// Solved: the write goes out once more, carrying the answer.
    Solved(Answer),
}

/// At most one write waits on a person at a time.
#[derive(Default)]
pub(crate) struct Challenges {
    next: RequestId,
    pub waiting: Option<Waiting>,
}

impl Challenges {
    /// Hold `command`, stopped by `issued`, and the event offering it; `None`
    /// when nobody could wait on it or another one already waits.
    pub fn offer(
        &mut self,
        command: &Command,
        issued: Box<Issued>,
        left: Duration,
    ) -> Option<Event> {
        let failure = Failure::of(command)?;
        if self.waiting.is_some() {
            return None;
        }
        self.next += 1;
        self.waiting = Some(Waiting {
            id: self.next,
            command: command.clone(),
            until: Instant::now() + CHALLENGE_LIFETIME,
            left,
            stage: Stage::Offered(issued),
        });
        Some(Event::ChallengeRequired {
            challenge: self.next,
            failure,
        })
    }

    /// The person's answer to the offer `id`: show it in `login`'s window,
    /// or give it up. The write given up, with why, if any.
    pub fn answer(
        &mut self,
        id: RequestId,
        solve: bool,
        login: &Arc<dyn SessionLogin>,
    ) -> Option<(Command, LoginError)> {
        let found = self
            .waiting
            .take_if(|waiting| waiting.id == id && matches!(waiting.stage, Stage::Offered(_)))?;
        let Waiting {
            id,
            command,
            until,
            left,
            stage: Stage::Offered(issued),
        } = found
        else {
            unreachable!("only an offered challenge is answered")
        };
        if !solve {
            return Some((command, CANCELLED));
        }
        let Issued {
            challenge,
            rqtoken,
            session,
            route,
        } = *issued;
        let login = login.clone();
        self.waiting = Some(Waiting {
            id,
            command,
            until,
            left,
            stage: Stage::Shown {
                route,
                rqtoken,
                session,
                future: Box::pin(async move {
                    tokio::time::timeout_at(until, login.verify(challenge))
                        .await
                        .unwrap_or(Err(EXPIRED))
                }),
            },
        });
        None
    }

    /// Wait for the window's answer; never resolves while none is shown.
    pub async fn shown(&mut self) -> Result<ChallengeAnswer, LoginError> {
        match self.waiting.as_mut().map(|waiting| &mut waiting.stage) {
            Some(Stage::Shown { future, .. }) => future.await,
            _ => std::future::pending().await,
        }
    }

    /// Keep the window's `answer`; a closed window gives the write up.
    pub fn solved(
        &mut self,
        answer: Result<ChallengeAnswer, LoginError>,
    ) -> Option<(Command, LoginError)> {
        let waiting = self.waiting.as_mut()?;
        let Stage::Shown {
            route,
            rqtoken,
            session,
            ..
        } = &mut waiting.stage
        else {
            return None;
        };
        match answer {
            Ok(answer) => {
                waiting.stage = Stage::Solved(Answer {
                    route: std::mem::take(route),
                    solved: Solved {
                        answer,
                        rqtoken: rqtoken.take(),
                        session: session.take(),
                    },
                });
                None
            }
            Err(reason) => {
                let reason = if reason == EXPIRED {
                    EXPIRED
                } else {
                    LoginError::new(es("No se completó la verificación; no se envió nada."))
                };
                self.waiting.take().map(|waiting| (waiting.command, reason))
            }
        }
    }

    /// When the offer not yet taken lapses; `None` when none waits so.
    pub fn deadline(&self) -> Option<Instant> {
        self.waiting
            .as_ref()
            .filter(|waiting| matches!(waiting.stage, Stage::Offered(_)))
            .map(|waiting| waiting.until)
    }

    /// The solved write, to send once more.
    pub fn take_solved(&mut self) -> Option<(Command, Answer, Duration)> {
        let waiting = self
            .waiting
            .take_if(|waiting| matches!(waiting.stage, Stage::Solved(_)))?;
        let Stage::Solved(answer) = waiting.stage else {
            unreachable!("checked above")
        };
        Some((waiting.command, answer, waiting.left))
    }

    /// Whether the host's window shows one now.
    pub fn showing(&self) -> bool {
        self.waiting
            .as_ref()
            .is_some_and(|waiting| matches!(waiting.stage, Stage::Shown { .. }))
    }
}
