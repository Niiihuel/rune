//! The desktop work (P6), driven over the simulation: favorites and pinned
//! conversations, hidden channels, the panels' widths, the desktop's
//! theme, the local activity, the voice shortcuts and the call's codes.

use super::*;
use rune_model::{ActivitySource, LocalActivity, Permissions, UserId};

use crate::desktop::{self, Shortcut, CALL_CHAT_HANDLE, CHANNELS_HANDLE, MEMBERS_HANDLE};
use crate::title_bar::TitleBar;

fn saved(intents: &[Intent], key: &str) -> Option<Option<String>> {
    intents.iter().find_map(|intent| match intent {
        Intent::SaveSetting { key: saved, value } if saved == key => Some(value.clone()),
        _ => None,
    })
}

impl Host {
    fn desktop_click(&mut self, id: &str) -> Vec<Intent> {
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::Clicked { id: id.into() },
            self.now,
        );
        self.run(intents.clone());
        intents
    }
}

#[test]
fn a_favorite_goes_first_under_its_heading_and_survives_a_restart() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let intents = host.desktop_click(&format!("cl:favorite:{channel}"));
    assert_eq!(
        saved(&intents, "favorite_channels"),
        Some(Some(channel.0.to_string()))
    );
    let painted = host.painted();
    let heading = painted.iter().position(|text| text == "Favoritos").unwrap();
    let first = painted.iter().position(|text| text == "general").unwrap();
    assert!(heading < first, "the heading, then the channel");
    let row = format!("channel:{channel}");
    assert_eq!(
        host.frame()
            .regions
            .iter()
            .filter(|region| region.id == row)
            .count(),
        1,
        "lifted out of its category, not repeated"
    );
    // Kept and read back as the next run would.
    let kept = host.ui.settings_to_keep();
    let mut again = Ui::new();
    again.restore(|key| {
        kept.iter()
            .find(|(found, _)| *found == key)
            .and_then(|(_, value)| value.clone())
    });
    assert_eq!(again.prefs.favorites, [channel]);
    host.desktop_click(&format!("cl:favorite:{channel}"));
    assert!(host.ui.prefs.favorites.is_empty());
    assert!(!host.painted().contains(&"Favoritos".to_owned()));
}

#[test]
fn a_pinned_conversation_leads_the_direct_list() {
    let mut host = Host::new();
    host.click("home");
    let last = *host
        .store
        .directs()
        .last()
        .map(|channel| &channel.id)
        .unwrap();
    let intents = host.desktop_click(&rune_components::group::ids::of(last, "pin"));
    assert_eq!(
        saved(&intents, "pinned_directs"),
        Some(Some(last.0.to_string()))
    );
    let painted = host.painted();
    let pinned = painted.iter().position(|text| text == "Fijados").unwrap();
    // The column's header says it too: the list's heading is the last.
    let directs = painted
        .iter()
        .rposition(|text| text == "Mensajes directos")
        .unwrap();
    let title = host.store.channel_title(last);
    let row = painted.iter().position(|text| *text == title).unwrap();
    assert!(pinned < row && row < directs, "{painted:?}");
}

#[test]
fn hidden_channels_show_dimmed_only_when_asked_and_never_open() {
    let mut host = Host::new();
    let channel = host.channel_named("general");
    let server = host.store.channel(channel).unwrap().server.unwrap();
    // The account loses the right to see it.
    let mut hidden = host.store.channel(channel).unwrap().clone();
    hidden.permissions = hidden.permissions.without(Permissions::VIEW);
    host.event(Event::ChannelChanged(hidden));
    host.click(&format!("server:{server}"));
    assert!(!host.painted().contains(&"general".to_owned()));
    let intents = host.desktop_click("cl:show-hidden");
    assert_eq!(
        saved(&intents, "show_hidden_channels"),
        Some(Some("true".into()))
    );
    let frame = host.frame();
    assert!(host.painted().contains(&"general".to_owned()));
    assert!(
        !frame
            .regions
            .iter()
            .any(|region| region.id == format!("channel:{channel}")),
        "a hidden channel is not a control"
    );
}

#[test]
fn a_panel_is_dragged_wider_within_its_bounds_and_kept_or_put_back() {
    let mut host = Host::new();
    let start = host.ui.prefs.sidebar_width;
    let drag = |host: &mut Host, action: UiAction| {
        let intents = update(&mut host.ui, &mut host.store, &action, host.now);
        host.run(intents.clone());
        intents
    };
    let id = || CHANNELS_HANDLE.to_owned();
    drag(
        &mut host,
        UiAction::DragStarted {
            id: id(),
            position: Point::new(380.0, 300.0),
        },
    );
    drag(
        &mut host,
        UiAction::DragMoved {
            id: id(),
            position: Point::new(430.0, 300.0),
            over: None,
        },
    );
    assert_eq!(host.ui.prefs.sidebar_width, (start + 50.0).round());
    drag(
        &mut host,
        UiAction::DragMoved {
            id: id(),
            position: Point::new(4000.0, 300.0),
            over: None,
        },
    );
    assert_eq!(host.ui.prefs.sidebar_width, *desktop::SIDEBAR_WIDTHS.end());
    let intents = drag(
        &mut host,
        UiAction::Dropped {
            id: id(),
            position: Point::new(4000.0, 300.0),
            target: None,
        },
    );
    assert!(saved(&intents, "sidebar_width").is_some());
    // The member list's handle is on its left: going left widens it.
    let members = host.ui.prefs.members_width;
    let id = || MEMBERS_HANDLE.to_owned();
    drag(
        &mut host,
        UiAction::DragStarted {
            id: id(),
            position: Point::new(900.0, 300.0),
        },
    );
    drag(
        &mut host,
        UiAction::DragMoved {
            id: id(),
            position: Point::new(880.0, 300.0),
            over: None,
        },
    );
    assert_eq!(host.ui.prefs.members_width, (members + 20.0).round());
    drag(&mut host, UiAction::DragCancelled { id: id() });
    assert_eq!(host.ui.prefs.members_width, members, "cancelled, as it was");
    // The window lays out with the column as wide as left.
    let frame = host.frame();
    let handle = frame
        .regions
        .iter()
        .find(|region| region.id == CHANNELS_HANDLE)
        .unwrap();
    let edge = rune_components::rail::WIDTH + host.ui.prefs.sidebar_width;
    assert!((handle.rect.x + handle.rect.width / 2.0 - edge).abs() < 1.0);
}

#[test]
fn the_call_s_chat_is_dragged_wider_from_its_left_edge_and_kept() {
    let mut host = Host::new();
    let start = host.ui.prefs.call_chat_width;
    let drag = |host: &mut Host, action: UiAction| {
        let intents = update(&mut host.ui, &mut host.store, &action, host.now);
        host.run(intents.clone());
        intents
    };
    let id = || CALL_CHAT_HANDLE.to_owned();
    drag(
        &mut host,
        UiAction::DragStarted {
            id: id(),
            position: Point::new(900.0, 300.0),
        },
    );
    // Its handle is on its left: going left widens it.
    drag(
        &mut host,
        UiAction::DragMoved {
            id: id(),
            position: Point::new(860.0, 300.0),
            over: None,
        },
    );
    assert_eq!(host.ui.prefs.call_chat_width, (start + 40.0).round());
    drag(
        &mut host,
        UiAction::DragMoved {
            id: id(),
            position: Point::new(-4000.0, 300.0),
            over: None,
        },
    );
    assert_eq!(
        host.ui.prefs.call_chat_width,
        *desktop::CALL_CHAT_WIDTHS.end()
    );
    let intents = drag(
        &mut host,
        UiAction::Dropped {
            id: id(),
            position: Point::new(-4000.0, 300.0),
            target: None,
        },
    );
    assert!(saved(&intents, "call_chat_width").is_some());
}

#[test]
fn the_system_theme_is_followed_only_when_chosen() {
    let mut host = Host::new();
    host.ui.theme = Theme::Dark;
    desktop::color_scheme(&mut host.ui, Some(false));
    assert_eq!(host.ui.theme, Theme::Dark, "not asked to follow");
    host.ui.settings = Some(crate::SettingsPage::Appearance);
    let intents = host.desktop_click("us:theme=system");
    assert_eq!(saved(&intents, "theme"), Some(Some("system".into())));
    assert_eq!(host.ui.theme, Theme::Light, "the desktop prefers light");
    desktop::color_scheme(&mut host.ui, Some(true));
    assert_eq!(host.ui.theme, Theme::Onyx);
    assert!(host
        .ui
        .settings_to_keep()
        .contains(&("theme", Some("system".into()))));
    host.desktop_click("us:theme=light");
    desktop::color_scheme(&mut host.ui, Some(true));
    assert_eq!(
        host.ui.theme,
        Theme::Light,
        "a theme chosen stops following"
    );
}

// Windows (windows-base).
#[test]
fn the_system_accent_tints_only_the_system_theme_and_never_over_the_person_s() {
    let mut host = Host::new();
    let blue = limn::Color::rgb(0x00, 0x78, 0xd7);
    desktop::system_accent(&mut host.ui, Some(blue));
    assert_ne!(host.ui.look().accent, blue, "not while a theme is chosen");
    host.ui.settings = Some(crate::SettingsPage::Appearance);
    host.desktop_click("us:theme=system");
    assert_eq!(
        host.ui.look().accent,
        blue,
        "the system's theme, its accent"
    );
    let own = limn::Color::rgb(0xf2, 0x3f, 0x43);
    host.ui.themes.accent = Some(own);
    assert_eq!(host.ui.look().accent, own, "the person's main color wins");
    host.ui.themes.accent = None;
    desktop::system_accent(&mut host.ui, None);
    assert_eq!(host.ui.look().accent, host.ui.theme.look().accent);
}

#[test]
fn the_local_activity_needs_its_switch_and_shows_as_the_account_s() {
    let mut host = Host::new();
    let game = LocalActivity {
        application: Some(1_001),
        name: Some("Ajedrez de prueba".into()),
        source: ActivitySource::Game,
        ..LocalActivity::default()
    };
    let me = host.store.me().unwrap();
    let intents = desktop::local_activity(&mut host.ui, &mut host.store, Some(game.clone()));
    assert!(intents.is_empty(), "games are off until turned on");
    assert!(host.store.presence(me).activities.is_empty());
    host.ui.prefs.game_activity = true;
    let intents = desktop::local_activity(&mut host.ui, &mut host.store, Some(game.clone()));
    assert_eq!(
        intents,
        [Intent::Command(Command::SetLocalActivity(Some(
            game.clone()
        )))]
    );
    let shown = host.store.presence(me).activities.remove(0);
    assert_eq!(shown.name, "Ajedrez de prueba");
    assert!(
        desktop::local_activity(&mut host.ui, &mut host.store, Some(game)).is_empty(),
        "the same activity is not sent again"
    );
    // A program's needs its own switch.
    let program = LocalActivity {
        application: Some(5),
        source: ActivitySource::Program,
        ..LocalActivity::default()
    };
    let intents = desktop::local_activity(&mut host.ui, &mut host.store, Some(program));
    assert_eq!(intents, [Intent::Command(Command::SetLocalActivity(None))]);
    assert!(host.store.presence(me).activities.is_empty());
}

#[test]
fn a_voice_shortcut_is_recorded_for_its_own_and_cleared() {
    let mut host = Host::new();
    host.ui.settings = Some(crate::SettingsPage::Voice);
    host.desktop_click("us:key-record=mute");
    assert!(crate::recording_key(&host.ui));
    assert_eq!(
        host.ui.settings_state.recording_for,
        crate::keybinds::Action::Voice(Shortcut::Mute)
    );
    let intents = crate::record_key(&mut host.ui, Some("Ctrl+Shift+M".into()));
    assert_eq!(
        saved(&intents, "mute_key"),
        Some(Some("Ctrl+Shift+M".into()))
    );
    assert_eq!(host.ui.prefs.mute_key.as_deref(), Some("Ctrl+Shift+M"));
    assert_eq!(
        host.ui.prefs.push_to_talk_key, None,
        "push-to-talk is untouched"
    );
    let intents = host.desktop_click("us:key-reset=mute");
    assert_eq!(saved(&intents, "mute_key"), Some(None));
    assert_eq!(host.ui.prefs.mute_key, None);
}

#[test]
fn the_call_says_its_code_and_each_person_s_is_asked_for() {
    let mut host = Host::new();
    let room = host.channel_named("Sala común");
    host.event(Event::CallPhase {
        channel: room,
        phase: rune_model::CallPhase::Connected,
        quality: None,
    });
    host.event(Event::CallPrivacyCode {
        channel: room,
        code: Some("0".repeat(30)),
    });
    let someone = UserId(10);
    host.event(Event::VoiceChanged(rune_model::VoiceState {
        user: someone,
        channel: room,
        muted: false,
        deafened: false,
        speaking: false,
        streaming: false,
        server_muted: false,
        server_deafened: false,
        camera: false,
        suppressed: false,
        speak_request: None,
    }));
    host.ui.voice.details = true;
    // The connection's tab first; the codes in «Privacidad».
    assert!(!host.painted().contains(&"00000 00000 00000".to_owned()));
    host.click(rune_components::voice::details_tabs::PRIVACY);
    let painted = host.painted();
    assert!(
        painted.contains(&"00000 00000 00000".to_owned()),
        "{painted:?}"
    );
    assert!(painted.iter().any(|text| text.contains("simulados")));
    let intents = host.desktop_click(&format!("e2ee:verify:{someone}"));
    assert_eq!(
        intents,
        [Intent::Command(Command::VoiceVerificationCode {
            channel: room,
            user: someone
        })]
    );
    assert_eq!(
        host.store.verification(someone),
        Some(&rune_state::Verification::Asking)
    );
    host.event(Event::VoiceVerification {
        channel: room,
        user: someone,
        code: Some("1".repeat(45)),
    });
    assert!(host.painted().contains(&"11111 11111 11111".to_owned()));
    let intents = host.desktop_click("e2ee:copy");
    assert_eq!(intents, [Intent::Copy("0".repeat(30))]);
}

#[test]
fn a_link_handed_over_opens_its_conversation_here() {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .trusted_links = &["chat.test"];
    let channel = host.channel_named("general");
    let intents = desktop::open_link(
        &mut host.ui,
        &mut host.store,
        &format!("https://chat.test/channels/1/{channel}"),
        host.now,
    );
    host.run(intents);
    assert_eq!(host.ui.channel(), Some(channel));
    // Anything else asks before leaving RUNE.
    let intents = desktop::open_link(
        &mut host.ui,
        &mut host.store,
        "https://example.com/x",
        host.now,
    );
    assert!(intents.is_empty());
    assert!(host.ui.message_actions.external.is_some());
}

// Linux desktop (escritorio-linux).
#[test]
fn the_window_s_decorations_and_gpu_are_chosen_for_the_host() {
    let mut host = Host::new();
    desktop::window_known(
        &mut host.ui,
        TitleBar::Automatic,
        desktop::Graphics {
            gpus: vec![
                ("amd".into(), "AMD".into()),
                ("nvidia".into(), "NVIDIA".into()),
            ],
            chosen: None,
            drawing: Some("AMD Radeon 780M (Vulkan)".into()),
            overridden: None,
            fallback: false,
            fallback_chosen: false,
        },
    );
    host.ui.settings = Some(crate::SettingsPage::Desktop);
    host.desktop_click("us:jump=graphics");
    host.frame();
    let painted = host.painted();
    for text in [
        "Barra de título",
        "Automática",
        "Dibujar con",
        "Ahora dibuja con AMD Radeon 780M (Vulkan). Se aplica la próxima vez que abras RUNE.",
    ] {
        assert!(painted.iter().any(|shown| shown == text), "{text}");
    }
    // The window's own title bar (barra-e-icono): chosen from a list.
    host.desktop_click("us:select:title-bar");
    assert_eq!(host.ui.settings_state.select, Some("title-bar"));
    for (id, choice) in [
        ("us:title-bar=rune", TitleBar::Rune),
        ("us:title-bar=system", TitleBar::System),
        ("us:title-bar=auto", TitleBar::Automatic),
    ] {
        assert_eq!(host.desktop_click(id), [Intent::TitleBar(choice)]);
        assert_eq!(host.ui.desktop.title_bar, choice);
        assert_eq!(host.ui.settings_state.select, None);
    }
    host.desktop_click("us:select:gpu");
    assert_eq!(host.ui.settings_state.select, Some("gpu"));
    assert_eq!(
        host.desktop_click("us:gpu=nvidia"),
        [Intent::DrawWith(Some("nvidia".into()))]
    );
    assert_eq!(host.ui.settings_state.select, None);
    assert_eq!(host.ui.desktop.graphics.chosen.as_deref(), Some("nvidia"));
    // Only a GPU the host found.
    assert!(!host
        .desktop_click("us:gpu=intel")
        .iter()
        .any(|intent| matches!(intent, Intent::DrawWith(_))));
    assert_eq!(host.ui.desktop.graphics.chosen.as_deref(), Some("nvidia"));
    assert_eq!(host.desktop_click("us:gpu=auto"), [Intent::DrawWith(None)]);
    // A variable of the environment that decides says so.
    host.ui.desktop.graphics.overridden = Some("RUNE_GPU".into());
    host.frame();
    // Painted line by line: the note wraps.
    let painted = host.painted().join(" ");
    let painted = painted.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(painted.contains("RUNE. La variable RUNE_GPU elige la GPU mientras esté."));
    // The start again with any GPU says whose GPU could not open the window.
    host.ui.desktop.graphics.overridden = None;
    host.ui.desktop.graphics.fallback = true;
    for (chosen, said) in [
        (true, "RUNE. La GPU elegida no pudo abrir la ventana"),
        (
            false,
            "RUNE. La GPU que eligió RUNE no pudo abrir la ventana",
        ),
    ] {
        host.ui.desktop.graphics.fallback_chosen = chosen;
        host.frame();
        let painted = host.painted().join(" ");
        let painted = painted.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(painted.contains(said), "{said}");
    }
}

// Linux desktop (escritorio-linux).
#[test]
fn what_the_host_read_of_the_window_outlasts_a_sign_out_and_a_new_provider() {
    let mut host = Host::new();
    let graphics = desktop::Graphics {
        gpus: vec![("nvidia".into(), "NVIDIA".into())],
        chosen: Some("nvidia".into()),
        drawing: Some("NVIDIA GeForce RTX 5060 (Vulkan)".into()),
        ..desktop::Graphics::default()
    };
    desktop::window_known(&mut host.ui, TitleBar::Rune, graphics.clone());
    host.ui.desktop.system_language = rune_i18n::Language::English;
    host.event(Event::Authentication(
        rune_provider::AuthenticationState::SignedOut,
    ));
    assert_eq!(host.ui.desktop.title_bar, TitleBar::Rune);
    assert_eq!(host.ui.desktop.graphics, graphics);
    assert_eq!(
        host.ui.desktop.system_language,
        rune_i18n::Language::English
    );
    crate::providers::provider_changed(&mut host.ui, "mock");
    assert_eq!(host.ui.desktop.graphics, graphics);
    // The list shows the choice kept: another is one click.
    assert_eq!(
        host.desktop_click("us:title-bar=system"),
        [Intent::TitleBar(TitleBar::System)]
    );
}

// Linux desktop (escritorio-linux).
#[test]
fn a_sign_out_closes_the_folder_dialog_it_opened() {
    let mut host = Host::new();
    host.desktop_click("us:sound-folder:choose");
    assert!(host.ui.call_sounds.choosing.is_some());
    let intents = crate::providers::provider_changed(&mut host.ui, "mock");
    assert!(intents.contains(&Intent::CancelSoundFolder));
    assert_eq!(host.ui.call_sounds.choosing, None);
}

// Desktop session and games (escritorio-sesion-y-juegos).
fn painted_text(host: &mut Host) -> String {
    host.frame();
    let painted = host.painted().join(" ");
    painted.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn starting_in_the_tray_follows_the_session_s_start_and_is_asked_of_the_host() {
    let mut host = Host::new();
    host.ui.settings = Some(crate::SettingsPage::Desktop);
    assert!(
        host.desktop_click("us:autostart-minimized").is_empty(),
        "not read yet"
    );
    desktop::autostart_known(&mut host.ui, false, false, None);
    assert!(
        host.desktop_click("us:autostart-minimized").is_empty(),
        "the session's start first"
    );
    assert_eq!(
        host.desktop_click("us:autostart"),
        [Intent::SetAutostart {
            on: true,
            minimized: false
        }]
    );
    desktop::autostart_known(&mut host.ui, true, false, None);
    assert_eq!(
        host.desktop_click("us:autostart-minimized"),
        [Intent::SetAutostart {
            on: true,
            minimized: true
        }]
    );
    assert_eq!(host.ui.desktop.autostart, None, "changing");
    desktop::autostart_known(&mut host.ui, true, true, None);
    host.desktop_click("us:jump=desktop");
    assert!(painted_text(&mut host).contains("Iniciar minimizado"));
    // Off, nothing starts in the tray either.
    assert_eq!(
        host.desktop_click("us:autostart"),
        [Intent::SetAutostart {
            on: false,
            minimized: false
        }]
    );
    desktop::autostart_known(&mut host.ui, false, true, None);
    assert!(!host.ui.desktop.autostart_minimized);
}

#[test]
fn the_account_s_hidden_activity_is_said_and_turned_on_only_by_a_click() {
    use rune_model::ActivityVisibility;
    let mut host = Host::new();
    host.ui.prefs.game_activity = true;
    host.event(Event::ActivitySharing(false));
    host.ui.settings = Some(crate::SettingsPage::Activity);
    host.desktop_click("us:jump=activity");
    let painted = painted_text(&mut host);
    assert!(
        painted
            .contains("Tu cuenta oculta tu actividad: aunque RUNE detecte un juego, nadie lo ve."),
        "{painted}"
    );
    assert!(painted.contains("Activar en la cuenta"));
    let from = host.sent.len();
    assert_eq!(
        host.desktop_click("us:activity-sharing-on"),
        [Intent::Command(Command::SetActivitySharing(true))]
    );
    assert!(
        host.desktop_click("us:activity-sharing-on").is_empty(),
        "once while it is asked"
    );
    assert!(painted_text(&mut host).contains("Activando…"));
    host.answer_account(from);
    assert_eq!(host.store.activity_sharing(), Some(true));
    assert!(painted_text(&mut host).contains("Tu cuenta muestra tu actividad."));
    // What the sessions say of the game published, simulated here.
    let game = LocalActivity {
        application: Some(1_001),
        name: Some("Ajedrez de prueba".into()),
        source: ActivitySource::Game,
        ..LocalActivity::default()
    };
    desktop::local_activity(&mut host.ui, &mut host.store, Some(game));
    let painted = painted_text(&mut host);
    assert!(
        painted.contains("Esperando que tu cuenta la confirme. (simulado)"),
        "{painted}"
    );
    host.event(Event::ActivityVisibility(ActivityVisibility::Hidden));
    assert!(painted_text(&mut host).contains("Tu cuenta la oculta: nadie la ve. (simulado)"));
}

#[test]
fn the_proxy_takes_only_a_bare_address_and_is_kept_for_the_host() {
    use rune_provider::ApiProxy;
    let mut host = Host::new();
    host.ui.settings = Some(crate::SettingsPage::Connection);
    host.desktop_click("us:jump=proxy");
    assert!(painted_text(&mut host).contains("Proxy para la API"));
    assert_eq!(
        host.desktop_click("us:proxy=direct"),
        [Intent::SetProxy(ApiProxy::Direct)]
    );
    let opened = host.desktop_click("us:proxy=url");
    assert!(opened.contains(&Intent::Focus(desktop::PROXY.into())));
    let write = |host: &mut Host, text: &str| {
        let action = UiAction::InputChanged {
            id: desktop::PROXY.into(),
            value: text.into(),
        };
        update(&mut host.ui, &mut host.store, &action, host.now)
    };
    write(&mut host, "http://yo:clave@proxy.example:8080");
    assert!(
        host.desktop_click("us:proxy-save").is_empty(),
        "no credentials"
    );
    assert!(painted_text(&mut host).contains("sin usuario ni contraseña"));
    assert_eq!(host.ui.desktop.proxy, ApiProxy::Direct, "unchanged");
    write(&mut host, "http://127.0.0.1:3128");
    let saved = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Submitted {
            id: desktop::PROXY.into(),
            value: "http://127.0.0.1:3128".into(),
        },
        host.now,
    );
    let wanted = ApiProxy::url("http://127.0.0.1:3128").unwrap();
    assert_eq!(saved, [Intent::SetProxy(wanted.clone())]);
    assert_eq!(host.ui.desktop.proxy, wanted);
    assert!(painted_text(&mut host).contains("Ahora: http://127.0.0.1:3128"));
}

#[test]
fn the_registered_games_page_lists_renames_and_hides_them() {
    use crate::games::{self, Running};
    let mut host = Host::new();
    host.ui.prefs.game_activity = true;
    let running = Running {
        executable: "juego.exe".into(),
        application: Some(1_001),
        name: "Ajedrez de prueba".into(),
    };
    let intents = games::running(&mut host.ui, Some(running), 1_790_000_000_000);
    assert!(matches!(&intents[..], [Intent::SaveSetting { key, .. }] if key == games::SETTING));
    host.ui.settings = Some(crate::SettingsPage::Activity);
    host.desktop_click("us:jump=games");
    let painted = painted_text(&mut host);
    for text in [
        "Juegos registrados",
        "Ahora: Ajedrez de prueba",
        "Agregar un programa",
    ] {
        assert!(painted.contains(text), "{text}: {painted}");
    }
    // Choosing a program to add asks the host for the list.
    assert_eq!(
        host.desktop_click("us:select:programs"),
        [Intent::ListPrograms]
    );
    games::programs_listed(
        &mut host.ui,
        vec![("bin/otro.x86_64".into(), "bin/Otro.x86_64".into())],
    );
    host.desktop_click("us:game-add=0");
    assert!(painted_text(&mut host).contains("Agregado a mano"));
    // Renamed in its field, kept and shown.
    host.desktop_click("us:game-rename=juego.exe");
    let action = UiAction::InputChanged {
        id: games::NAME.into(),
        value: "Ajedrez".into(),
    };
    update(&mut host.ui, &mut host.store, &action, host.now);
    let saved = host.desktop_click("us:game-name:save");
    assert!(saved
        .iter()
        .any(|intent| matches!(intent, Intent::SaveSetting { key, .. } if key == games::SETTING)));
    assert!(painted_text(&mut host).contains("Ahora: Ajedrez"));
    host.desktop_click("us:game-hide-running");
    assert!(painted_text(&mut host).contains("Oculto: no se detecta."));
    // Kept and read back as the next run would.
    let kept = host.ui.settings_to_keep();
    let mut again = Ui::new();
    again.restore(|key| {
        kept.iter()
            .find(|(found, _)| *found == key)
            .and_then(|(_, value)| value.clone())
    });
    assert_eq!(again.prefs.registered_games, host.ui.prefs.registered_games);
}
