//! What the account keeps with its provider, through the whole window over
//! the simulation, which keeps it: notifications asked for and shown once
//! answered, folders on the rail and their menus, privacy in the settings
//! and notes in profiles.

use super::*;
use rune_components::folders::ids as folder_ids;
use rune_components::server_menu::ids as menu_ids;
use rune_model::{
    NotificationChange, PrivacyChange, ServerFolder, ServerId, UserId, MUTED_FOREVER,
};

impl Host {
    fn ac_server(&self, name: &str) -> ServerId {
        self.store
            .servers()
            .iter()
            .find(|server| server.name == name)
            .map(|server| server.id)
            .unwrap()
    }

    /// The account's settings read, as a session starts with them.
    fn ac_load(&mut self) {
        let from = self.sent.len();
        self.sent.push(Command::LoadAccountSettings);
        self.answer_account(from);
        for event in self.provider.manage(Command::SetNotifications {
            server: None,
            change: NotificationChange::HideMuted(false),
        }) {
            self.event(event);
        }
    }

    fn ac_controls(&mut self) -> Vec<String> {
        self.frame()
            .regions
            .iter()
            .filter(|region| region.kind == limn::HitKind::Button)
            .map(|region| region.id.clone())
            .collect()
    }

    /// Choose `id`, as a click on it reports it, even inside a submenu.
    fn ac_choose(&mut self, id: &str) {
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::Clicked { id: id.into() },
            self.now,
        );
        self.run(intents);
    }

    fn ac_right_click(&mut self, id: &str) {
        let frame = self.frame();
        let rect = frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is not on screen"))
            .rect;
        let at = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        self.send(UiEvent::ContextMenu { position: at });
    }
}

#[test]
fn notifications_are_asked_of_the_account_and_shown_once_it_answers() {
    let mut host = Host::new();
    let taller = host.ac_server("Taller de Rust");
    host.click(menu_ids::TOGGLE);
    let from = host.sent.len();
    host.ac_choose(&format!("sm:mute:{taller}:always"));
    assert_eq!(
        host.sent.last(),
        Some(&Command::SetNotifications {
            server: Some(taller),
            change: NotificationChange::Mute(Some(MUTED_FOREVER)),
        })
    );
    // Nothing is kept on this machine, nor shown before the answer.
    assert!(host.ui.server_menu.notify.is_empty());
    let wall = rune_provider::mock::wall_clock();
    assert!(!crate::account::server_notify(&host.ui, &host.store, taller).muted(wall));
    host.answer_account(from);
    assert!(crate::account::server_notify(&host.ui, &host.store, taller).muted(wall));
    let waiting = (true, 2);
    assert_eq!(
        crate::server_menu::rail_marks(&host.ui, &host.store, taller, waiting, wall),
        (false, 2, true)
    );
    // @everyone, now settable, stops alerting.
    let from = host.sent.len();
    host.ac_choose(&format!("sm:flag:{taller}:everyone"));
    host.answer_account(from);
    assert!(host.store.suppresses_everyone(Some(taller)));
    // A channel's mute goes the same way.
    let general = host.open_channel("general");
    let from = host.sent.len();
    host.ac_choose(&format!("cl:mute:{general}:1h"));
    assert!(matches!(
        host.sent.last(),
        Some(Command::SetNotifications {
            server: Some(server),
            change: NotificationChange::ChannelMute(channel, Some(_)),
        }) if *server == taller && *channel == general
    ));
    host.answer_account(from);
    let channel = host.store.channel(general).unwrap().clone();
    assert!(crate::channel_list::effective(&host.ui, &host.store, &channel, wall).1);
}

#[test]
fn folders_show_on_the_rail_open_and_are_changed_from_their_menus() {
    let mut host = Host::new();
    host.ac_load();
    let (cafe, club) = (
        host.ac_server("Café Nocturno"),
        host.ac_server("Club de Fotografía"),
    );
    let controls = host.ac_controls();
    assert!(controls.contains(&"folder:1".to_owned()));
    assert!(!controls.contains(&format!("server:{cafe}")), "closed");
    host.click("folder:1");
    let controls = host.ac_controls();
    assert!(controls.contains(&format!("server:{cafe}")));
    assert!(controls.contains(&format!("server:{club}")));
    // A server out of its folder, by its menu.
    host.ac_right_click(&format!("server:{club}"));
    let from = host.sent.len();
    host.ac_choose(&format!("rail:out:{club}"));
    let Some(Command::SetServerFolders(layout)) = host.sent.last().cloned() else {
        panic!("{:?}", host.sent.last())
    };
    assert_eq!(
        layout.last(),
        Some(&ServerFolder::single(club)),
        "right after its folder"
    );
    host.answer_account(from);
    assert!(host.ui.account.menu.is_none());
    // A new folder with a server alone, then named and colored.
    let taller = host.ac_server("Taller de Rust");
    host.ac_right_click(&format!("server:{taller}"));
    let from = host.sent.len();
    host.ac_choose(&format!("rail:into:{taller}:new"));
    host.answer_account(from);
    let made = host
        .store
        .folders()
        .unwrap()
        .iter()
        .find(|folder| folder.servers == [taller])
        .and_then(|folder| folder.id)
        .expect("a new folder");
    assert!(
        host.ui.account.open_folders.contains(&made),
        "it shows open"
    );
    host.ac_right_click(&format!("folder:{made}"));
    host.ac_choose(&format!("rail:edit:{made}"));
    host.type_text("Rust");
    host.click(&format!("{}E05561", folder_ids::COLOR));
    let from = host.sent.len();
    host.click(folder_ids::SAVE);
    host.answer_account(from);
    let renamed = host
        .store
        .folders()
        .unwrap()
        .iter()
        .find(|folder| folder.id == Some(made))
        .unwrap()
        .clone();
    assert_eq!(renamed.name.as_deref(), Some("Rust"));
    assert_eq!(renamed.color, Some(rune_model::Rgb(0xE05561)));
}

#[test]
fn folders_cannot_change_while_the_provider_may_not() {
    let mut host = Host::new();
    host.ac_load();
    host.store.connection = rune_provider::ConnectionState::Connecting;
    let taller = host.ac_server("Taller de Rust");
    host.ac_right_click(&format!("server:{taller}"));
    let from = host.sent.len();
    host.ac_choose(&format!("rail:down:server:{taller}"));
    assert_eq!(host.sent.len(), from, "a denied item sends nothing");
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title == "No se pudieron cambiar las carpetas"));
}

#[test]
fn messaging_privacy_is_shown_and_changed_from_the_settings() {
    let mut host = Host::new();
    host.ac_load();
    host.click("me:settings");
    host.ac_choose("us:page=privacy");
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "Mensajes directos y solicitudes"));
    let from = host.sent.len();
    host.ac_choose("us:privacy:server-dms");
    assert_eq!(
        host.sent.last(),
        Some(&Command::SetMessagingPrivacy(PrivacyChange::ServerDms(
            false
        )))
    );
    host.answer_account(from);
    assert!(!host.store.messaging_privacy().unwrap().server_dms);
    // One server's members are let write again.
    let club = host.ac_server("Club de Fotografía");
    let from = host.sent.len();
    host.ac_choose(&format!("us:privacy:dms-in:{club}"));
    host.answer_account(from);
    assert!(host.store.messaging_privacy().unwrap().dms_in(club));
}

#[test]
fn a_note_is_read_with_the_profile_and_kept_with_enter() {
    let mut host = Host::new();
    let ada = UserId(10);
    let from = host.sent.len();
    let asked = crate::profile::request_profile(&mut host.ui, &mut host.store, ada, None);
    host.run(asked);
    assert!(host.sent[from..].contains(&Command::LoadNote { user: ada }));
    for event in host.provider.manage(Command::LoadNote { user: ada }) {
        host.event(event);
    }
    assert_eq!(
        host.store.note(ada),
        Some("Conocimos a Ada en la RustConf.")
    );
    let field = crate::account::note_field(&host.ui, &host.store, ada).unwrap();
    assert_eq!(field.text, "Conocimos a Ada en la RustConf.");
    assert!(!field.loading);
    // Asked once only.
    let again = crate::account::load_note(&mut host.ui, &host.store, ada);
    assert!(again.is_empty());
    let action = UiAction::Submitted {
        id: "pp:note:10".into(),
        value: " de la RustConf ".into(),
    };
    let intents = crate::update(&mut host.ui, &mut host.store, &action, host.now);
    assert_eq!(
        intents,
        vec![Intent::Command(Command::SetNote {
            user: ada,
            note: "de la RustConf".into()
        })]
    );
    // Never on the account's own profile.
    let me = host.store.me().unwrap();
    assert!(crate::account::note_field(&host.ui, &host.store, me).is_none());
}

#[test]
fn the_rail_offers_a_servers_menu_without_folders_and_invites_from_it() {
    let mut host = Host::new();
    host.ac_load();
    if let Some(info) = host.store.provider.as_mut() {
        info.capabilities.server_folders = false;
    }
    let lab = host.ac_server("Laboratorio limn");
    host.ac_right_click(&format!("server:{lab}"));
    assert!(host.ui.account.menu.is_some(), "a server's menu opens");
    let buttons = host.ac_controls();
    assert!(
        buttons.iter().all(|id| !id.starts_with("rail:")),
        "no folder entries without folders: {buttons:?}"
    );
    assert!(buttons.contains(&format!("sm:invite:{lab}")));
    assert_eq!(
        buttons.contains(&format!("sm:read:{lab}")),
        host.store.server_unread(lab).0,
        "reads are offered while something waits"
    );
    // The invitation is made, and its link copied with a notice.
    let from = host.sent.len();
    host.ac_choose(&format!("sm:invite:{lab}"));
    assert!(matches!(
        host.sent.last(),
        Some(Command::CreateInvite { server, .. }) if *server == lab
    ));
    host.answer_account(from);
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title == "Enlace de invitación copiado"));
}
