//! RUNE's own updates (actualizaciones, 2026-10-19): what the interface
//! knows of them, which the host keeps current; the notice of an optional
//! one over the window; and what that notice and Ajustes › Actualizaciones
//! ask for. A mandatory update never reaches this window: the host shows
//! its screen (`rune_components::updates::screen`) before anything opens.

use limn::{Element, UiAction};
use rune_components::updates::{ids, notice, Stage, UpdateView};
use rune_components::Look;
use rune_model::Timestamp;

use crate::{Intent, Ui};

/// What the interface knows of updates.
#[derive(Clone, Debug, PartialEq)]
pub struct Updates {
    /// Where this copy is installed, when RUNE's installer put it there
    /// and an update can replace it; `None` for a build from source.
    pub installation: Option<String>,
    /// Look for a newer RUNE when it starts.
    pub at_start: bool,
    /// A check is on its way.
    pub checking: bool,
    /// When the last check ended, and why it failed if it did.
    pub checked: Option<Timestamp>,
    pub error: Option<String>,
    /// The newer release, and where its installation stands.
    pub offered: Option<UpdateView>,
    /// «Más tarde»: no notice until the next start; the settings still
    /// offer it.
    pub later: bool,
    pub notes_open: bool,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            installation: None,
            // On until turned off (owner's decision, 2026-10-19).
            at_start: true,
            checking: false,
            checked: None,
            error: None,
            offered: None,
            later: false,
            notes_open: false,
        }
    }
}

impl Updates {
    /// The release offered can be installed now: this copy is RUNE's
    /// installer's, and nothing is under way.
    pub fn installable(&self) -> bool {
        self.installation.is_some()
            && self
                .offered
                .as_ref()
                .is_some_and(|offered| matches!(offered.stage, Stage::Offered | Stage::Failed(_)))
    }
}

/// What the notice and the settings ask for; `None` for other actions.
pub(crate) fn update(ui: &mut Ui, action: &UiAction) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    let updates = &mut ui.updates;
    Some(match id.as_str() {
        ids::CHECK if !updates.checking => vec![Intent::CheckForUpdates],
        ids::INSTALL | ids::RETRY if updates.installable() => vec![Intent::InstallUpdate],
        ids::CHECK | ids::INSTALL | ids::RETRY => Vec::new(),
        ids::LATER => {
            updates.later = true;
            Vec::new()
        }
        ids::NOTES => {
            updates.notes_open = !updates.notes_open;
            Vec::new()
        }
        ids::AT_START => {
            updates.at_start = !updates.at_start;
            vec![Intent::CheckUpdatesAtStart(updates.at_start)]
        }
        _ => return None,
    })
}

/// The notice of an optional update, for the corner, until it is put off;
/// only for a copy that can install it, and not over the settings, whose
/// page offers it already. The other notices stack over it there
/// (`view::with_notices`), never on its buttons.
pub(crate) fn card(ui: &Ui, look: &Look) -> Option<Element> {
    let updates = &ui.updates;
    match &updates.offered {
        Some(offered)
            if !updates.later && updates.installation.is_some() && ui.settings.is_none() =>
        {
            Some(notice(offered, updates.notes_open, look))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(ui: &mut Ui, id: &str) -> Option<Vec<Intent>> {
        update(ui, &UiAction::Clicked { id: id.into() })
    }

    fn offered(stage: Stage) -> Option<UpdateView> {
        Some(UpdateView {
            version: "1.2.0".into(),
            installed: "1.1.0".into(),
            notes: "Notas.".into(),
            stage,
        })
    }

    #[test]
    fn an_update_is_installed_only_by_a_copy_that_can_and_only_once() {
        let mut ui = Ui::new();
        assert_eq!(click(&mut ui, ids::INSTALL), Some(Vec::new()));
        ui.updates.offered = offered(Stage::Offered);
        // A build from source only hears of it.
        assert_eq!(click(&mut ui, ids::INSTALL), Some(Vec::new()));
        ui.updates.installation = Some("/home/ada/.local/share/rune/app".into());
        assert_eq!(
            click(&mut ui, ids::INSTALL),
            Some(vec![Intent::InstallUpdate])
        );
        ui.updates.offered = offered(Stage::Downloading { have: 1, whole: 2 });
        assert_eq!(click(&mut ui, ids::INSTALL), Some(Vec::new()));
        ui.updates.offered = offered(Stage::Failed("No.".into()));
        assert_eq!(
            click(&mut ui, ids::RETRY),
            Some(vec![Intent::InstallUpdate])
        );
    }

    #[test]
    fn checks_are_asked_one_at_a_time_and_at_start_is_kept() {
        let mut ui = Ui::new();
        assert_eq!(
            click(&mut ui, ids::CHECK),
            Some(vec![Intent::CheckForUpdates])
        );
        ui.updates.checking = true;
        assert_eq!(click(&mut ui, ids::CHECK), Some(Vec::new()));
        assert!(ui.updates.at_start, "on by default");
        assert_eq!(
            click(&mut ui, ids::AT_START),
            Some(vec![Intent::CheckUpdatesAtStart(false)])
        );
        assert!(!ui.updates.at_start);
        assert_eq!(click(&mut ui, "us:close"), None);
    }

    #[test]
    fn the_notice_shows_until_put_off_and_never_for_a_build_from_source() {
        let mut ui = Ui::new();
        let look = Look::DARK;
        let base = format!("{:?}", Element::spacer());
        let shown = |ui: &Ui| format!("{:?}", overlay(ui, Element::spacer(), &look)) != base;
        ui.updates.offered = offered(Stage::Offered);
        assert!(!shown(&ui));
        ui.updates.installation = Some("/opt".into());
        assert!(shown(&ui));
        ui.settings = Some(crate::SettingsPage::Updates);
        assert!(!shown(&ui), "the page offers it");
        ui.settings = None;
        assert_eq!(click(&mut ui, ids::NOTES), Some(Vec::new()));
        assert!(ui.updates.notes_open);
        assert_eq!(click(&mut ui, ids::LATER), Some(Vec::new()));
        assert!(!shown(&ui));
    }
}
