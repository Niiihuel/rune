//! AutoMod (automod): a server's automatic moderation that refused a
//! message, an edit or a title ends that command like any refusal, with
//! the reason worded for the person, so the line under a message that did
//! not go says why instead of a generic failure.

use rune_i18n::{t, tr};
use rune_provider::Event;

/// `event`, with a refusal by AutoMod turned into the failure of its command.
pub(crate) fn worded(event: Event) -> Event {
    match event {
        Event::AutoModBlocked { failure, notice } => Event::Failed {
            failure,
            reason: reason(notice.as_deref()),
            ambiguous: false,
        },
        event => event,
    }
}

/// Why it did not go: AutoMod, and what the server tells whoever wrote it.
fn reason(notice: Option<&str>) -> String {
    match notice.map(str::trim).filter(|notice| !notice.is_empty()) {
        Some(notice) => tr!(
            "la moderación automática del servidor lo bloqueó: «{notice}»",
            notice
        ),
        None => t("la moderación automática del servidor lo bloqueó").into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::ChannelId;
    use rune_provider::Failure;

    #[test]
    fn a_block_ends_its_command_with_what_the_server_says() {
        let failure = Failure::Send {
            channel: ChannelId(5),
            nonce: 9,
        };
        let _spanish = rune_i18n::scoped(rune_i18n::Language::Spanish);
        let blocked = |notice: Option<&str>| {
            worded(Event::AutoModBlocked {
                failure: failure.clone(),
                notice: notice.map(str::to_owned),
            })
        };
        assert_eq!(
            blocked(Some(" Sin invitaciones. ")),
            Event::Failed {
                failure: failure.clone(),
                reason: "la moderación automática del servidor lo bloqueó: «Sin invitaciones.»"
                    .into(),
                ambiguous: false,
            }
        );
        for silent in [None, Some("  ")] {
            assert!(
                matches!(blocked(silent), Event::Failed { reason, ambiguous: false, .. }
                if reason == "la moderación automática del servidor lo bloqueó")
            );
        }
        let other = Event::ActivitySharing(true);
        assert_eq!(worded(other.clone()), other);
    }
}
