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
    /// A check or an install is on its way: the buttons wait for it.
    pub busy: bool,
    /// When the last check ended, and why it failed if it did.
    pub checked: Option<Timestamp>,
    pub error: Option<String>,
    /// The newer release, and where its installation stands.
    pub offered: Option<UpdateView>,
    /// The latest release the last check found, newer or not: its version
    /// and notes, for Ajustes › Actualizaciones to say what it brought.
    pub latest: Option<(String, String)>,
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
            busy: false,
            checked: None,
            error: None,
            offered: None,
            latest: None,
            later: false,
            notes_open: false,
        }
    }
}

impl Updates {
    /// The notes Ajustes › Actualizaciones shows, with their version: the
    /// update offered's, else those of the version installed when the last
    /// check found it is the latest.
    pub fn shown_notes(&self) -> Option<(&str, &str)> {
        if let Some(offered) = &self.offered {
            return Some((offered.version.as_str(), offered.notes.as_str()));
        }
        self.latest
            .as_ref()
            .filter(|(version, _)| version == env!("CARGO_PKG_VERSION"))
            .map(|(version, notes)| (version.as_str(), notes.as_str()))
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
        ids::CHECK if !updates.checking && !updates.busy => vec![Intent::CheckForUpdates],
        ids::INSTALL | ids::RETRY if updates.installable() => vec![Intent::InstallUpdate],
        ids::RESTART if matches!(&updates.offered, Some(offered) if offered.stage == Stage::Installed) =>
        {
            vec![Intent::RestartForUpdate]
        }
        ids::CHECK | ids::INSTALL | ids::RETRY | ids::RESTART => Vec::new(),
        ids::LATER => {
            updates.later = true;
            Vec::new()
        }
        ids::NOTES => {
            updates.notes_open = !updates.notes_open;
            Vec::new()
        }
        // Its page on GitHub, with the pictures and videos of its notes.
        ids::ON_GITHUB => match updates.shown_notes() {
            Some((version, _)) => vec![Intent::OpenUrl(rune_components::updates::release_page(
                version,
            ))],
            None => Vec::new(),
        },
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
    fn the_notes_shown_are_the_offered_ones_else_the_installed_version_s() {
        let mut ui = Ui::new();
        assert_eq!(ui.updates.shown_notes(), None);
        assert_eq!(click(&mut ui, ids::ON_GITHUB), Some(Vec::new()));
        ui.updates.latest = Some(("0.0.1".into(), "Vieja.".into()));
        assert_eq!(ui.updates.shown_notes(), None, "not this version's");
        let installed = env!("CARGO_PKG_VERSION");
        ui.updates.latest = Some((installed.into(), "Lo nuevo.".into()));
        assert_eq!(ui.updates.shown_notes(), Some((installed, "Lo nuevo.")));
        ui.updates.offered = offered(Stage::Offered);
        assert_eq!(ui.updates.shown_notes(), Some(("1.2.0", "Notas.")));
        assert_eq!(
            click(&mut ui, ids::ON_GITHUB),
            Some(vec![Intent::OpenUrl(
                "https://github.com/Niiihuel/rune/releases/tag/v1.2.0".into()
            )])
        );
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
        let shown = |ui: &Ui| card(ui, &look).is_some();
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
