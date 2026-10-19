//! A simulated verification, so the question and its answers can be tried
//! without a service: a friend request to [`CHALLENGED`] waits for
//! [`Command::SolveChallenge`]. No window opens; solving runs the request
//! once, cancelling sends nothing.

use rune_i18n::t;

use super::Session;
use crate::{Command, Event, Failure};

/// The person whose friend request asks for a simulated verification.
pub const CHALLENGED: &str = "pablo";

impl Session {
    /// What to run now of `command`: `None` while it waits on a simulated
    /// verification, or when it answered one.
    pub(super) fn verify(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::SolveChallenge { challenge, solve } => {
                let (_, held) = self.challenge.take_if(|(id, _)| *id == challenge)?;
                if solve {
                    // Solved: it runs once, and does not ask again.
                    return Some(held);
                }
                if let Some(failure) = Failure::of(&held) {
                    self.events.send_lossy(Event::Failed {
                        failure,
                        reason: t("Cancelaste la verificación (simulada); no se envió nada.")
                            .into(),
                        ambiguous: false,
                    });
                }
                None
            }
            Command::SendFriendRequest { ref username }
                if username.trim().trim_start_matches('@').to_lowercase() == CHALLENGED
                    && self.challenge.is_none() =>
            {
                self.challenges += 1;
                let failure = Failure::of(&command).expect("a friend request has a failure");
                self.challenge = Some((self.challenges, command));
                self.events.send_lossy(Event::ChallengeRequired {
                    challenge: self.challenges,
                    failure,
                });
                None
            }
            command => Some(command),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockConfig, MockProvider};
    use crate::{EventSink, Provider};
    use rune_model::RelationshipKind;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::mpsc;

    async fn next(events: &mut mpsc::Receiver<crate::Envelope>) -> Event {
        tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap()
            .event
    }

    #[tokio::test]
    async fn the_simulated_case_waits_and_runs_once_when_solved() {
        let provider = Arc::new(MockProvider::new(MockConfig {
            latency: Duration::ZERO,
            ambient: false,
            replies: false,
            ..MockConfig::default()
        }));
        let (sender, mut events) = mpsc::channel(64);
        let sink = EventSink::new(sender, Default::default(), Arc::new(|| {}));
        let (commands, mut receiver) = mpsc::channel(8);
        let running = provider.clone();
        tokio::spawn(async move { running.connect(&mut receiver, &sink).await });
        assert!(matches!(next(&mut events).await, Event::Ready(_)));
        // The account's settings (activity sharing too), its perks (P5), the app in voice and, per
        // server, since when its voice rooms are on (voz-protocolo).
        for _ in 0..6 + crate::mock::SERVERS.len() {
            next(&mut events).await;
        }
        let ask = || Command::SendFriendRequest {
            username: "Pablo".into(),
        };
        commands.send(ask()).await.unwrap();
        let Event::ChallengeRequired { challenge, failure } = next(&mut events).await else {
            panic!("a simulated verification first");
        };
        assert_eq!(failure, Failure::Relationship { user: None });
        // Cancelled: nothing is sent.
        commands
            .send(Command::SolveChallenge {
                challenge,
                solve: false,
            })
            .await
            .unwrap();
        assert!(matches!(
            next(&mut events).await,
            Event::Failed {
                failure: Failure::Relationship { user: None },
                ambiguous: false,
                ..
            }
        ));
        // Asked again and solved: the request goes out once.
        commands.send(ask()).await.unwrap();
        let Event::ChallengeRequired { challenge, .. } = next(&mut events).await else {
            panic!("asked again");
        };
        for _ in 0..2 {
            commands
                .send(Command::SolveChallenge {
                    challenge,
                    solve: true,
                })
                .await
                .unwrap();
        }
        assert!(matches!(next(&mut events).await,
            Event::RelationshipChanged(relationship) if relationship.kind == RelationshipKind::OutgoingRequest));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), events.recv())
                .await
                .is_err(),
            "a second answer runs nothing"
        );
    }
}
