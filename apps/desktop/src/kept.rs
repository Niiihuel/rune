//! The settings kept on this device, as the host hands them to the local
//! store, with any provider: the device's under their own keys, and each
//! account's ([`rune_ui::ACCOUNT_SETTINGS`]) under
//! `account:{provider}:{id}:{key}`. All of them are read once at startup,
//! so taking back the settings of the account that signs in never waits on
//! the disk. Never a credential: only what the interface keeps.

use std::collections::HashMap;

use rune_model::UserId;
use rune_ui::ACCOUNT_SETTINGS;

#[derive(Default)]
pub struct Kept {
    values: HashMap<String, String>,
    /// The prefix of the account signed in, whose settings the interface holds.
    account: Option<String>,
}

impl Kept {
    pub fn new(rows: Vec<(String, String)>) -> Self {
        Self {
            values: rows.into_iter().collect(),
            account: None,
        }
    }

    /// The value of `key` as the interface asks for it: an account's
    /// setting is the one of the account signed in, none without one.
    pub fn get(&self, key: &str) -> Option<String> {
        self.stored(key)
            .and_then(|stored| self.values.get(&stored))
            .cloned()
    }

    /// Keep `value` under `key`, and say where it goes in the store; `None`
    /// for an account's setting while nobody is signed in.
    pub fn put(&mut self, key: &str, value: Option<String>) -> Option<String> {
        let stored = self.stored(key)?;
        match &value {
            Some(value) => self.values.insert(stored.clone(), value.clone()),
            None => self.values.remove(&stored),
        };
        Some(stored)
    }

    /// The account of `provider` signed in now, if any: true when it is
    /// another one than before, and the interface must take back its
    /// settings. `legacy`: it is the account the device's keys held before
    /// settings were kept by account (the simulation's), so with nothing of
    /// its own yet it takes those.
    pub fn sign_in(&mut self, provider: &str, account: Option<UserId>, legacy: bool) -> bool {
        let prefix = account.map(|account| format!("account:{provider}:{}:", account.0));
        if prefix == self.account {
            return false;
        }
        if let Some(prefix) = prefix.as_ref().filter(|_| legacy) {
            if !self
                .values
                .keys()
                .any(|key| key.starts_with(prefix.as_str()))
            {
                for key in ACCOUNT_SETTINGS {
                    if let Some(value) = self.values.get(key).cloned() {
                        self.values.insert(format!("{prefix}{key}"), value);
                    }
                }
            }
        }
        self.account = prefix;
        true
    }

    fn stored(&self, key: &str) -> Option<String> {
        if ACCOUNT_SETTINGS.contains(&key) {
            self.account.as_ref().map(|prefix| format!("{prefix}{key}"))
        } else {
            Some(key.to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_account_keeps_its_own_and_the_device_s_are_everyone_s() {
        let mut kept = Kept::new(vec![
            ("zoom".into(), "125".into()),
            ("favorite_channels".into(), "1,2".into()),
            (
                "account:discord-session:7:favorite_channels".into(),
                "70".into(),
            ),
        ]);
        assert_eq!(kept.get("zoom").as_deref(), Some("125"));
        assert_eq!(kept.get("favorite_channels"), None, "nobody signed in");
        assert_eq!(kept.put("server", Some("5".into())), None);
        assert!(kept.sign_in("discord-session", Some(UserId(7)), false));
        assert!(!kept.sign_in("discord-session", Some(UserId(7)), false));
        assert_eq!(kept.get("favorite_channels").as_deref(), Some("70"));
        assert_eq!(
            kept.put("server", Some("5".into())).as_deref(),
            Some("account:discord-session:7:server")
        );
        assert_eq!(kept.put("zoom", None).as_deref(), Some("zoom"));
        // Another account, or another provider with the same id: their own.
        assert!(kept.sign_in("discord", Some(UserId(7)), false));
        assert_eq!(kept.get("server"), None);
        assert_eq!(kept.get("zoom"), None, "the device's, for everyone");
        // The simulation's account takes what the device kept before.
        assert!(kept.sign_in("mock", Some(UserId(1)), true));
        assert_eq!(kept.get("favorite_channels").as_deref(), Some("1,2"));
        assert!(kept.sign_in("mock", None, true));
        assert_eq!(kept.get("favorite_channels"), None);
    }
}
