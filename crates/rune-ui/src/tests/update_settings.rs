//! Ajustes › Actualizaciones shows an update the way the screens do: its
//! progress while it downloads, a failure with «Reintentar», and the restart
//! a cancelled close leaves to offer.

use super::*;
use rune_components::updates::{ids, Stage, UpdateView};

fn press(host: &mut Host, id: &str) -> Vec<Intent> {
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked { id: id.into() },
        host.now,
    );
    host.run(intents.clone());
    intents
}

fn open_updates(host: &mut Host) {
    host.click("me:settings");
    press(host, "us:page=updates");
    assert_eq!(host.ui.settings, Some(crate::SettingsPage::Updates));
}

fn offer(host: &mut Host, stage: Stage) {
    host.ui.updates.installation = Some("/home/ada/.local/share/rune/app".into());
    host.ui.updates.offered = Some(UpdateView {
        version: "1.2.0".into(),
        installed: "1.1.0".into(),
        notes: "Notas.".into(),
        stage,
    });
}

fn names(frame: &Frame) -> Vec<String> {
    frame.access.iter().map(|node| node.name.clone()).collect()
}

fn has_button(frame: &Frame, id: &str) -> bool {
    frame
        .regions
        .iter()
        .any(|region| region.id == id && region.kind == limn::HitKind::Button)
}

#[test]
fn settings_shows_the_download_with_its_bytes_and_no_install_button() {
    let mut host = Host::new();
    open_updates(&mut host);
    offer(
        &mut host,
        Stage::Downloading {
            have: 512 * 1024,
            whole: 2 * 1024 * 1024,
        },
    );
    let frame = host.frame();
    assert!(
        names(&frame)
            .iter()
            .any(|name| name.starts_with("Descargando… 512.0 KB de 2.0 MB")),
        "the bar's label: {:?}",
        names(&frame)
    );
    assert!(
        !has_button(&frame, ids::INSTALL),
        "no second install while it runs"
    );
}

#[test]
fn a_failed_download_offers_try_again_and_pressing_it_installs_again() {
    let mut host = Host::new();
    open_updates(&mut host);
    offer(&mut host, Stage::Failed("Sin conexión.".into()));
    let frame = host.frame();
    assert!(names(&frame).iter().any(|name| name == "Sin conexión."));
    assert!(has_button(&frame, ids::RETRY));
    assert!(!has_button(&frame, ids::INSTALL));
    assert_eq!(press(&mut host, ids::RETRY), vec![Intent::InstallUpdate]);
}

#[test]
fn an_installed_update_whose_close_was_cancelled_restarts_from_settings() {
    let mut host = Host::new();
    open_updates(&mut host);
    offer(&mut host, Stage::Installed);
    let frame = host.frame();
    assert!(has_button(&frame, ids::RESTART));
    assert_eq!(
        press(&mut host, ids::RESTART),
        vec![Intent::RestartForUpdate]
    );
}

#[test]
fn check_waits_while_a_job_runs() {
    let mut host = Host::new();
    open_updates(&mut host);
    host.ui.updates.busy = true;
    assert_eq!(press(&mut host, ids::CHECK), Vec::new());
}
