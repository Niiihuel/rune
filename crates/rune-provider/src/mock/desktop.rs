//! Desktop, language and local voice (P6) in the simulation: the account's
//! local activity shows in its presence, a short made-up list of games, and
//! call codes that are made up from the call, the same every time. Nothing
//! is encrypted here; the interface says so.

use rune_i18n::{t, tr};
use rune_model::{ActivityVisibility, ChannelId, DetectableGame, LocalActivity, Presence, UserId};

use super::{World, ME};
use crate::Event;

impl World {
    /// Show `activity` as the account's, as a service would echo it.
    pub(super) fn local_activity(&mut self, activity: Option<LocalActivity>) -> Vec<Event> {
        let presence = Presence {
            activities: activity
                .map(|activity| {
                    activity.shown(activity.name.clone().unwrap_or_else(|| {
                        match activity.application {
                            Some(application) => {
                                tr!("Aplicación {application} (simulada)", application)
                            }
                            None => t("Aplicación (simulada)").into(),
                        }
                    }))
                })
                .into_iter()
                .collect(),
            ..self.presences.get(&ME).cloned().unwrap_or_default()
        };
        self.presences.insert(ME, presence.clone());
        let mut events = vec![Event::PresenceChanged { user: ME, presence }];
        events.extend(self.local_visibility());
        events
    }

    // Desktop session and games (escritorio-sesion-y-juegos).
    /// Whether the activity shown is seen, as a service's sessions would
    /// say it: listed while the account shares it, hidden otherwise.
    pub(super) fn local_visibility(&self) -> Option<Event> {
        let shown = self
            .presences
            .get(&ME)
            .is_some_and(|presence| !presence.activities.is_empty());
        shown.then_some(Event::ActivityVisibility(if self.account.sharing {
            ActivityVisibility::Listed
        } else {
            ActivityVisibility::Hidden
        }))
    }

    /// The code `user` and the account compare, while both are in the call.
    pub(super) fn verification(&self, channel: ChannelId, user: UserId) -> Event {
        let here = |who: UserId| {
            self.voice
                .iter()
                .any(|state| state.user == who && state.channel == channel)
        };
        let code = (here(ME) && here(user) && user != ME)
            .then(|| digits(&[channel.0, ME.0.min(user.0), ME.0.max(user.0)], 45));
        Event::VoiceVerification {
            channel,
            user,
            code,
        }
    }
}

/// The call's privacy code in the simulation.
pub(super) fn privacy_code(channel: ChannelId) -> String {
    digits(&[channel.0, 0xDA7E], 30)
}

/// A few made-up games, one named after a program anyone can start to try it.
pub(super) fn games() -> Vec<DetectableGame> {
    vec![
        DetectableGame {
            application: Some(1_001),
            name: "Ajedrez de prueba".into(),
            executables: vec!["rune-ajedrez".into(), "win32/rune-ajedrez.exe".into()],
        },
        DetectableGame {
            application: Some(1_002),
            name: "Carreras simuladas".into(),
            executables: vec!["carreras.x86_64".into()],
        },
    ]
}

/// `count` decimal digits from `seed`, the same for the same seed.
fn digits(seed: &[u64], count: usize) -> String {
    let mut state = seed.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, value| {
        (hash ^ value).wrapping_mul(0x0100_0000_01b3)
    });
    (0..count)
        .map(|_| {
            // xorshift64*
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let value = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
            char::from(b'0' + (value >> 60) as u8 % 10)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_digits_and_the_same_for_the_same_call() {
        let code = privacy_code(ChannelId(7));
        assert_eq!(code.len(), 30);
        assert!(code.bytes().all(|byte| byte.is_ascii_digit()));
        assert_eq!(code, privacy_code(ChannelId(7)));
        assert_ne!(code, privacy_code(ChannelId(8)));
    }
}
