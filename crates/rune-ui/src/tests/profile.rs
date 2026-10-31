//! The account's popout and status, its custom status, the menus of the
//! panel's arrows, the profile editor and the cards of people, driven as a
//! person would, with the simulation answering what they ask of it.

use rune_components::profile::{ids, CardMenu, FullTab, ABOUT_MAX};
use rune_components::voice::MenuEntry;
use rune_model::{Member, Rgb, Status};
use rune_provider::{Failure, ProfileEdit};

use super::*;

impl Host {
    /// Have the simulation answer the status and profile commands sent from
    /// `from` on, and apply what it reports.
    fn pf_answer(&mut self, from: usize) {
        let mut next = from;
        while next < self.sent.len() {
            let command = self.sent[next].clone();
            next += 1;
            if matches!(
                command,
                Command::SetStatus(_)
                    | Command::SetCustomStatus { .. }
                    | Command::EditProfile(_)
                    | Command::EditServerProfile { .. }
                    | Command::LoadProfile { .. }
            ) {
                for event in self.provider.manage(command) {
                    self.event(event);
                }
            }
        }
    }

    /// Put the pointer over the control `id`, as a person does to open a submenu.
    fn pf_hover(&mut self, id: &str) {
        let frame = self.frame();
        let region = frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is not on screen"));
        let center = Point::new(
            region.rect.x + region.rect.width / 2.0,
            region.rect.y + region.rect.height / 2.0,
        );
        self.send(UiEvent::PointerMove(center));
    }

    fn pf_shows(&mut self, id: &str) -> bool {
        self.frame().regions.iter().any(|region| region.id == id)
    }

    /// Let the wall clock reach `wall`: the status timers that are due run.
    fn pf_tick(&mut self, wall: Timestamp) {
        let intents = tick(&mut self.ui, &self.store, wall);
        self.run(intents);
    }

    /// Replace what the field `id` holds, as typing would.
    fn pf_input(&mut self, id: &str, value: &str) {
        self.tree.set_input_value(id, value.to_owned());
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::InputChanged {
                id: id.into(),
                value: value.into(),
            },
            self.now,
        );
        self.run(intents);
    }

    /// Report the action `id` as the tree would, wherever its control is.
    fn pf_act(&mut self, id: &str) {
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::Clicked { id: id.into() },
            self.now,
        );
        self.run(intents);
    }

    fn pf_status(&self) -> Status {
        self.store.presence(self.store.me().unwrap()).status
    }
}

#[test]
fn pf_the_popout_opens_from_the_panel_and_its_menu_sets_the_status() {
    let mut host = Host::new();
    host.click("me:status");
    assert!(host.ui.status_menu);
    assert!(
        !host
            .painted()
            .iter()
            .any(|text| text == "Copiar ID del usuario"),
        "the ID only in developer mode"
    );
    host.ui.prefs.developer = true;
    let painted = host.painted();
    for expected in ["Editar perfil", "Copiar ID del usuario", "En línea"] {
        assert!(
            painted.iter().any(|text| text == expected),
            "{expected}: {painted:?}"
        );
    }
    // It floats above the panel, its left edge on the panel's.
    let frame = host.frame();
    let rect = |id: &str| {
        frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id)
            .unwrap()
            .rect
    };
    let (surface, account) = (rect("popover_surface:pp:dismiss"), rect("me:status"));
    assert_eq!(surface.width, rune_components::profile::WIDTH);
    assert!(
        surface.y + surface.height < account.y,
        "{surface:?} {account:?}"
    );
    assert!(
        surface.x < account.x
            && (surface.x - rune_components::chrome::USER_PANEL_MARGIN.0).abs() < 1.0,
        "{surface:?} {account:?}"
    );
    assert!(!host.pf_shows("pp:status=dnd"));
    host.click(ids::STATUS_MENU);
    assert!(
        host.pf_shows("pp:status=dnd"),
        "the status row opens the menu"
    );
    let from = host.sent.len();
    host.click("pp:status=dnd");
    assert_eq!(
        host.sent.last(),
        Some(&Command::SetStatus(Status::DoNotDisturb))
    );
    assert!(!host.ui.status_menu, "choosing closes the popout");
    assert!(host.ui.profiles.timed.is_none(), "until changed");
    assert_eq!(
        host.pf_status(),
        Status::Online,
        "nothing before the answer"
    );
    host.pf_answer(from);
    assert_eq!(host.pf_status(), Status::DoNotDisturb);
    assert!(host.painted().iter().any(|text| text == "No molestar"));
    // Copying the id says so.
    host.click("me:status");
    host.click(ids::COPY_ID);
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title == "ID copiado"));
}

#[test]
fn pf_the_status_menu_opens_on_hover_and_stays_while_its_durations_are_used() {
    let mut host = Host::new();
    host.click("me:status");
    assert!(!host.pf_shows("pp:status=idle"));
    host.pf_hover(ids::STATUS_MENU);
    assert!(host.pf_shows("pp:status=idle"), "the pointer opens it");
    assert!(!host.ui.profiles.status_open, "without a click");
    host.pf_hover("pp:status=idle");
    assert!(host.pf_shows("pp:status=idle:1h"), "its durations open too");
    host.pf_hover("pp:status=idle:1h");
    assert!(
        host.pf_shows("pp:status=dnd") && host.pf_shows("pp:status=idle:1h"),
        "the status menu stays open while a duration is pointed at"
    );
    host.click("pp:status=idle:1h");
    assert_eq!(host.sent.last(), Some(&Command::SetStatus(Status::Idle)));
}

#[test]
fn pf_a_status_for_a_while_goes_back_when_its_time_is_up_and_is_kept() {
    let mut host = Host::new();
    host.click("me:status");
    host.click(ids::STATUS_MENU);
    host.pf_hover("pp:status=idle");
    assert!(
        host.pf_shows("pp:status=idle:1h"),
        "its durations open on hover"
    );
    let from = host.sent.len();
    host.click("pp:status=idle:1h");
    assert_eq!(host.sent.last(), Some(&Command::SetStatus(Status::Idle)));
    let timed = host.ui.profiles.timed.expect("a timer runs");
    assert_eq!((timed.status, timed.then), (Status::Idle, Status::Online));
    host.pf_answer(from);
    // The popout says until when.
    host.click("me:status");
    assert!(host.painted().iter().any(|text| text.starts_with("hasta ")));
    host.key(Key::Escape, false, false);
    assert!(!host.ui.status_menu, "Escape closes the popout");
    // Kept for the next run.
    let kept = host.ui.settings_to_keep();
    let mut again = Ui::new();
    again.restore(|key| {
        kept.iter()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| value.clone())
    });
    assert_eq!(again.profiles.timed, Some(timed));
    // Not yet: nothing happens.
    let sent = host.sent.len();
    host.pf_tick(Timestamp(timed.until.0 - 1));
    assert_eq!(host.sent.len(), sent);
    // Due: the status it replaced comes back, and the timer goes.
    host.pf_tick(timed.until);
    assert_eq!(host.sent.last(), Some(&Command::SetStatus(Status::Online)));
    assert!(host.ui.profiles.timed.is_none());
    host.pf_answer(sent);
    assert_eq!(host.pf_status(), Status::Online);
    assert_eq!(next_tick(&host.ui, timed.until), None);
}

#[test]
fn pf_due_status_timers_clear_locally_when_their_capabilities_were_revoked() {
    let mut host = Host::new();
    host.pf_act("pp:status=online:1h");
    let timed = host.ui.profiles.timed.unwrap();
    host.ui.profiles.custom_until = Some(timed.until);
    let before = host.sent.len();
    let notices = host.ui.notices.len();
    let capabilities = &mut host.store.provider.as_mut().unwrap().capabilities;
    capabilities.presence = false;
    capabilities.custom_status = false;

    let intents = crate::profile::tick(&mut host.ui, &host.store, timed.until);
    assert!(
        intents
            .iter()
            .all(|intent| !matches!(intent, Intent::Command(_))),
        "expired timers must revalidate the current capabilities: {intents:?}"
    );
    for key in ["status_timer", "custom_status_until"] {
        assert!(intents.iter().any(|intent| {
            matches!(intent, Intent::SaveSetting { key: actual, value: None } if actual == key)
        }));
    }
    host.run(intents);
    assert_eq!(host.sent.len(), before);
    assert_eq!(host.ui.notices.len(), notices);
    assert!(host.ui.profiles.timed.is_none());
    assert!(host.ui.profiles.custom_until.is_none());
    assert_eq!(next_tick(&host.ui, timed.until), None);
    assert_eq!(host.pf_status(), Status::Online);
}

#[test]
fn pf_restored_status_timers_do_not_send_unsupported_commands_on_ready() {
    let mut host = Host::new();
    let until = crate::server_menu::wall_now().plus_minutes(60);
    let saved_timer = format!("idle:{}:online", until.0);
    host.ui.restore(|key| match key {
        "status_timer" => Some(saved_timer.clone()),
        "custom_status_until" => Some(until.0.to_string()),
        _ => None,
    });
    let timed = host
        .ui
        .profiles
        .timed
        .expect("a persisted timer was restored");
    let capabilities = &mut host.store.provider.as_mut().unwrap().capabilities;
    capabilities.presence = false;
    capabilities.custom_status = false;

    let intents =
        crate::profile::on_applied(&mut host.ui, &host.store, &rune_state::Applied::Ready);
    assert!(
        intents.is_empty(),
        "Ready must not reapply a status the provider does not support: {intents:?}"
    );
    assert_eq!(host.ui.profiles.timed, Some(timed));
    assert_eq!(host.ui.profiles.custom_until, Some(until));

    // Restoring support before expiry allows the original schedule to run.
    host.store.provider.as_mut().unwrap().capabilities.presence = true;
    let intents =
        crate::profile::on_applied(&mut host.ui, &host.store, &rune_state::Applied::Ready);
    assert!(intents
        .iter()
        .any(|intent| { matches!(intent, Intent::Command(Command::SetStatus(Status::Idle))) }));
    assert_eq!(host.ui.profiles.timed, Some(timed));

    // An already elapsed setting is consumed on Ready without attempting a write.
    host.store.provider.as_mut().unwrap().capabilities.presence = false;
    host.ui.restore(|key| match key {
        "status_timer" => Some("idle:1:online".into()),
        "custom_status_until" => Some("1".into()),
        _ => None,
    });
    let intents =
        crate::profile::on_applied(&mut host.ui, &host.store, &rune_state::Applied::Ready);
    assert!(intents
        .iter()
        .all(|intent| !matches!(intent, Intent::Command(_))));
    assert!(host.ui.profiles.timed.is_none());
    assert!(host.ui.profiles.custom_until.is_none());
}

#[test]
fn pf_status_timer_capabilities_are_independent() {
    for presence in [false, true] {
        let mut host = Host::new();
        let until = Timestamp(1_790_000_200_000);
        host.ui.profiles.timed = Some(crate::profile::Timed {
            status: Status::Online,
            until,
            then: Status::Idle,
        });
        host.ui.profiles.custom_until = Some(until);
        let capabilities = &mut host.store.provider.as_mut().unwrap().capabilities;
        capabilities.presence = presence;
        capabilities.custom_status = !presence;
        let intents = crate::profile::tick(&mut host.ui, &host.store, until);
        let commands: Vec<_> = intents
            .iter()
            .filter_map(|intent| match intent {
                Intent::Command(command) => Some(command),
                _ => None,
            })
            .collect();
        let expected = if presence {
            Command::SetStatus(Status::Idle)
        } else {
            Command::SetCustomStatus {
                text: None,
                emoji: None,
                until: None,
            }
        };
        assert_eq!(commands, vec![&expected]);
        assert!(host.ui.profiles.timed.is_none());
        assert!(host.ui.profiles.custom_until.is_none());
    }
}

#[test]
fn pf_the_custom_status_dialog_sets_it_and_the_panel_shows_it_once_answered() {
    let mut host = Host::new();
    host.click("me:status");
    host.click(ids::CUSTOM);
    assert!(host.ui.profiles.custom.is_some() && !host.ui.status_menu);
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "Establecer tu estado"));
    host.type_text("de viaje");
    assert_eq!(host.ui.profiles.custom.as_ref().unwrap().text, "de viaje");
    host.click(ids::CUSTOM_EMOJI);
    host.click(&format!("{}🚀", ids::CUSTOM_EMOJI_PICK));
    host.click(ids::CUSTOM_EXPIRY);
    host.click(&format!("{}1h", ids::CUSTOM_EXPIRY_PICK));
    // The preview says it as it is written.
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("🚀 de viaje")));
    let from = host.sent.len();
    host.click(ids::CUSTOM_SAVE);
    assert_eq!(
        host.sent.last(),
        Some(&Command::SetCustomStatus {
            text: Some("de viaje".into()),
            emoji: Some("🚀".into()),
            until: host.ui.profiles.custom_until,
        })
    );
    assert!(host.ui.profiles.custom.is_none());
    assert!(
        !host.painted().iter().any(|text| text == "🚀 de viaje"),
        "nothing before the answer"
    );
    host.pf_answer(from);
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text == "🚀 de viaje"),
        "the panel shows it: {painted:?}"
    );
    // It goes by itself after the hour chosen.
    let until = host.ui.profiles.custom_until.expect("it clears itself");
    host.pf_tick(until);
    assert_eq!(
        host.sent.last(),
        Some(&Command::SetCustomStatus {
            text: None,
            emoji: None,
            until: None,
        })
    );
    assert!(host.ui.profiles.custom_until.is_none());
}

#[test]
fn pf_a_custom_status_too_long_is_not_sent() {
    let mut host = Host::new();
    host.click("me:status");
    host.click(ids::CUSTOM);
    host.pf_input(ids::CUSTOM_TEXT, &"x".repeat(129));
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("hasta 128 caracteres")));
    let sent = host.sent.len();
    host.key(Key::Enter, false, false);
    host.pf_act(ids::CUSTOM_SAVE);
    assert_eq!(host.sent.len(), sent, "nothing leaves");
    assert!(host.ui.profiles.custom.is_some(), "the dialog stays");
    // Escape closes the menu of when it goes, then the dialog, unsaved.
    host.click(ids::CUSTOM_EXPIRY);
    host.key(Key::Escape, false, false);
    let draft = host.ui.profiles.custom.as_ref().expect("still open");
    assert!(!draft.expiry_open);
    host.key(Key::Escape, false, false);
    assert!(host.ui.profiles.custom.is_none());
    assert_eq!(host.sent.len(), sent);
}

#[test]
fn pf_the_editor_saves_what_changed_and_blocks_a_long_about() {
    let mut host = Host::new();
    host.click("me:status");
    host.click(ids::EDIT);
    assert!(host.ui.profiles.editor.is_some() && !host.ui.status_menu);
    let painted = host.painted();
    for expected in ["Editar perfil", "Vista previa", "Sobre mí", "Pronombres"] {
        assert!(
            painted.iter().any(|text| text == expected),
            "{expected}: {painted:?}"
        );
    }
    // Too long: said under the field and not sent.
    host.pf_input(ids::ABOUT, &"x".repeat(ABOUT_MAX + 1));
    let painted = host.painted();
    assert!(painted.contains(&format!("{}/{ABOUT_MAX}", ABOUT_MAX + 1)));
    assert!(painted
        .iter()
        .any(|text| text.contains("«Sobre mí» puede tener hasta 190")));
    let sent = host.sent.len();
    host.click(ids::SAVE);
    assert_eq!(host.sent.len(), sent, "nothing leaves while it is too long");
    // Fixed, with pronouns and a color: one change with what differs.
    host.pf_input(ids::ABOUT, "Programo en Rust.");
    host.pf_input(ids::PRONOUNS, " elle ");
    host.click("profile:color:3498db");
    assert!(
        host.painted().iter().any(|text| text == "elle"),
        "the preview follows"
    );
    let from = host.sent.len();
    host.click(ids::SAVE);
    assert_eq!(
        host.sent.last(),
        Some(&Command::EditProfile(ProfileEdit {
            display_name: None,
            pronouns: Some(Some("elle".into())),
            about: Some(Some("Programo en Rust.".into())),
            accent: Some(Rgb(0x34_98_DB)),
            avatar: None,
            banner: None,
        }))
    );
    assert!(host.ui.profiles.editor.as_ref().unwrap().saving);
    host.pf_answer(from);
    let me = host.store.me_user().unwrap();
    assert_eq!(me.pronouns.as_deref(), Some("elle"));
    assert_eq!(me.accent, Rgb(0x34_98_DB));
    let editor = host.ui.profiles.editor.as_ref().unwrap();
    assert!(!editor.saving && editor.edit == ProfileEdit::default());
    assert!(!host.pf_shows(ids::SAVE), "nothing left to save");
    // A refusal is said beside the form, not in the corner.
    host.pf_input(ids::NAME, "Otro nombre");
    host.click(ids::SAVE);
    host.event(Event::Failed {
        failure: Failure::Profile,
        reason: "el servicio no responde".into(),
        ambiguous: false,
    });
    assert!(host.ui.notices.is_empty());
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("el servicio no responde")));
    // Leaving with changes is refused until they are reset.
    host.key(Key::Escape, false, false);
    assert!(host.ui.profiles.editor.as_ref().unwrap().nudged);
    host.click(ids::RESET);
    host.click(ids::EDITOR_CLOSE);
    assert!(host.ui.profiles.editor.is_none());
}

#[test]
fn pf_the_editor_opened_over_the_settings_goes_back_to_them() {
    let mut host = Host::new();
    host.click("me:settings");
    assert!(host.ui.settings.is_some());
    host.pf_act(ids::EDIT);
    assert!(host.ui.settings.is_none() && host.ui.profiles.editor.is_some());
    host.key(Key::Escape, false, false);
    assert!(host.ui.profiles.editor.is_none());
    assert_eq!(host.ui.settings, Some(SettingsPage::default()));
}

#[test]
fn pf_the_arrows_open_discords_device_menus_and_choose_there() {
    use rune_components::quick_voice::ids as quick;
    let mut host = Host::new();
    let device = |id: &str, name: &str, default: bool| rune_media::DeviceInfo {
        id: id.into(),
        name: name.into(),
        default,
    };
    // Asked for when they were not looked for yet.
    let asked = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: ids::INPUT.into(),
        },
        host.now,
    );
    assert_eq!(asked, vec![Intent::RefreshDevices]);
    host.ui.audio.devices = Some(Ok(rune_media::Devices {
        backend: "prueba".into(),
        inputs: vec![
            device("mic-a", "Micrófono A", true),
            device("mic-b", "Micrófono B", false),
        ],
        outputs: vec![device("out-a", "Parlantes", true)],
    }));
    let painted = host.painted();
    for text in [
        "Dispositivo de entrada",
        "Predeterminado del sistema",
        "Perfil de entrada",
        "Aislamiento de voz",
        "Volumen de entrada",
        "Ajustes de voz",
    ] {
        assert!(painted.iter().any(|it| it == text), "{text}: {painted:?}");
    }
    // Out of a call there is no level, and without its key no push-to-talk.
    assert!(!painted.iter().any(|it| it == "Nivel de entrada"));
    assert!(!painted.iter().any(|it| it == "Pulsar para hablar"));
    // A device chosen from its list closes the menu, as in Discord.
    host.pf_hover("qv:mic");
    host.click("us:mic=mic-b");
    assert_eq!(host.ui.audio.input.as_deref(), Some("mic-b"));
    assert!(host.ui.profiles.devices.is_none(), "choosing closes it");
    // Every profile it offers is one the voice page keeps.
    for (key, _) in rune_components::quick_voice::PROFILES {
        host.click(ids::INPUT);
        host.pf_hover("qv:profile");
        host.click(&format!("us:profile={key}"));
        assert_eq!(host.ui.prefs.input_profile.key(), key);
        assert!(host.ui.profiles.devices.is_none());
    }
    // With a key for it, push-to-talk is a check box that keeps it open.
    host.ui.prefs.push_to_talk_key = Some("Ctrl+T".into());
    host.click(ids::INPUT);
    host.click(quick::PUSH_TO_TALK);
    assert!(host.ui.prefs.push_to_talk);
    assert!(
        host.ui.profiles.devices.is_some(),
        "a check box keeps it open"
    );
    host.click(quick::VOICE_MODE);
    assert!(!host.ui.prefs.push_to_talk);
    host.click(ids::INPUT);
    assert!(host.ui.profiles.devices.is_none(), "its arrow closes it");
    // The headphones' menu: the output, its volume and the voice settings.
    host.click(ids::OUTPUT);
    assert!(host
        .ui
        .profiles
        .devices
        .is_some_and(|it| it.output && !it.bar));
    let painted = host.painted();
    for text in [
        "Dispositivo de salida",
        "Volumen de salida",
        "Ajustes de voz",
    ] {
        assert!(painted.iter().any(|it| it == text), "{text}: {painted:?}");
    }
    assert!(!painted.iter().any(|it| it == "Perfil de entrada"));
    host.pf_hover("qv:speaker");
    host.click("us:speaker=out-a");
    assert_eq!(host.ui.audio.output.as_deref(), Some("out-a"));
    assert!(host.ui.profiles.devices.is_none());
    host.click(ids::OUTPUT);
    host.click(ids::OUTPUT);
    assert!(host.ui.profiles.devices.is_none(), "its arrow closes it");
    // All the voice settings, from its foot.
    host.click(ids::INPUT);
    host.click(quick::SETTINGS);
    assert_eq!(host.ui.settings, Some(SettingsPage::Voice));
    assert!(host.ui.profiles.devices.is_none());
}

#[test]
fn pf_clicking_an_author_opens_their_card_with_their_roles() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let frame = host.frame();
    let (id, message) = frame
        .regions
        .iter()
        .filter_map(|region| {
            let message = region.id.strip_prefix("pp:name:")?.parse().ok()?;
            Some((region.id.clone(), MessageId(message)))
        })
        .next_back()
        .expect("an author's name is on screen");
    let author = host
        .store
        .timeline(general)
        .unwrap()
        .get(message)
        .unwrap()
        .author;
    // Give them a role, as the service would report it.
    let role = host.store.server(server).unwrap().roles[1].clone();
    host.event(Event::MemberChanged(Member {
        user: author,
        server,
        nick: None,
        roles: vec![role.id],
        ..Default::default()
    }));
    host.click(&id);
    host.answer_profile();
    assert_eq!(host.ui.profiles.author, Some((message, false)));
    let painted = host.painted();
    let name = host.store.display_name(author, Some(server));
    let handle = host.store.user(author).unwrap().name.clone();
    assert!(
        painted.iter().filter(|text| **text == name).count() >= 2,
        "{painted:?}"
    );
    assert!(painted.iter().any(|text| text.starts_with(&handle)));
    assert!(painted.contains(&role.name));
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.profiles.author, None);
    // Their mark opens it too.
    host.click(&format!("pp:avatar:{message}"));
    assert_eq!(host.ui.profiles.author, Some((message, true)));
}

#[test]
fn pf_the_editor_reaches_the_nickname_of_each_server() {
    let mut host = Host::new();
    // Without profiles per server, the server menu's dialog renames.
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .server_profiles = false;
    host.click("me:status");
    host.click(ids::EDIT);
    host.click(ids::SERVERS);
    let server = host.store.servers()[0].id;
    host.click(&format!("sm:profile:{server}"));
    assert!(matches!(
        host.ui.server_menu.dialog,
        Some(Dialog::Nick { server: of, .. }) if of == server
    ));
    let editor = host.ui.profiles.editor.as_ref().expect("the editor stays");
    assert!(!editor.servers_open);
    // Escape closes the dialog, not the editor under it.
    host.key(Key::Escape, false, false);
    assert!(host.ui.server_menu.dialog.is_none());
    assert!(host.ui.profiles.editor.is_some());
}

#[test]
fn pf_read_only_profile_uses_the_real_avatar_in_both_places_and_shows_available_data() {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .edit_profile = false;
    let source = rune_model::RemoteImage {
        url: "https://cdn.example.test/avatar.png".into(),
        kind: Default::default(),
    };
    let mut user = host.store.me_user().unwrap().clone();
    user.avatar = Some(source.clone());
    user.pronouns = Some("elle".into());
    user.about = Some("Programo en Rust.".into());
    host.event(Event::UserChanged(user));
    let image = limn::RasterImage::new(
        "c".repeat(64),
        limn::RasterImageFormat::Png,
        3,
        std::sync::Arc::from("AAAA"),
        "Avatar de prueba",
    )
    .unwrap();
    host.ui.images.insert(source, image.clone());
    host.pf_act(ids::EDIT);
    let frame = host.frame();
    // Avatar applies cover fitting; its presentation flag differs from the
    // cached resource, while all encoded image data remains the same.
    let expected = image.cover();
    let images = frame.commands.iter().filter(|command| {
        matches!(command, limn::DrawCommand::RasterImage { image: actual, .. } if actual == &expected)
    }).count();
    assert_eq!(
        images, 2,
        "the avatar section and profile card share the received image"
    );
    let text = host.painted().join(" ");
    for expected in [
        "Tu perfil",
        "Nombre de usuario",
        "elle",
        "Programo en Rust.",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert!(!text.contains("Tu inicial") && !text.contains("Vista previa"));
    assert!(!frame.regions.iter().any(|region| {
        matches!(
            region.kind,
            limn::HitKind::TextInput | limn::HitKind::TextArea
        ) || region.id.starts_with(ids::COLOR)
            || matches!(region.id.as_str(), ids::SERVERS | ids::SAVE | ids::RESET)
    }));
    assert_eq!(host.tree.focused_id(), Some(ids::EDITOR_CLOSE));
}

#[test]
fn pf_unsupported_editing_ignores_stale_input_and_does_not_trap_the_user_with_unsaved_changes() {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .edit_profile = false;
    host.pf_act(ids::EDIT);
    let before = host.sent.len();
    for id in [ids::NAME, ids::ABOUT, ids::PRONOUNS, ids::COLOR_HEX] {
        host.pf_input(id, "stale input");
    }
    host.pf_act("profile:color:3498db");
    host.pf_act(ids::SERVERS);
    host.pf_act(ids::SAVE);
    let editor = host.ui.profiles.editor.as_ref().unwrap();
    assert_eq!(editor.edit, ProfileEdit::default());
    assert!(!editor.servers_open && !editor.saving);
    assert_eq!(host.sent.len(), before);
    host.key(Key::Escape, false, false);
    assert!(host.ui.profiles.editor.is_none());

    // Capabilities can also change after a draft was already written.
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .edit_profile = true;
    host.pf_act(ids::EDIT);
    host.pf_input(ids::NAME, "An unsaved name");
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .edit_profile = false;
    assert!(!host.painted().join(" ").contains("An unsaved name"));
    host.key(Key::Escape, false, false);
    assert!(host.ui.profiles.editor.is_none());
}

#[test]
fn pf_profile_layout_keeps_close_and_save_outside_its_scroll_on_small_windows() {
    for theme in [Theme::Onyx, Theme::Light] {
        for window in [Size::new(320.0, 640.0), Size::new(720.0, 480.0), WINDOW] {
            for editable in [true, false] {
                let mut host = Host::new();
                host.ui.theme = theme;
                host.store
                    .provider
                    .as_mut()
                    .unwrap()
                    .capabilities
                    .edit_profile = editable;
                host.pf_act(ids::EDIT);
                if editable {
                    host.pf_input(ids::NAME, "Nombre modificado");
                }
                let mut context = host.context();
                context.window = window;
                host.tree.set_root(view(&host.ui, &host.store, &context));
                let frame = host.tree.layout(window);
                let rect = |frame: &Frame, id: &str| {
                    frame
                        .regions
                        .iter()
                        .find(|region| region.id == id)
                        .unwrap_or_else(|| {
                            panic!("{id}: {theme:?}, {window:?}, editable={editable}")
                        })
                        .rect
                };
                let close = rect(&frame, ids::EDITOR_CLOSE);
                let scroll = rect(&frame, "profile:form");
                assert!(close.x >= 0.0 && close.x + close.width <= window.width);
                assert!(close.y >= 0.0 && close.y + close.height <= scroll.y);
                if editable {
                    let save = rect(&frame, ids::SAVE);
                    let reset = rect(&frame, ids::RESET);
                    assert!(save.x + save.width <= window.width && reset.x >= 0.0);
                    assert!(reset.x + reset.width < save.x);
                    assert!(save.y >= scroll.y + scroll.height);
                    assert!(save.y + save.height <= window.height);
                }
                host.tree.scroll_to_end("profile:form");
                let end = host.tree.layout(window);
                assert_eq!(
                    rect(&end, ids::EDITOR_CLOSE),
                    close,
                    "close must not scroll away"
                );
                for region in &end.regions {
                    if region.kind == limn::HitKind::TextInput
                        || region.kind == limn::HitKind::TextArea
                    {
                        assert!(
                            region.rect.x >= 0.0
                                && region.rect.x + region.rect.width <= window.width,
                            "input escapes: {region:?}"
                        );
                    }
                }
                if editable {
                    assert_eq!(
                        rect(&end, ids::SAVE),
                        rect(&frame, ids::SAVE),
                        "save must not scroll away"
                    );
                }
            }
        }
    }
}

#[test]
fn pf_missing_profile_fields_are_unknown_and_not_claimed_to_be_empty_on_the_service() {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .edit_profile = false;
    let mut user = host.store.me_user().unwrap().clone();
    user.pronouns = None;
    user.about = None;
    host.event(Event::UserChanged(user));
    host.pf_act(ids::EDIT);
    let text = host.painted();
    assert_eq!(
        text.iter()
            .filter(|line| line.as_str() == "No disponible")
            .count(),
        2
    );
    assert!(!text
        .iter()
        .any(|line| line == "Agrega tus pronombres" || line == "Cuenta algo sobre ti."));
}

#[test]
fn pf_profile_banner_arrival_paints_received_artwork_and_removal_restores_the_color() {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .edit_profile = false;
    let source = rune_model::RemoteImage {
        url: "https://cdn.example.test/banner.png".into(),
        kind: rune_model::ImageKind::Banner,
    };
    let mut user = host.store.me_user().unwrap().clone();
    user.banner = rune_model::ProfileBanner::Image(source.clone());
    host.event(Event::UserChanged(user.clone()));
    host.pf_act(ids::EDIT);
    let before = host.frame();
    assert!(!before
        .commands
        .iter()
        .any(|command| matches!(command, limn::DrawCommand::RasterImage { .. })));
    let image = limn::RasterImage::new(
        "b".repeat(64),
        limn::RasterImageFormat::Png,
        3,
        std::sync::Arc::from("AAAA"),
        "Banner de prueba",
    )
    .unwrap();
    host.ui.images.insert(source, image.clone());
    let loaded = host.frame();
    let rects: Vec<_> = loaded
        .commands
        .iter()
        .filter_map(|command| match command {
            limn::DrawCommand::RasterImage {
                image: actual,
                rect,
                ..
            } if actual == &image.clone().cover() => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(rects.len(), 1);
    assert!((rects[0].width - rune_components::profile::WIDTH).abs() < 1.0);
    assert_eq!(rects[0].height, 105.0);
    let close = |frame: &Frame| {
        frame
            .regions
            .iter()
            .find(|region| region.id == ids::EDITOR_CLOSE)
            .unwrap()
            .rect
    };
    assert_eq!(close(&before), close(&loaded));
    assert!(host
        .sent
        .iter()
        .all(|command| !matches!(command, Command::EditProfile(_))));
    user.banner = rune_model::ProfileBanner::Absent;
    host.event(Event::UserChanged(user));
    let removed = host.frame();
    assert!(!removed
        .commands
        .iter()
        .any(|command| matches!(command, limn::DrawCommand::RasterImage { .. })));
    assert_eq!(close(&before), close(&removed));
}

#[test]
fn pf_a_timeout_says_until_when_in_the_composer_and_lifts_by_itself() {
    let mut host = Host::new();
    let charla = host.open_channel("charla");
    let server = host.store.channel(charla).unwrap().server;
    let until = Timestamp(1_790_000_100_000).plus_minutes(95);
    let expected = format!(
        "Estás aislado hasta {} · quedan 1 h 35 min",
        rune_components::profile::until_label(until, host.store.wall())
    );
    assert_eq!(
        crate::profile::timeout_notice(&host.store, server).as_deref(),
        Some(expected.as_str())
    );
    let painted = host.painted().join(" ");
    assert!(painted.contains("Estás aislado hasta"), "{painted}");
    // Its end lifts it with no event: the store reads its clock.
    host.store.clock = Some(|| Timestamp(1_790_000_100_000).plus_minutes(96));
    assert_eq!(crate::profile::timeout_notice(&host.store, server), None);
    assert!(host.store.abilities(charla).unwrap().write.allowed());
    assert!(!host.painted().join(" ").contains("Estás aislado"));
}

#[test]
fn pf_a_members_card_loads_their_profile_once_and_ignores_stale_answers() {
    use rune_model::UserProfile;
    let mut host = Host::new();
    host.ui.members_open = true;
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let frame = host.frame();
    let user = frame
        .regions
        .iter()
        .filter_map(|region| region.id.strip_prefix("member:")?.parse().ok())
        .map(rune_model::UserId)
        .find(|user| Some(*user) != host.store.me())
        .expect("someone is listed");
    host.click(&format!("member:{user}"));
    let Some(Command::LoadProfile {
        request,
        user: asked,
        server: scope,
        ..
    }) = host.sent.last().cloned()
    else {
        panic!(
            "opening the card asks for the profile: {:?}",
            host.sent.last()
        );
    };
    assert_eq!((asked, scope), (user, Some(server)));
    // An answer to another request never reaches the card.
    host.event(Event::ProfileLoaded {
        request: request - 1,
        profile: UserProfile {
            user,
            server: Some(server),
            mutual_servers: Some(Vec::new()),
            ..Default::default()
        },
    });
    assert!(!host.painted().iter().any(|t| t.contains("en común")));
    for event in host.provider.manage(Command::LoadProfile {
        request,
        user,
        server: Some(server),
        effect: true,
    }) {
        host.event(event);
    }
    let painted = host.painted();
    assert!(
        painted.iter().any(|t| t.contains("en común")),
        "{painted:?}"
    );
    // The dates are the full profile's: the card shows its mutuals instead.
    assert!(!painted.iter().any(|t| t == "Miembro desde"));
    // Opened again, the card uses what was loaded (only who is watched
    // changes as it closes and opens).
    let sent = host.sent.len();
    host.click(&format!("member:{user}"));
    host.click(&format!("member:{user}"));
    assert!(host.sent[sent..]
        .iter()
        .all(|command| matches!(command, Command::WatchPeople { .. })));
    // The full profile is in the «…» menu of the card now.
    host.click(&format!("{}{user}", ids::CARD_MORE));
    host.pf_act(&format!("{}{user}", ids::FULL));
    assert_eq!(host.ui.profiles.full, Some((user, Some(server))));
    let painted = host.painted();
    let name = host.store.server(server).unwrap().name.clone();
    assert!(painted.contains(&format!("En {name}")), "{painted:?}");
    // The tabs say how many once known; the servers' tab lists them.
    assert!(painted
        .iter()
        .any(|t| t.starts_with("Servidores en común · ")));
    host.click(&format!("{}servers", ids::FULL_TAB));
    let painted = host.painted();
    assert!(painted.contains(&name), "{painted:?}");
    assert!(
        !painted.contains(&format!("En {name}")),
        "«Sobre mí» is left"
    );
    assert!(!painted
        .iter()
        .any(|t| t == "No disponible en esta conexión."));
    // Opening a profile again starts on «Sobre mí».
    host.pf_act(ids::FULL_CLOSE);
    host.click(&format!("member:{user}"));
    // The full profile is in the «…» menu of the card now.
    host.click(&format!("{}{user}", ids::CARD_MORE));
    host.pf_act(&format!("{}{user}", ids::FULL));
    assert_eq!(host.ui.profiles.full_tab, FullTab::About);
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.profiles.full, None, "Escape closes it");
}

#[test]
fn pf_a_loaded_profile_shows_what_was_set_for_the_server_and_a_failed_one_stays_quiet() {
    use rune_model::UserProfile;
    let mut host = Host::new();
    host.ui.members_open = true;
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let frame = host.frame();
    let mut listed = frame
        .regions
        .iter()
        .filter_map(|region| region.id.strip_prefix("member:")?.parse().ok())
        .map(rune_model::UserId)
        .filter(|user| Some(*user) != host.store.me());
    let (user, other) = (listed.next().unwrap(), listed.next().unwrap());
    host.click(&format!("member:{user}"));
    let Some(Command::LoadProfile { request, .. }) = host.sent.last().cloned() else {
        panic!("the card asks for the profile");
    };
    host.event(Event::ProfileLoaded {
        request,
        profile: UserProfile {
            user,
            server: Some(server),
            about: Some("Texto de la cuenta".into()),
            server_about: Some("Sólo en este servidor".into()),
            ..Default::default()
        },
    });
    let painted = host.painted();
    assert!(
        painted.iter().any(|t| t.contains("Sólo en este servidor")),
        "{painted:?}"
    );
    assert!(!painted.iter().any(|t| t.contains("Texto de la cuenta")));
    // A click outside the card closes it first.
    host.key(Key::Escape, false, false);
    host.click(&format!("member:{other}"));
    let Some(Command::LoadProfile { request, .. }) = host.sent.last().cloned() else {
        panic!("another card asks for its profile");
    };
    host.event(Event::Failed {
        failure: rune_provider::Failure::ProfileLoad {
            request,
            user: other,
        },
        reason: "Discord no pudo confirmar esta acción.".into(),
        ambiguous: false,
    });
    assert!(host.ui.notices.is_empty(), "the card says it, not a toast");
    assert!(!host.store.profile_loading(other));
}

/// A picture as the host prepares it: a bounded PNG and its preview.
fn pf_picture() -> (rune_provider::ProfileImage, limn::RasterImage) {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(64u32.to_be_bytes());
    png.extend(64u32.to_be_bytes());
    let preview = limn::RasterImage::new(
        "d".repeat(64),
        limn::RasterImageFormat::Png,
        3,
        std::sync::Arc::from("AAAA"),
        "Vista previa",
    )
    .unwrap();
    (
        rune_provider::ProfileImage::new(png.into()).unwrap(),
        preview,
    )
}

/// The whole of a picture chosen, twice as wide as tall (edicion-de-perfil).
fn pf_source(animated: bool) -> crate::PictureSource {
    crate::PictureSource {
        image: pf_picture().1,
        width: 400,
        height: 200,
        animated,
    }
}

/// The frames asked of the host since `from`.
fn pf_frames(host: &Host, from: usize) -> Vec<(u64, rune_provider::Crop, bool)> {
    host.upload_intents[from..]
        .iter()
        .filter_map(|intent| match intent {
            Intent::FrameProfileImage {
                request,
                crop,
                animated,
                ..
            } => Some((*request, *crop, *animated)),
            _ => None,
        })
        .collect()
}

#[test]
fn pf_a_chosen_picture_is_framed_by_dragging_over_it_and_brought_closer() {
    let mut host = Host::new();
    host.click("me:status");
    host.click(ids::EDIT);
    host.click(ids::AVATAR_PICK);
    let Some(Intent::PickProfileImage {
        request, animated, ..
    }) = host.upload_intents.last().cloned()
    else {
        panic!("{:?}", host.upload_intents);
    };
    assert!(animated, "the simulated plan allows a moving avatar");
    let (image, preview) = pf_picture();
    profile_image_picked(
        &mut host.ui,
        request,
        Ok(Some((image.clone(), preview.clone()))),
        Some(pf_source(false)),
        host.now,
    );
    host.tree.scroll_to_end("profile:form");
    assert!(host.pf_shows(ids::FRAME) && host.pf_shows(ids::FRAME_ZOOM));
    let act = |host: &mut Host, action: UiAction| {
        let intents = update(&mut host.ui, &mut host.store, &action, host.now);
        host.run(intents);
    };
    // Dragging moves the frame here; only letting go asks for it.
    let from = host.upload_intents.len();
    let at = |x: f32, committed: bool| UiAction::AreaSliderChanged {
        id: ids::FRAME.into(),
        x,
        y: 0.5,
        committed,
    };
    act(&mut host, at(0.2, false));
    assert!(pf_frames(&host, from).is_empty());
    act(&mut host, at(0.0, true));
    let left = rune_provider::Crop {
        x: 0.0,
        y: 0.5,
        zoom: 1.0,
    };
    let [(first, crop, moving)] = pf_frames(&host, from)[..] else {
        panic!("{:?}", host.upload_intents);
    };
    assert!(
        crop == left && !moving,
        "a still picture asks for no motion"
    );
    // Closer, while that frame is on its way: the newer one replaces it.
    act(
        &mut host,
        UiAction::SliderChanged {
            id: ids::FRAME_ZOOM.into(),
            fraction: 1.0,
            committed: true,
        },
    );
    let frames = pf_frames(&host, from);
    let (second, crop, _) = frames[1];
    assert_eq!(crop.zoom, rune_provider::Crop::MAX_ZOOM);
    // Saving waits for it.
    let sent = host.sent.len();
    host.pf_act(ids::SAVE);
    assert_eq!(host.sent.len(), sent);
    let framed = pf_picture();
    profile_image_picked(
        &mut host.ui,
        first,
        Ok(Some(framed.clone())),
        None,
        host.now,
    );
    assert!(
        host.ui.profiles.editor.as_ref().unwrap().picking.is_some(),
        "an older frame"
    );
    let reveal = profile_image_picked(&mut host.ui, second, Ok(Some(framed)), None, host.now);
    assert!(reveal.is_empty(), "a new frame does not scroll");
    let editor = host.ui.profiles.editor.as_ref().unwrap();
    assert!(editor.picking.is_none() && editor.edit.avatar.is_some());
    assert!(editor
        .framing
        .as_ref()
        .is_some_and(|framing| framing.pending.is_none() && framing.crop.zoom == 4.0));
    // Taken away, it is no longer framed.
    host.pf_act(ids::AVATAR_REMOVE);
    assert!(host.ui.profiles.editor.as_ref().unwrap().framing.is_none());
}

#[test]
fn pf_without_the_plan_a_banner_is_not_chosen_and_a_moving_avatar_stays_still() {
    let mut host = Host::new();
    host.event(Event::Perks(rune_model::Perks {
        upgrade: Some("Nitro".into()),
        emoji_anywhere: true,
        ..Default::default()
    }));
    host.click("me:status");
    host.click(ids::EDIT);
    host.tree.scroll_to_end("profile:form");
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Necesitas Nitro para usar un banner")));
    let asked = host.upload_intents.len();
    host.pf_act(ids::BANNER_PICK);
    assert_eq!(
        host.upload_intents.len(),
        asked,
        "before the service refuses it"
    );
    host.click(ids::AVATAR_PICK);
    let Some(Intent::PickProfileImage {
        request, animated, ..
    }) = host.upload_intents.last().cloned()
    else {
        panic!("{:?}", host.upload_intents);
    };
    assert!(!animated);
    // A moving picture came back still: the editor says why.
    profile_image_picked(
        &mut host.ui,
        request,
        Ok(Some(pf_picture())),
        Some(pf_source(true)),
        host.now,
    );
    host.tree.scroll_to_end("profile:form");
    let painted = host
        .painted()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        painted
            .contains("Necesitas Nitro para usar un avatar animado. Se usa sólo el primer cuadro."),
        "{painted}"
    );
}

/// A moving picture as the core makes one: a GIF (edicion-de-perfil).
fn pf_moving() -> (rune_provider::ProfileImage, limn::RasterImage) {
    let mut gif = b"GIF89a".to_vec();
    gif.extend(64u16.to_le_bytes());
    gif.extend(64u16.to_le_bytes());
    (
        rune_provider::ProfileImage::new(gif.into()).unwrap(),
        pf_picture().1,
    )
}

impl Host {
    /// What the editor paints at the top of its form and at its end, one
    /// space between words.
    fn pf_painted(&mut self) -> String {
        self.tree.scroll_to_start("profile:form");
        let mut painted = self.painted();
        self.tree.scroll_to_end("profile:form");
        painted.extend(self.painted());
        painted
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Choose a picture of `kind` and have the host answer with `picture`,
    /// whole as `source`; what the editor asked to keep moving.
    fn pf_choose(
        &mut self,
        pick: &str,
        picture: (rune_provider::ProfileImage, limn::RasterImage),
        source: Option<crate::PictureSource>,
    ) -> bool {
        self.pf_act(pick);
        let Some(Intent::PickProfileImage {
            request, animated, ..
        }) = self.upload_intents.last().cloned()
        else {
            panic!("{:?}", self.upload_intents);
        };
        let intents =
            profile_image_picked(&mut self.ui, request, Ok(Some(picture)), source, self.now);
        self.run(intents);
        animated
    }

    /// Let go of the frame at `x`: the frame asked of the host.
    fn pf_drag(&mut self, x: f32) -> (u64, rune_provider::Crop, bool) {
        let from = self.upload_intents.len();
        let action = UiAction::AreaSliderChanged {
            id: ids::FRAME.into(),
            x,
            y: 0.5,
            committed: true,
        };
        let intents = update(&mut self.ui, &mut self.store, &action, self.now);
        self.run(intents);
        let [frame] = pf_frames(self, from)[..] else {
            panic!("{:?}", self.upload_intents);
        };
        frame
    }
}

#[test]
fn pf_what_the_plan_denies_after_the_choice_is_said_on_saving_and_not_sent() {
    let mut host = Host::new();
    host.click("me:status");
    host.click(ids::EDIT);
    // The plan is not known yet: a banner and a moving avatar may be chosen.
    host.pf_choose(ids::BANNER_PICK, pf_picture(), Some(pf_source(false)));
    assert!(host.pf_choose(ids::AVATAR_PICK, pf_moving(), Some(pf_source(true))));
    // Then it arrives without them.
    host.event(Event::Perks(rune_model::Perks {
        upgrade: Some("Nitro".into()),
        ..Default::default()
    }));
    assert!(host.pf_painted().contains(
        "Necesitas Nitro para usar un avatar animado. Mueve el encuadre para usar sólo el primer cuadro."
    ));
    let sent = host.sent.len();
    host.pf_act(ids::SAVE);
    assert_eq!(host.sent.len(), sent, "before the service refuses it");
    assert!(host
        .pf_painted()
        .contains("No se pudo guardar: Necesitas Nitro para usar un banner"));
    host.pf_act(ids::BANNER_REMOVE);
    host.pf_act(ids::SAVE);
    assert_eq!(host.sent.len(), sent);
    assert!(host
        .pf_painted()
        .contains("No se pudo guardar: Necesitas Nitro para usar un avatar animado"));
    // Framed again, it is asked still; once still, it is saved.
    let (request, _, animated) = host.pf_drag(0.0);
    assert!(!animated);
    let intents = profile_image_picked(
        &mut host.ui,
        request,
        Ok(Some(pf_picture())),
        None,
        host.now,
    );
    host.run(intents);
    assert!(host
        .pf_painted()
        .contains("Necesitas Nitro para usar un avatar animado. Se usa sólo el primer cuadro."));
    host.pf_act(ids::SAVE);
    assert!(matches!(
        &host.sent[sent..],
        [Command::EditProfile(edit)] if edit.avatar.as_ref().is_some_and(|avatar| avatar.as_ref().is_some_and(|avatar| !avatar.animated()))
    ));
}

#[test]
fn pf_too_heavy_to_move_it_stays_still_and_a_failed_frame_goes_back() {
    let mut host = Host::new();
    host.click("me:status");
    host.click(ids::EDIT);
    // Asked to keep moving, it came back still.
    assert!(host.pf_choose(ids::AVATAR_PICK, pf_picture(), Some(pf_source(true))));
    assert!(host.pf_painted().contains(
        "La animación es demasiado larga o pesada para un perfil. Se usa sólo el primer cuadro."
    ));
    // From then on it is framed still; a frame that fails goes back.
    let (request, crop, animated) = host.pf_drag(0.0);
    assert!(!animated && crop.x == 0.0);
    let intents = profile_image_picked(
        &mut host.ui,
        request,
        Err("No se pudo preparar la imagen.".into()),
        None,
        host.now,
    );
    host.run(intents);
    let framing = host.ui.profiles.editor.as_ref().unwrap().framing.as_ref();
    assert!(framing.is_some_and(|framing| {
        framing.crop == rune_provider::Crop::default() && framing.pending.is_none()
    }));
    // A new choice the host could not show whole is not framed, nor is the
    // picture before it.
    host.pf_choose(ids::AVATAR_PICK, pf_picture(), None);
    let editor = host.ui.profiles.editor.as_ref().unwrap();
    assert!(editor.framing.is_none() && editor.edit.avatar.is_some());
}

#[test]
fn pf_a_plan_that_allows_motion_later_leaves_no_stray_note_and_the_next_frame_moves() {
    let mut host = Host::new();
    host.event(Event::Perks(rune_model::Perks {
        upgrade: Some("Nitro".into()),
        ..Default::default()
    }));
    host.click("me:status");
    host.click(ids::EDIT);
    assert!(!host.pf_choose(ids::AVATAR_PICK, pf_picture(), Some(pf_source(true))));
    assert!(host.pf_painted().contains("Se usa sólo el primer cuadro."));
    host.event(Event::Perks(rune_model::Perks {
        upgrade: Some("Nitro".into()),
        animated_avatar: true,
        ..Default::default()
    }));
    assert!(!host.pf_painted().contains("primer cuadro"));
    assert!(host.pf_drag(0.0).2, "the plan allows it now");
}

#[test]
fn pf_the_editor_chooses_a_picture_previews_it_and_saves_it_with_the_profile() {
    let mut host = Host::new();
    host.click("me:status");
    host.click(ids::EDIT);
    // Username and email are never changed here: said beside them.
    host.tree.scroll_to_end("profile:form");
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("contraseña")));
    host.click(ids::AVATAR_PICK);
    let Some(Intent::PickProfileImage { request, kind, .. }) = host.upload_intents.last().cloned()
    else {
        panic!("{:?}", host.upload_intents);
    };
    assert_eq!(kind, rune_provider::ProfileImageKind::Avatar);
    // A second click while the selector is open asks for nothing more.
    let asked = host.upload_intents.len();
    host.pf_act(ids::BANNER_PICK);
    assert_eq!(host.upload_intents.len(), asked);
    // An answer to an older choice is not this one.
    let (image, preview) = pf_picture();
    profile_image_picked(
        &mut host.ui,
        request + 100,
        Ok(Some((image.clone(), preview.clone()))),
        None,
        host.now,
    );
    assert!(host.ui.profiles.editor.as_ref().unwrap().picking.is_some());
    let reveal = profile_image_picked(
        &mut host.ui,
        request,
        Ok(Some((image.clone(), preview.clone()))),
        Some(pf_source(false)),
        host.now,
    );
    assert!(matches!(&reveal[..], [Intent::Reveal { id, .. }] if id == ids::PICTURES));
    let editor = host.ui.profiles.editor.as_ref().unwrap();
    assert!(editor.picking.is_none() && editor.avatar_preview.is_some());
    assert!(host.pf_shows(ids::SAVE), "a picture chosen is a change");
    let from = host.sent.len();
    host.click(ids::SAVE);
    assert_eq!(
        host.sent.last(),
        Some(&Command::EditProfile(ProfileEdit {
            avatar: Some(Some(image)),
            ..ProfileEdit::default()
        }))
    );
    host.pf_answer(from);
    assert!(host.store.me_user().unwrap().avatar.is_some());
    let editor = host.ui.profiles.editor.as_ref().unwrap();
    assert!(!editor.saving && editor.avatar_preview.is_none());
    // Taking it away is a change too; a file that could not be used says why.
    host.click(ids::AVATAR_REMOVE);
    assert_eq!(
        host.ui.profiles.editor.as_ref().unwrap().edit.avatar,
        Some(None)
    );
    host.click(ids::RESET);
    host.click(ids::BANNER_PICK);
    let Some(Intent::PickProfileImage { request, .. }) = host.upload_intents.last().cloned() else {
        panic!();
    };
    profile_image_picked(
        &mut host.ui,
        request,
        Err("La imagen pesa más de 8 MB.".into()),
        None,
        host.now,
    );
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title == "No se pudo usar la imagen"));
    // Closing the editor while choosing tells the host to stop the selector.
    host.click(ids::AVATAR_PICK);
    host.click(ids::EDITOR_CLOSE);
    assert!(matches!(
        host.upload_intents.last(),
        Some(Intent::CancelProfileImage)
    ));
}

#[test]
fn pf_the_editor_changes_the_profile_of_a_server_and_waits_where_restricted() {
    let mut host = Host::new();
    host.click("me:status");
    host.click(ids::EDIT);
    host.click(ids::SERVERS);
    let taller = host
        .store
        .servers()
        .iter()
        .find(|server| server.name == "Taller de Rust")
        .unwrap()
        .id;
    let from = host.sent.len();
    host.click(&format!("{}{taller}", ids::SCOPE));
    assert_eq!(
        host.ui.profiles.editor.as_ref().unwrap().server,
        Some(taller)
    );
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "Perfil en Taller de Rust"));
    // Its «Sobre mí» and banner there are read.
    assert!(host.sent[from..].iter().any(
        |command| matches!(command, Command::LoadProfile { server: Some(s), .. } if *s == taller)
    ));
    host.pf_answer(from);
    host.pf_input(ids::NICK, "Lab");
    host.pf_input(ids::ABOUT, "Aquí pruebo cosas");
    // Going elsewhere with that unsaved is refused, as leaving is.
    host.pf_act(ids::MAIN_PROFILE);
    assert!(host.ui.profiles.editor.as_ref().unwrap().nudged);
    let from = host.sent.len();
    host.click(ids::SAVE);
    assert_eq!(
        host.sent.last(),
        Some(&Command::EditServerProfile {
            server: taller,
            edit: rune_provider::ServerProfileEdit {
                nick: Some(Some("Lab".into())),
                about: Some(Some("Aquí pruebo cosas".into())),
                ..Default::default()
            },
        })
    );
    host.pf_answer(from);
    let me = host.store.me().unwrap();
    assert_eq!(
        host.store.member_of(taller, me).unwrap().nick.as_deref(),
        Some("Lab")
    );
    assert_eq!(
        host.store
            .profile(me, Some(taller))
            .and_then(|profile| profile.server_about.as_deref()),
        Some("Aquí pruebo cosas")
    );
    let editor = host.ui.profiles.editor.as_ref().unwrap();
    assert!(!editor.saving && !editor.dirty(&host.store));
    // Timed out in Café Nocturno: its name and «Sobre mí» there wait, with why.
    let cafe = host
        .store
        .servers()
        .iter()
        .find(|server| server.name == "Café Nocturno")
        .unwrap()
        .id;
    host.click(ids::SERVERS);
    host.click(&format!("{}{cafe}", ids::SCOPE));
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("Estás aislado en Café Nocturno")));
    // Back to the main profile.
    host.pf_act(ids::MAIN_PROFILE);
    assert_eq!(host.ui.profiles.editor.as_ref().unwrap().server, None);
}

/// A member of `server` other than the account, who has no conversation with
/// the account yet (or has one, when `with_conversation`).
fn pf_someone(host: &Host, server: rune_model::ServerId, with_conversation: bool) -> UserId {
    let me = host.store.me().unwrap();
    host.store
        .members(server)
        .iter()
        .map(|member| member.user)
        .find(|user| {
            *user != me
                && host.store.direct_with(*user).is_some() == with_conversation
                && !host.store.user(*user).is_some_and(|account| account.bot)
                && host.store.open_direct(*user).allowed()
        })
        .expect("someone to write to")
}

#[test]
fn pf_a_cards_field_waits_for_a_new_conversation_and_sends_once_it_is_there() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let user = pf_someone(&host, server, false);
    host.ui.profile = Some(user);
    let field = format!("{}{user}", ids::MESSAGE);
    host.pf_input(&field, "hola");
    let from = host.sent.len();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Submitted {
            id: field.clone(),
            value: "hola".into(),
        },
        host.now,
    );
    host.run(intents);
    assert!(
        host.sent[from..].iter().any(|command| matches!(
            command,
            Command::OpenDirect { user: opened, .. } if *opened == user
        )),
        "{:?}",
        host.sent
    );
    // The card keeps its text and stays until the conversation exists.
    assert_eq!(host.ui.profile, Some(user));
    assert_eq!(host.ui.profiles.message_waiting, Some(user));
    host.answer_account(from);
    let channel = host.store.direct_with(user).expect("their conversation");
    assert!(
        host.sent.iter().any(|command| matches!(
            command,
            Command::SendMessage { channel: sent, content, .. }
                if *sent == channel && content == "hola"
        )),
        "{:?}",
        host.sent
    );
    assert_eq!(host.ui.profile, None, "the card closes once it is sent");
    assert_eq!(host.ui.profiles.message, None);
}

#[test]
fn pf_a_cards_field_sends_at_once_to_an_open_conversation_and_empty_text_nothing() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let user = pf_someone(&host, server, true);
    host.ui.profile = Some(user);
    let field = format!("{}{user}", ids::MESSAGE);
    // Empty text does nothing.
    let from = host.sent.len();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Submitted {
            id: field.clone(),
            value: String::new(),
        },
        host.now,
    );
    host.run(intents);
    assert_eq!(host.sent.len(), from);
    assert_eq!(host.ui.profile, Some(user));
    host.pf_input(&field, "  buenas  ");
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Submitted {
            id: field,
            value: "  buenas  ".into(),
        },
        host.now,
    );
    host.run(intents);
    let channel = host.store.direct_with(user).unwrap();
    assert!(
        host.sent[from..].iter().any(|command| matches!(
            command,
            Command::SendMessage { channel: sent, content, .. }
                if *sent == channel && content == "buenas"
        )),
        "{:?}",
        host.sent
    );
    assert_eq!(host.ui.profile, None);
    assert_eq!(host.ui.profiles.message, None);
}

#[test]
fn pf_the_more_menu_shares_and_reports_as_the_provider_allows() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let user = pf_someone(&host, server, true);
    host.ui.profile = Some(user);
    let labels = |host: &Host| -> Vec<String> {
        let card = crate::profile::card_of(
            &host.ui,
            &host.store,
            Some(server),
            user,
            &rune_components::Look::DARK,
        )
        .unwrap();
        card.more
            .unwrap_or_default()
            .into_iter()
            .filter_map(|entry| match entry {
                MenuEntry::Item(_, label, _) | MenuEntry::Danger(_, label) => Some(label),
                _ => None,
            })
            .collect()
    };
    // The simulation has no trusted domain and no report form.
    let shown = labels(&host);
    assert!(
        shown.contains(&"Ver perfil completo".to_owned()),
        "{shown:?}"
    );
    assert!(!shown.contains(&"Compartir perfil".to_owned()), "{shown:?}");
    assert!(
        !shown.contains(&"Denunciar perfil de usuario".to_owned()),
        "{shown:?}"
    );
    let capabilities = &mut host.store.provider.as_mut().unwrap().capabilities;
    capabilities.trusted_links = &["discord.com"];
    capabilities.report_form = Some("https://dis.gd/report");
    let shown = labels(&host);
    assert!(shown.contains(&"Compartir perfil".to_owned()), "{shown:?}");
    assert!(
        shown.contains(&"Denunciar perfil de usuario".to_owned()),
        "{shown:?}"
    );
    // Sharing copies the page's link on that domain.
    let shared = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("{}{user}", ids::SHARE),
        },
        host.now,
    );
    assert!(
        shared.iter().any(|intent| matches!(
            intent,
            Intent::Copy(link) if *link == format!("https://discord.com/users/{user}")
        )),
        "{shared:?}"
    );
    // Reporting copies the ID for the form, and opens the form.
    let reported = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("{}{user}", ids::REPORT),
        },
        host.now,
    );
    assert!(reported.iter().any(|intent| matches!(
        intent,
        Intent::Copy(id) if *id == user.0.to_string()
    )));
    assert!(reported.iter().any(|intent| matches!(
        intent,
        Intent::OpenInBrowser(url) if url == "https://dis.gd/report"
    )));
}

#[test]
fn pf_a_mutual_part_opens_the_full_profile_on_its_tab() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let user = pf_someone(&host, server, true);
    host.ui.profile = Some(user);
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("{}{user}:servers", ids::MUTUAL),
        },
        host.now,
    );
    host.run(intents);
    assert_eq!(host.ui.profiles.full, Some((user, Some(server))));
    assert_eq!(host.ui.profiles.full_tab, FullTab::Servers);
    assert_eq!(host.ui.profile, None, "the full profile replaces the card");
}

#[test]
fn pf_the_roles_plus_opens_its_checklist_and_a_role_keeps_it_open() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    let user = pf_someone(&host, server, true);
    let toggle = format!("{}{user}", ids::CARD_ROLES);
    host.pf_act(&toggle);
    assert_eq!(host.ui.profiles.menu, Some((user, CardMenu::Roles)));
    // The same «+» closes it.
    host.pf_act(&toggle);
    assert_eq!(host.ui.profiles.menu, None);
    host.pf_act(&toggle);
    // A role's check mark is a choice that keeps the checklist open.
    let role = host.store.server(server).unwrap().roles[1].id;
    host.pf_act(&format!("ss:member-of:{server}:{user}:role:{role}"));
    assert_eq!(host.ui.profiles.menu, Some((user, CardMenu::Roles)));
}
