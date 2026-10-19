//! The settings kept for each account rather than for the device: where it
//! was, its favorites and pinned conversations, how its servers and channels
//! notify and which it hides or collapses, and its status timers. They name
//! its servers and channels, so another account never takes them. The host
//! keeps them apart and hands them back when the account signs in.

use rune_model::{ChannelId, ServerId};

use crate::{account, channel_list, profile, server_menu, Ui};

/// The keys of [`Ui::settings_to_keep`] that belong to the account.
pub const ACCOUNT_SETTINGS: [&str; 11] = [
    "server",
    "channel",
    "favorite_channels",
    "pinned_directs",
    server_menu::NOTIFY_SETTING,
    channel_list::NOTIFY_SETTING,
    channel_list::HIDE_MUTED_SETTING,
    channel_list::COLLAPSED_SETTING,
    profile::STATUS_TIMER,
    profile::CUSTOM_UNTIL,
    // Rail and notifications.
    account::OPEN_FOLDERS_SETTING,
];

impl Ui {
    /// Take back what the account signed in kept ([`ACCOUNT_SETTINGS`]);
    /// what it did not keep starts as for a new account.
    pub fn restore_account(&mut self, setting: impl Fn(&str) -> Option<String>) {
        self.server = setting("server")
            .and_then(|value| value.parse().ok())
            .map(ServerId);
        if let Some(channel) = setting("channel")
            .and_then(|value| value.parse().ok())
            .map(ChannelId)
        {
            self.channels.insert(self.server, channel);
        }
        crate::desktop::restore_lists(&mut self.prefs, &setting);
        server_menu::restore(self, setting(server_menu::NOTIFY_SETTING));
        channel_list::restore(self, &setting);
        profile::restore(self, &setting);
        account::restore_open_folders(self, setting(account::OPEN_FOLDERS_SETTING));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_takes_back_only_its_own_and_leaves_the_device_s() {
        let mut ui = Ui::new();
        ui.prefs.ui_font = Some("Inter".into());
        ui.prefs.favorites = vec![ChannelId(1)];
        ui.server = Some(ServerId(5));
        let kept = |key: &str| match key {
            "server" => Some("7".into()),
            "channel" => Some("70".into()),
            "favorite_channels" => Some("71,72".into()),
            "ui_font" => Some("Other".into()),
            _ => None,
        };
        ui.restore_account(kept);
        assert_eq!(ui.server, Some(ServerId(7)));
        assert_eq!(ui.channels.get(&Some(ServerId(7))), Some(&ChannelId(70)));
        assert_eq!(ui.prefs.favorites, [ChannelId(71), ChannelId(72)]);
        assert_eq!(ui.prefs.ui_font.as_deref(), Some("Inter"), "the device's");
        // Nothing kept: a new account's.
        ui.restore_account(|_| None);
        assert_eq!(ui.server, None);
        assert!(ui.prefs.favorites.is_empty());
        // Everything the account keeps is among what the window keeps.
        let kept: Vec<&str> = ui.settings_to_keep().iter().map(|(key, _)| *key).collect();
        for key in ACCOUNT_SETTINGS {
            assert!(kept.contains(&key), "{key}");
        }
    }
}
