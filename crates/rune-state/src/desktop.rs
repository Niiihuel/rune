//! What the desktop work (P6) keeps of the session: the activity this
//! device publishes, shown as the account's own; the call's privacy code
//! and the codes compared with each person; and the games the provider
//! recognizes.

use std::collections::HashMap;
use std::sync::Arc;

use rune_model::{
    Activity, ActivityVisibility, Channel, ChannelId, DetectableGame, ServerId, UserId,
};

use crate::{Applied, Store};

/// People whose code is kept at once, in a call.
const VERIFICATIONS_KEPT: usize = 32;

/// A code compared with someone in the call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verification {
    /// Asked for; it takes a moment.
    Asking,
    Code(String),
    /// It could not be known: they left, or the call is not encrypted yet.
    Unknown,
}

#[derive(Debug, Default)]
pub(crate) struct DesktopState {
    local_activity: Option<Activity>,
    privacy: Option<(ChannelId, String)>,
    verifications: HashMap<UserId, Verification>,
    games: Option<Arc<Vec<DetectableGame>>>,
    // Desktop session and games (escritorio-sesion-y-juegos).
    /// The account shows its activity, once read.
    sharing: Option<bool>,
    /// What the person asked it to become, until the provider answers.
    sharing_asked: Option<bool>,
    /// Whether the activity published now is seen, as last said.
    visibility: Option<ActivityVisibility>,
}

impl Store {
    /// Show `activity` as the account's own while its provider has none:
    /// what this device publishes. Another one is not seen until said.
    pub fn set_local_activity(&mut self, activity: Option<Activity>) {
        if self.desktop.local_activity != activity {
            self.desktop.visibility = None;
        }
        self.desktop.local_activity = activity;
        self.revision += 1;
    }

    // Desktop session and games (escritorio-sesion-y-juegos).
    /// Whether the account shows its activity to others, once read.
    pub fn activity_sharing(&self) -> Option<bool> {
        self.desktop.sharing
    }

    /// What the person asked the account's activity sharing to become,
    /// while the provider has not answered.
    pub fn activity_sharing_asked(&self) -> Option<bool> {
        self.desktop.sharing_asked
    }

    /// Mark the change to `on` asked for.
    pub fn ask_activity_sharing(&mut self, on: bool) {
        self.desktop.sharing_asked = Some(on);
        self.revision += 1;
    }

    /// Whether others see the activity published now, once said.
    pub fn activity_visibility(&self) -> Option<ActivityVisibility> {
        self.desktop.local_activity.as_ref()?;
        self.desktop.visibility
    }

    pub(crate) fn sharing_changed(&mut self, on: bool) -> Applied {
        self.desktop.sharing = Some(on);
        self.desktop.sharing_asked = None;
        Applied::AccountSettings
    }

    pub(crate) fn visibility_changed(&mut self, seen: ActivityVisibility) -> Applied {
        self.desktop.visibility = Some(seen);
        Applied::AccountSettings
    }

    pub(crate) fn sharing_failed(&mut self) {
        self.desktop.sharing_asked = None;
    }

    /// Another account: what the last one keeps is unknown again.
    pub(crate) fn desktop_account_changed(&mut self) {
        self.desktop.sharing = None;
        self.desktop.sharing_asked = None;
        self.desktop.visibility = None;
    }

    /// The activity this device publishes, if any.
    pub fn local_activity(&self) -> Option<&Activity> {
        self.desktop.local_activity.as_ref()
    }

    /// The privacy code of the account's call, once encrypted.
    pub fn privacy_code(&self) -> Option<&str> {
        let (channel, code) = self.desktop.privacy.as_ref()?;
        (self.call.as_ref()?.channel == *channel).then_some(code.as_str())
    }

    /// The code compared with `user`, once asked for.
    pub fn verification(&self, user: UserId) -> Option<&Verification> {
        self.desktop.verifications.get(&user)
    }

    /// Mark `user`'s code asked for; false when too many are kept.
    pub fn begin_verification(&mut self, user: UserId) -> bool {
        let known = self.desktop.verifications.contains_key(&user);
        if !known && self.desktop.verifications.len() >= VERIFICATIONS_KEPT {
            return false;
        }
        self.desktop
            .verifications
            .insert(user, Verification::Asking);
        self.revision += 1;
        true
    }

    /// [`Store::channel_tree`] with the channels the account cannot see
    /// too, as Serein lists them when asked: whatever a server sent.
    pub fn channel_tree_with_hidden(
        &self,
        server: ServerId,
    ) -> Vec<(Option<&Channel>, Vec<&Channel>)> {
        self.tree_of(server, |_| true)
    }

    /// The games the provider recognizes by their programs, once read.
    pub fn detectable_games(&self) -> Option<Arc<Vec<DetectableGame>>> {
        self.desktop.games.clone()
    }

    pub(crate) fn call_code_changed(
        &mut self,
        channel: ChannelId,
        code: Option<String>,
    ) -> Applied {
        if self
            .call
            .as_ref()
            .is_none_or(|call| call.channel != channel)
        {
            return Applied::Nothing;
        }
        let changed = self.desktop.privacy.as_ref().map(|(_, code)| code) != code.as_ref();
        self.desktop.privacy = code.map(|code| (channel, code));
        // A new group has new keys: what was compared is old.
        if changed {
            self.desktop.verifications.clear();
        }
        Applied::Call
    }

    pub(crate) fn verified(
        &mut self,
        channel: ChannelId,
        user: UserId,
        code: Option<String>,
    ) -> Applied {
        let asked = self.desktop.verifications.contains_key(&user);
        if !asked
            || self
                .call
                .as_ref()
                .is_none_or(|call| call.channel != channel)
        {
            return Applied::Nothing;
        }
        self.desktop
            .verifications
            .insert(user, code.map_or(Verification::Unknown, Verification::Code));
        Applied::Call
    }

    pub(crate) fn games_listed(&mut self, games: Vec<DetectableGame>) -> Applied {
        self.desktop.games = Some(Arc::new(games));
        Applied::DetectableGames
    }

    /// The call ended: its codes go with it.
    pub(crate) fn desktop_call_ended(&mut self) {
        self.desktop.privacy = None;
        self.desktop.verifications.clear();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use rune_model::CallPhase;
    use rune_provider::Event;

    use super::*;

    #[test]
    fn codes_belong_to_the_call_and_go_with_it() {
        let mut store = Store::new();
        let now = Instant::now();
        let channel = ChannelId(5);
        let code = |code: &str| Event::CallPrivacyCode {
            channel,
            code: Some(code.into()),
        };
        assert_eq!(store.apply(code("1"), now), Applied::Nothing, "no call");
        store.apply(
            Event::CallPhase {
                channel,
                phase: CallPhase::Connected,
                quality: None,
            },
            now,
        );
        store.apply(code("123"), now);
        assert_eq!(store.privacy_code(), Some("123"));
        let ana = UserId(7);
        let answer = Event::VoiceVerification {
            channel,
            user: ana,
            code: Some("9".into()),
        };
        store.apply(answer.clone(), now);
        assert_eq!(store.verification(ana), None, "nobody asked");
        assert!(store.begin_verification(ana));
        assert_eq!(store.verification(ana), Some(&Verification::Asking));
        store.apply(answer, now);
        assert_eq!(
            store.verification(ana),
            Some(&Verification::Code("9".into()))
        );
        store.apply(code("456"), now);
        assert_eq!(store.verification(ana), None, "a new group: compare again");
        store.apply(
            Event::CallEnded {
                channel,
                reason: None,
            },
            now,
        );
        assert_eq!(store.privacy_code(), None);
    }

    // Desktop session and games (escritorio-sesion-y-juegos).
    #[test]
    fn sharing_waits_for_its_answer_and_visibility_belongs_to_the_activity() {
        use rune_provider::Failure;
        let mut store = Store::new();
        let now = Instant::now();
        assert_eq!(store.activity_sharing(), None);
        store.apply(Event::ActivitySharing(false), now);
        store.ask_activity_sharing(true);
        assert_eq!(store.activity_sharing_asked(), Some(true));
        store.apply(
            Event::Failed {
                failure: Failure::ActivitySharing,
                reason: "no".into(),
                ambiguous: false,
            },
            now,
        );
        assert_eq!(
            (store.activity_sharing(), store.activity_sharing_asked()),
            (Some(false), None),
            "a failure leaves it as read"
        );
        store.ask_activity_sharing(true);
        store.apply(Event::ActivitySharing(true), now);
        assert_eq!(
            (store.activity_sharing(), store.activity_sharing_asked()),
            (Some(true), None)
        );
        // Seen only while there is an activity, and only that one.
        store.apply(Event::ActivityVisibility(ActivityVisibility::Listed), now);
        assert_eq!(store.activity_visibility(), None);
        let game = |name: &str| rune_model::LocalActivity::default().shown(name.into());
        store.set_local_activity(Some(game("Uno")));
        store.apply(Event::ActivityVisibility(ActivityVisibility::Hidden), now);
        assert_eq!(
            store.activity_visibility(),
            Some(ActivityVisibility::Hidden)
        );
        store.set_local_activity(Some(game("Uno")));
        assert_eq!(
            store.activity_visibility(),
            Some(ActivityVisibility::Hidden)
        );
        store.set_local_activity(Some(game("Dos")));
        assert_eq!(store.activity_visibility(), None);
    }
}
