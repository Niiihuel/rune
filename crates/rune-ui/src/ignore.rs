//! Ignoring someone, as the official client does: their messages fold, they
//! raise no notifications, their activity is hidden, and they are not told.
//! The item for a person's menu is here; the question before ignoring and the
//! answer to it are in `relationships`, under the person's `friend:` actions.

use rune_components::voice::MenuEntry;
use rune_i18n::t;
use rune_model::UserId;
use rune_state::Store;

/// The ignore item of `user`'s menu: «Ignorar», or «Dejar de ignorar» once
/// ignored; none when the account may not do either.
pub(crate) fn menu(store: &Store, user: UserId) -> Vec<MenuEntry> {
    let social = store.social(user);
    let person = |action: &str| format!("friend:{}:{action}", user.0);
    if store.is_ignored(user) {
        if social.unignore.allowed() {
            return vec![MenuEntry::Item(
                person("unignore"),
                t("Dejar de ignorar").into(),
                None,
            )];
        }
    } else if social.ignore.allowed() {
        return vec![MenuEntry::Item(person("ignore"), t("Ignorar").into(), None)];
    }
    Vec::new()
}
