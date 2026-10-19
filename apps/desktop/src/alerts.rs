//! The system's notifications of new messages and of calls coming in: which
//! ones `rune_ui` decided, shown through `rune_platform::notifications`, and
//! where a click or a button leads.
//!
//! The service starts the first time something is shown. Each notification
//! keeps the account and conversation it came from; a click on one of
//! another account opens nothing, and changing account withdraws them all.
//! A call's offers «Aceptar» and «Rechazar» and goes away when it stops
//! ringing (llamadas-y-panel-de-voz).

use std::collections::VecDeque;
use std::sync::Arc;

use rune_i18n::t;
use rune_model::{ChannelId, UserId};
use rune_platform::notifications::{Notice, Notifier};
use rune_ui::Alert;

/// Notifications whose click is remembered; older ones open nothing. A
/// call's is forgotten last: it must be found again to take it away.
const REMEMBERED: usize = 32;

/// One shown: its key, account and conversation, whether it is a call's,
/// and whether that call stopped ringing and it is still to be withdrawn.
struct Shown {
    key: u64,
    account: UserId,
    channel: ChannelId,
    call: bool,
    stopped: bool,
}

/// Remember `shown`, forgetting the oldest beyond [`REMEMBERED`]: a
/// message's before a call's.
fn remember(list: &mut VecDeque<Shown>, shown: Shown) {
    list.push_back(shown);
    if list.len() > REMEMBERED {
        let oldest = list.iter().position(|known| !known.call);
        list.remove(oldest.unwrap_or(0));
    }
}

/// Withdraw the calls of `stopped`, and those the service was too busy to
/// take before; `withdraw` says whether it took one. Messages stay.
fn withdraw_stopped(
    list: &mut VecDeque<Shown>,
    stopped: &[ChannelId],
    mut withdraw: impl FnMut(u64) -> bool,
) {
    for shown in list.iter_mut() {
        shown.stopped |= shown.call && stopped.contains(&shown.channel);
    }
    list.retain(|shown| !(shown.stopped && withdraw(shown.key)));
}

pub struct Alerts {
    notifier: Option<Notifier>,
    wake: Arc<dyn Fn() + Send + Sync>,
    shown: VecDeque<Shown>,
    next: u64,
    account: Option<UserId>,
}

impl Alerts {
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            notifier: None,
            wake,
            shown: VecDeque::new(),
            next: 0,
            account: None,
        }
    }

    /// Show `alert` as a notification of the system.
    pub fn show(&mut self, alert: &Alert) {
        let notifier = self
            .notifier
            .get_or_insert_with(|| Notifier::start(self.wake.clone()));
        self.next += 1;
        let mut notice = Notice::new(self.next, &alert.title, &alert.body);
        if alert.call {
            notice = notice.ringing(&[
                (rune_ui::CALL_ANSWER, t("Aceptar")),
                (rune_ui::CALL_DECLINE, t("Rechazar")),
            ]);
        }
        if notifier.show(notice) {
            remember(
                &mut self.shown,
                Shown {
                    key: self.next,
                    account: alert.account,
                    channel: alert.channel,
                    call: alert.call,
                    stopped: false,
                },
            );
        }
    }

    /// The calls of `stopped` stopped ringing: their notifications go away.
    /// Asked every frame, so one the service was too busy for goes later.
    // shortcut: that retry waits for the next frame the window draws; wake
    // it if a call's notification is seen to linger.
    pub fn calls_stopped(&mut self, stopped: &[ChannelId]) {
        let Some(notifier) = &self.notifier else {
            return;
        };
        withdraw_stopped(&mut self.shown, stopped, |key| notifier.withdraw(key));
    }

    /// The account signed in now: another one's notifications go away.
    pub fn account(&mut self, account: Option<UserId>) {
        if self.account == account {
            return;
        }
        self.account = account;
        self.shown.clear();
        if let Some(notifier) = &self.notifier {
            notifier.withdraw_all();
            while notifier.poll_click().is_some() {}
        }
    }

    /// The account and conversation of a notification clicked, and the
    /// button pressed on it, if one was (`rune_ui::CALL_ANSWER` or
    /// `rune_ui::CALL_DECLINE`).
    pub fn clicked(&mut self) -> Option<(UserId, ChannelId, Option<&'static str>)> {
        let (key, button) = self.notifier.as_ref()?.poll_click()?;
        self.shown
            .iter()
            .find(|shown| shown.key == key)
            .map(|shown| (shown.account, shown.channel, button))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(key: u64, channel: u64, call: bool) -> Shown {
        Shown {
            key,
            account: UserId(1),
            channel: ChannelId(channel),
            call,
            stopped: false,
        }
    }

    #[test]
    fn a_call_stopped_withdraws_only_its_own_and_again_while_busy() {
        let mut list: VecDeque<Shown> =
            [shown(1, 7, false), shown(2, 7, true), shown(3, 8, true)].into();
        // Busy: nothing is taken, and it is kept to try again.
        withdraw_stopped(&mut list, &[ChannelId(7)], |_| false);
        assert_eq!(list.len(), 3);
        assert!(list[1].stopped && !list[0].stopped && !list[2].stopped);
        // Next frame, with nothing new stopped: that one goes, alone.
        let mut taken = Vec::new();
        withdraw_stopped(&mut list, &[], |key| {
            taken.push(key);
            true
        });
        assert_eq!(taken, [2]);
        let keys: Vec<u64> = list.iter().map(|known| known.key).collect();
        assert_eq!(keys, [1, 3], "the message and the other call stay");
    }

    #[test]
    fn a_call_ringing_is_remembered_past_newer_messages() {
        let mut list = VecDeque::new();
        remember(&mut list, shown(0, 7, true));
        for key in 1..=REMEMBERED as u64 + 4 {
            remember(&mut list, shown(key, 8, false));
        }
        assert_eq!(list.len(), REMEMBERED);
        assert_eq!(list[0].key, 0, "the call is still there to withdraw");
        assert_eq!(list[1].key, 6, "the oldest messages went");
    }
}
