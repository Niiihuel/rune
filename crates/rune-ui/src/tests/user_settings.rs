//! The account's settings, driven as a person would: the panel, its search
//! and sections, and what each preference changes in the window and keeps
//! for the next run.

use rune_model::Rgb;
use rune_provider::ProfileEdit;

use super::*;
use crate::settings::{jump_margin, ACCENT_HEX, NAME, SEARCH};

#[test]
fn forgetting_clears_unqueued_drafts_and_waits_for_the_host_confirmation() {
    let mut host = Host::new();
    host.open_channel("general");
    host.type_text("todavía no guardado");
    assert_eq!(host.ui.dirty_drafts().count(), 1);
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "us:forget".into(),
        },
        host.now,
    );
    assert_eq!(intents, vec![Intent::ForgetLocalData]);
    assert!(host.ui.drafts.is_empty());
    assert_eq!(host.ui.dirty_drafts().count(), 0);
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title == "Borrando datos locales…"));
    assert!(!host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title == "Datos locales borrados"));
}

impl Host {
    fn us_act(&mut self, action: UiAction) {
        let intents = update(&mut self.ui, &mut self.store, &action, self.now);
        self.run(intents);
    }

    fn us_press(&mut self, id: &str) {
        self.us_act(UiAction::Clicked { id: id.into() });
    }

    /// Write `value` in the field `id`, as typing it all would.
    fn us_write(&mut self, id: &str, value: &str) {
        self.tree.set_input_value(id, value);
        self.us_act(UiAction::InputChanged {
            id: id.into(),
            value: value.into(),
        });
    }

    /// Let go of the slider `id` at `fraction`.
    fn us_slide(&mut self, id: &str, fraction: f32, committed: bool) {
        self.us_act(UiAction::SliderChanged {
            id: id.into(),
            fraction,
            committed,
        });
    }

    /// Open the settings with the gear of the account's panel.
    fn us_open(&mut self) {
        self.click("me:settings");
        self.frame();
    }

    fn us_buttons(&mut self) -> Vec<String> {
        let frame = self.frame();
        frame
            .regions
            .iter()
            .filter(|region| region.kind == limn::HitKind::Button)
            .map(|region| region.id.clone())
            .collect()
    }

    /// Lay the window out at `size`.
    fn us_layout(&mut self, size: Size) -> Frame {
        self.tree.set_root(view(
            &self.ui,
            &self.store,
            &ViewContext {
                window: size,
                ..self.context()
            },
        ));
        self.tree.layout(size)
    }

    /// The interface as the next run starts it, from what this one keeps.
    fn us_next_run(&self) -> Ui {
        let kept = self.ui.settings_to_keep();
        let mut next = Ui::default();
        next.restore(|key| {
            kept.iter()
                .find(|(kept, _)| *kept == key)
                .and_then(|(_, value)| value.clone())
        });
        next
    }

    /// Avatars painted: 40 px discs.
    fn us_avatars(&mut self) -> usize {
        self.frame()
            .commands
            .iter()
            .filter(|command| {
                matches!(command, limn::DrawCommand::FillRect { rect, radius, .. }
                    if rect.width == 42.0 && rect.height == 42.0 && *radius == 21.0)
            })
            .count()
    }

    /// The sizes text is painted at.
    fn us_text_sizes(&mut self) -> Vec<f32> {
        self.frame()
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::Text { font_size, .. } => Some(*font_size),
                _ => None,
            })
            .collect()
    }

    /// Open the message menu of the last message of `channel`.
    fn us_message_menu(&mut self, channel: ChannelId) {
        let last = self
            .store
            .timeline(channel)
            .unwrap()
            .messages()
            .last()
            .unwrap()
            .id;
        let frame = self.frame();
        let rect = frame
            .regions
            .iter()
            .find(|region| region.id == format!("msg:{last}:reply"))
            .expect("the last message is on screen")
            .rect;
        let at = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        self.send(UiEvent::ContextMenu { position: at });
    }
}

fn painted_has(host: &mut Host, text: &str) -> bool {
    host.painted().iter().any(|painted| painted.contains(text))
}

#[test]
fn the_panel_floats_over_the_window_with_the_reference_s_measures() {
    let mut host = Host::new();
    host.us_open();
    assert_eq!(host.ui.settings, Some(SettingsPage::Account));
    let frame = host.us_layout(Size::new(1361.0, 797.0));
    let rect = |id: &str| {
        frame
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is laid out"))
            .rect
    };
    let close = rect("us:close");
    let (x, y) = (close.x + close.width / 2.0, close.y + close.height / 2.0);
    assert!(
        (x - 1291.0).abs() < 3.0 && (y - 69.0).abs() < 3.0,
        "the close button at {x},{y}"
    );
    let account = rect("us:page=account");
    assert!(
        (account.x - 60.5).abs() < 1.0 && account.width == 235.0 && account.height == 36.0,
        "{account:?}"
    );
    let next = rect("us:page=privacy");
    assert!(
        (next.y - account.y - 41.0).abs() < 0.5,
        "items are 41 px apart"
    );
    assert!(painted_has(&mut host, "Tu cuenta"));
}

#[test]
fn every_page_lays_out_at_every_width_without_spilling() {
    let mut host = Host::new();
    host.us_open();
    for page in SettingsPage::ALL {
        host.us_press(&format!("us:page={}", page.key()));
        assert_eq!(host.ui.settings, Some(page));
        for width in [720.0, 1024.0, 1600.0] {
            let frame = host.us_layout(Size::new(width, 700.0));
            let margin = if width >= 960.0 { 43.5 } else { 16.0 };
            let mine: Vec<_> = frame
                .regions
                .iter()
                .filter(|region| region.id.starts_with("us:"))
                .collect();
            assert!(!mine.is_empty(), "{page:?} at {width}");
            // The backdrop that closes the panel covers the whole window.
            for region in mine.into_iter().filter(|region| region.rect.width < width) {
                assert!(
                    region.rect.x >= margin - 1.0
                        && region.rect.x + region.rect.width <= width - margin + 1.0,
                    "{} spills on {page:?} at {width}: {:?}",
                    region.id,
                    region.rect
                );
            }
        }
    }
}

#[test]
fn searching_keeps_what_matches_and_opening_a_result_jumps_to_it() {
    let mut host = Host::new();
    host.us_open();
    host.us_write(SEARCH, "zoom");
    let buttons = host.us_buttons();
    assert!(buttons.contains(&"us:page=accessibility".to_owned()));
    assert!(
        !buttons.contains(&"us:page=account".to_owned()),
        "what does not match leaves the sidebar"
    );
    assert!(buttons.contains(&"us:search:clear".to_owned()));
    host.click("us:page=accessibility");
    assert_eq!(host.ui.settings, Some(SettingsPage::Accessibility));
    assert_eq!(host.ui.settings_state.section, Some("density"));
    let painted = host.painted();
    assert!(painted.iter().any(|text| text == "Zoom"));
    assert!(
        !painted.iter().any(|text| text == "Tamaño del texto"),
        "only what matches shows: {painted:?}"
    );
    assert!(host.us_buttons().contains(&"us:jump=density".to_owned()));
    // Enter opens the first page that matches, at what matches there.
    host.us_write(SEARCH, "formato de hora");
    host.us_act(UiAction::Submitted {
        id: SEARCH.into(),
        value: "formato de hora".into(),
    });
    assert_eq!(host.ui.settings, Some(SettingsPage::Language));
    assert_eq!(host.ui.settings_state.section, Some("clock"));
    // Emptied, the sidebar has every page again.
    host.click("us:search:clear");
    assert!(host.ui.settings_state.query.is_empty());
    assert!(host.us_buttons().contains(&"us:page=account".to_owned()));
    assert_eq!(host.tree.input_value(SEARCH), Some(""));
}

#[test]
fn a_section_s_link_jumps_to_it_and_the_sidebar_follows_the_scroll() {
    let mut host = Host::new();
    host.us_open();
    host.click("us:page=accessibility");
    host.frame();
    assert_eq!(host.ui.settings_state.section, Some("readability"));
    host.click("us:jump=color");
    host.frame();
    let top = host.tree.anchor_offset("us:section:color").unwrap();
    let margin = jump_margin(SettingsPage::Accessibility);
    // Under the preview, unless the page ends first: then the jump stops at
    // the end, a little lower.
    let scroll = SettingsPage::Accessibility.scroll_id();
    assert!(
        (top - margin).abs() < 8.0 || (top > margin && host.tree.is_scroll_at_end(&scroll, 1.0)),
        "{top} under the preview"
    );
    assert_eq!(host.ui.settings_state.section, Some("color"));
    // Scrolled back up by hand, the first section is the one being read.
    host.tree
        .scroll_to_start(&SettingsPage::Accessibility.scroll_id());
    host.frame();
    host.frame();
    assert_eq!(host.ui.settings_state.section, Some("readability"));
    // Scrolled to the end, the last one.
    host.tree
        .scroll_to_end(&SettingsPage::Accessibility.scroll_id());
    host.frame();
    host.frame();
    assert_eq!(host.ui.settings_state.section, Some("motion"));
}

#[test]
fn keys_stay_in_the_settings_while_they_are_open() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    assert_eq!(host.tree.focused_id(), Some(editor_id(general).as_str()));
    host.us_open();
    assert_ne!(host.tree.focused_id(), Some(editor_id(general).as_str()));
    host.type_text("hola");
    host.key(Key::ArrowUp, false, false);
    assert!(host
        .ui
        .drafts
        .get(&general)
        .is_none_or(|draft| draft.is_empty()));
    assert!(host.ui.editing.is_none());
}

#[test]
fn escape_closes_what_floats_first_then_the_settings() {
    let mut host = Host::new();
    host.us_open();
    host.click("us:page=accessibility");
    host.click("us:jump=color");
    host.click("us:select:role-colors");
    assert_eq!(host.ui.settings_state.select, Some("role-colors"));
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.settings_state.select, None);
    assert!(host.ui.settings.is_some());
    host.key(Key::Escape, false, false);
    assert!(host.ui.settings.is_none());
    // The close button, and a click outside, close them too.
    host.us_open();
    host.click("us:close");
    assert!(host.ui.settings.is_none());
}

#[test]
fn the_display_name_and_the_avatar_color_are_asked_of_the_provider() {
    let mut host = Host::new();
    host.us_open();
    assert!(host.us_buttons().contains(&"profile:edit".to_owned()));
    host.click("us:name:edit");
    host.frame();
    assert_eq!(
        host.tree.input_value(NAME),
        host.store.me_user().map(|me| me.shown_name())
    );
    host.us_write(NAME, "Nueva");
    host.click("us:name:save");
    let edit = ProfileEdit {
        display_name: Some(Some("Nueva".into())),
        ..ProfileEdit::default()
    };
    assert_eq!(host.sent.last(), Some(&Command::EditProfile(edit.clone())));
    assert!(host.ui.settings_state.name.is_none());
    for event in host.provider.manage(Command::EditProfile(edit)) {
        host.event(event);
    }
    assert_eq!(host.store.me_user().unwrap().shown_name(), "Nueva");
    assert!(painted_has(&mut host, "Nueva"));
    host.click("us:accent:1abc9c");
    assert_eq!(
        host.sent.last(),
        Some(&Command::EditProfile(ProfileEdit {
            accent: Some(Rgb(0x1a_bc_9c)),
            ..ProfileEdit::default()
        }))
    );
    // A color of one's own, in hex, goes on Enter.
    host.click("us:accent:custom");
    host.us_write(ACCENT_HEX, "#ff8800");
    host.us_act(UiAction::Submitted {
        id: ACCENT_HEX.into(),
        value: "#ff8800".into(),
    });
    assert_eq!(
        host.sent.last(),
        Some(&Command::EditProfile(ProfileEdit {
            accent: Some(Rgb(0xff_88_00)),
            ..ProfileEdit::default()
        }))
    );
}

#[test]
fn letting_go_of_the_avatar_color_picker_asks_for_the_color_it_shows() {
    let mut host = Host::new();
    host.us_open();
    host.click("us:accent:custom");
    let part = |host: &mut Host, id: &str| {
        host.frame()
            .regions
            .iter()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is in the picker"))
            .rect
    };
    let asked = |host: &Host| {
        let typed = host.ui.settings_state.accent_hex.as_deref().unwrap();
        Command::EditProfile(ProfileEdit {
            accent: rune_components::server_settings::parse_hex(typed),
            ..ProfileEdit::default()
        })
    };
    let sent = host.sent.len();
    let area = part(&mut host, "us:accent:area");
    let corner = Point::new(area.x + area.width - 1.0, area.y + 1.0);
    host.send(UiEvent::PointerDown(Point::new(
        area.x + area.width / 2.0,
        area.y + area.height / 2.0,
    )));
    host.send(UiEvent::PointerMove(corner));
    assert_eq!(
        host.sent.len(),
        sent,
        "while it moves the color is only drawn"
    );
    host.send(UiEvent::PointerUp(corner));
    assert_eq!(host.sent.len(), sent + 1);
    assert_eq!(host.sent.last(), Some(&asked(&host)));
    assert!(
        host.ui.settings_state.accent_hex.is_some(),
        "the picker stays open"
    );
    let first = asked(&host);
    // The hue bar too: at its middle, another hue.
    let hue = part(&mut host, "us:accent:hue");
    let middle = Point::new(hue.x + hue.width * 0.5, hue.y + hue.height / 2.0);
    host.send(UiEvent::PointerDown(middle));
    host.send(UiEvent::PointerUp(middle));
    assert_eq!(host.sent.len(), sent + 2);
    assert_eq!(host.sent.last(), Some(&asked(&host)));
    assert_ne!(asked(&host), first);
}

#[test]
fn the_theme_and_how_messages_show_change_the_window_and_are_kept() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    assert!(host.us_avatars() > 0, "messages start with their avatars");
    host.us_open();
    host.click("us:page=appearance");
    // Away from the default, Ónix, so that switching and keeping both show.
    host.click("us:theme=dark");
    assert_eq!(host.ui.theme, Theme::Dark);
    let dark = rune_components::Look::DARK.canvas;
    assert!(host.frame().commands.iter().any(|command| matches!(
        command,
        limn::DrawCommand::FillRect { color, .. } if *color == dark
    )));
    host.click("us:page=accessibility");
    host.click("us:jump=density");
    host.click("us:display=compact");
    assert!(host.ui.prefs.compact);
    host.us_slide("us:text-size", 1.0, true);
    assert_eq!(host.ui.prefs.text_size, 24);
    host.us_slide("us:group-gap", 0.0, true);
    assert_eq!(host.ui.prefs.group_gap, 0);
    host.key(Key::Escape, false, false);
    assert!(host.ui.settings.is_none());
    assert_eq!(host.us_avatars(), 0, "compact rows have no avatars");
    assert!(
        host.us_text_sizes()
            .iter()
            .any(|size| (size - 24.0).abs() < 0.01),
        "what messages say is 24 px"
    );
    // The next run starts as this one was left.
    let next = host.us_next_run();
    assert_eq!(next.theme, Theme::Dark);
    assert_eq!(next.prefs, host.ui.prefs);
    let _ = general;
}

#[test]
fn the_space_between_groups_moves_the_next_group_down() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let messages = host.store.timeline(general).unwrap().messages().clone();
    // The last message that starts a group with one after it: its row has
    // the space above it.
    let pair = messages
        .windows(3)
        .rev()
        .find(|run| run[0].author != run[1].author)
        .map(|run| (run[1].id, run[2].id))
        .unwrap();
    let distance = |host: &mut Host, gap: f32| {
        host.us_slide(
            "us:group-gap",
            settings::GROUP_GAPS
                .iter()
                .position(|value| f32::from(*value) == gap)
                .unwrap() as f32
                / 4.0,
            true,
        );
        host.layout_only();
        host.layout_only();
        let at = |id: MessageId| host.tree.anchor_offset(&format!("anchor:{id}")).unwrap();
        at(pair.1) - at(pair.0)
    };
    let close = distance(&mut host, 0.0);
    let far = distance(&mut host, 24.0);
    assert!((far - close - 24.0).abs() < 0.5, "{close} then {far}");
}

#[test]
fn a_twelve_hour_clock_writes_the_half_of_the_day() {
    let mut host = Host::new();
    host.open_channel("general");
    let halves = |host: &mut Host| {
        host.painted()
            .iter()
            .any(|text| text.contains("a. m.") || text.contains("p. m."))
    };
    assert!(!halves(&mut host), "24 hours to begin with");
    host.us_open();
    host.click("us:page=language");
    host.click("us:clock=12");
    assert!(host.ui.prefs.twelve_hour);
    host.key(Key::Escape, false, false);
    assert!(halves(&mut host));
    assert!(host.us_next_run().prefs.twelve_hour);
    // The clock is the thread's: leave it as the next test expects it.
    rune_components::time::set_twelve_hour(false);
}

#[test]
fn emoticons_go_as_emoji_only_once_the_switch_is_on() {
    let mut host = Host::new();
    host.open_channel("general");
    let sent = |host: &mut Host, text: &str| {
        host.type_text(text);
        host.key(Key::Enter, false, false);
        match host.sent.last() {
            Some(Command::SendMessage { content, .. }) => content.clone(),
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(sent(&mut host, "hola :)"), "hola :)", "off to begin with");
    host.us_open();
    host.click("us:page=appearance");
    // Under the themes, the main color and the transparency (temas-propios).
    host.click("us:jump=messages");
    host.click("us:emoticons");
    assert!(host.ui.prefs.convert_emoticons);
    host.key(Key::Escape, false, false);
    host.open_channel("general");
    assert_eq!(
        sent(&mut host, "hola :) `:)` <3"),
        "hola 🙂 `:)` ❤️",
        "code stays as written"
    );
    assert!(host.us_next_run().prefs.convert_emoticons);
}

#[test]
fn copying_ids_is_offered_in_developer_mode_only() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.us_message_menu(general);
    assert!(painted_has(&mut host, "Copiar texto"), "the menu is open");
    assert!(!painted_has(&mut host, "Copiar ID"));
    host.key(Key::Escape, false, false);
    let rect = host
        .frame()
        .regions
        .iter()
        .find(|region| region.id == format!("channel:{general}"))
        .unwrap()
        .rect;
    host.send(UiEvent::ContextMenu {
        position: Point::new(rect.x + 12.0, rect.y + rect.height / 2.0),
    });
    assert!(host.ui.channel_list.menu.is_some());
    assert!(!host.us_buttons().contains(&format!("cl:copy-id:{general}")));
    host.key(Key::Escape, false, false);

    host.us_open();
    host.click("us:page=developer");
    host.click("us:developer");
    assert!(host.ui.prefs.developer);
    host.key(Key::Escape, false, false);
    host.us_message_menu(general);
    assert!(painted_has(&mut host, "Copiar ID"));
    assert!(host.us_next_run().prefs.developer);
}

#[test]
fn a_threshold_of_one_s_own_is_kept_and_decides_who_speaks() {
    let mut host = Host::new();
    host.us_open();
    // The voice page is taller than the test window: show just this row.
    host.us_write(SEARCH, "sensibilidad umbral");
    host.click("us:page=voice");
    assert!(!host.us_buttons().contains(&"us:sensitivity".to_owned()));
    host.click("us:auto-sensitivity");
    assert!(!host.ui.prefs.auto_sensitivity);
    host.frame();
    assert!(
        host.frame()
            .regions
            .iter()
            .any(|region| region.id == "us:sensitivity"),
        "its own threshold has a slider"
    );
    // While the microphone is tested, its level runs along the slider's
    // track, green over the threshold; the meter apart is gone.
    host.ui.audio.testing = true;
    host.ui.audio.levels.push(0.9);
    let frame = host.frame();
    let slider = frame
        .regions
        .iter()
        .find(|region| region.id == "us:sensitivity")
        .unwrap()
        .rect;
    let green = host.ui.look().online;
    assert!(frame.commands.iter().any(|command| matches!(
        command,
        limn::DrawCommand::FillRect { rect, color, .. } | limn::DrawCommand::FillCornerRect { rect, color, .. }
            if *color == green && rect.x >= slider.x && rect.y >= slider.y
                && rect.y + rect.height <= slider.y + slider.height
    )));
    assert!(!frame
        .access
        .iter()
        .any(|node| node.name == "Nivel del micrófono"));
    host.ui.audio.testing = false;
    host.us_slide("us:sensitivity", 0.3, true);
    let next = host.us_next_run();
    assert!((next.prefs.sensitivity - 0.3).abs() < 0.001);
    assert!(!next.prefs.auto_sensitivity);
    // What the call's microphone hears is judged by it.
    let start = Instant::now();
    let mut voice = rune_media::VoiceActivity::default();
    assert!(!voice.update(0.4, start), "under RUNE's own threshold");
    next.prefs.tune(&mut voice);
    assert!(voice.update(0.4, start), "over the account's");
}

#[test]
fn noise_suppression_offers_rnnoise_and_webrtc_honestly_and_survives_a_restart() {
    use rune_media::ProcessingMode;
    let suppression = |ui: &Ui| ui.prefs.effective_processing().suppression;
    let mut host = Host::new();
    assert_eq!(
        suppression(&host.ui),
        ProcessingMode::RnNoise,
        "voice isolation by default, as in Discord"
    );
    host.us_open();
    // Looking for Discord's suppressor finds the open ones, and says why.
    host.us_write(SEARCH, "krisp");
    host.click("us:page=voice");
    assert!(painted_has(&mut host, "Supresión de ruido"));
    assert!(painted_has(&mut host, "Quita el ruido de fondo."));
    let buttons = host.us_buttons();
    for id in [
        "us:suppression=rnnoise",
        "us:suppression=webrtc",
        "us:suppression=off",
    ] {
        assert!(buttons.contains(&id.to_owned()), "{id}");
    }
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "us:suppression=rnnoise".into(),
        },
        host.now,
    );
    assert!(intents.contains(&Intent::SaveSetting {
        key: "noise_suppression".into(),
        value: Some("rnnoise".into()),
    }));
    assert_eq!(suppression(&host.us_next_run()), ProcessingMode::RnNoise);
    host.us_press("us:suppression=webrtc");
    assert_eq!(suppression(&host.us_next_run()), ProcessingMode::WebRtc);
    host.us_press("us:suppression=off");
    assert_eq!(suppression(&host.us_next_run()), ProcessingMode::Off);
    let mut old = Ui::default();
    old.restore(|_| None);
    assert_eq!(
        suppression(&old),
        ProcessingMode::RnNoise,
        "nothing kept: the default profile"
    );
    // A saved choice, even an unreadable one, was one's own: off, not a preset.
    old.restore(|key| (key == "noise_suppression").then(|| "invalid".into()));
    assert_eq!(suppression(&old), ProcessingMode::Off);
}

#[test]
fn a_profile_sets_the_processing_and_touching_it_makes_it_custom() {
    use rune_media::ProcessingMode;
    let mut host = Host::new();
    host.us_open();
    host.us_write(SEARCH, "perfil");
    host.click("us:page=voice");
    for name in ["Aislamiento de voz", "Estudio", "Personalizado"] {
        assert!(painted_has(&mut host, name), "{name}");
    }
    host.us_press("us:profile=voice-isolation");
    let processing = host.ui.prefs.effective_processing();
    assert_eq!(processing.suppression, ProcessingMode::RnNoise);
    assert!(processing.echo_cancellation && processing.automatic_gain);
    assert_eq!(
        host.us_next_run().prefs.input_profile,
        crate::InputProfile::VoiceIsolation
    );
    // The processing section shows what the profile set.
    host.us_write(SEARCH, "ganancia automático");
    assert!(painted_has(&mut host, "Control automático de ganancia"));
    host.us_press("us:automatic-gain");
    let next = host.us_next_run();
    assert_eq!(next.prefs.input_profile, crate::InputProfile::Custom);
    let processing = next.prefs.effective_processing();
    assert_eq!(processing.suppression, ProcessingMode::RnNoise);
    assert!(processing.echo_cancellation && !processing.automatic_gain);
    host.us_press("us:profile=studio");
    assert_eq!(
        host.ui.prefs.effective_processing(),
        crate::Processing::default()
    );
    host.us_press("us:profile=custom");
    assert!(
        !host.ui.prefs.effective_processing().automatic_gain,
        "one's own values come back"
    );
}

#[test]
fn each_call_sound_turns_off_alone_plays_on_trial_and_a_folder_of_ones_own_is_kept() {
    use crate::CallSound;
    let mut host = Host::new();
    host.us_open();
    host.us_write(SEARCH, "sonidos");
    host.click("us:page=voice");
    assert!(painted_has(&mut host, "Sonidos de la llamada"));
    // Tall enough for the whole section.
    let buttons: Vec<String> = host
        .us_layout(Size::new(1280.0, 4000.0))
        .regions
        .iter()
        .map(|region| region.id.clone())
        .collect();
    for sound in CallSound::ALL {
        assert!(
            buttons.contains(&format!("us:sound={}", sound.key())),
            "{sound:?}"
        );
        assert!(buttons.contains(&format!("us:sound-test={}", sound.key())));
    }
    host.us_press("us:sound=user_join");
    host.us_press("us:sound=deafen");
    host.us_press("us:sound=message");
    host.us_press("us:sound-test=join");
    assert_eq!(
        crate::call_sounds_take_preview(&mut host.ui),
        Some(CallSound::Join)
    );
    assert_eq!(crate::call_sounds_take_preview(&mut host.ui), None, "once");
    host.us_slide("us:sound-volume", 0.35, true);
    // The folder: what reads as one is kept; the host says what it found.
    host.us_write("us:sound-folder", "sonidos");
    host.us_press("us:sound-folder:use");
    assert!(
        host.ui.prefs.sound_folder.is_none(),
        "not a path from / or ~/"
    );
    host.us_write("us:sound-folder", "~/sonidos ");
    host.us_press("us:sound-folder:use");
    assert_eq!(host.ui.prefs.sound_folder.as_deref(), Some("~/sonidos"));
    host.us_write(SEARCH, "carpeta de sonidos");
    assert!(painted_has(&mut host, "Leyendo la carpeta…"));
    crate::call_sounds_folder_read(
        &mut host.ui,
        crate::FolderReport {
            folder: "~/sonidos".into(),
            taken: vec![(CallSound::Mute, "mute.wav".into())],
            refused: vec![("leave.ogg".into(), "dura más de 5 s".into())],
            error: None,
        },
    );
    assert!(painted_has(&mut host, "Usa 1 de 19: mute.wav."));
    assert!(painted_has(
        &mut host,
        "leave.ogg: dura más de 5 s. Suena el de RUNE."
    ));
    host.us_write(SEARCH, "silenciar micrófono");
    assert!(painted_has(&mut host, "Suena mute.wav, de tu carpeta."));
    let next = host.us_next_run().prefs;
    assert_eq!(next.sounds_off, [CallSound::UserJoin, CallSound::Deafen]);
    assert!(!next.sound_on(CallSound::Message) && next.sound_on(CallSound::Join));
    assert_eq!(next.sound_volume, 0.35);
    assert_eq!(next.sound_folder.as_deref(), Some("~/sonidos"));
    host.us_press("us:sound-folder:clear");
    assert!(host.ui.prefs.sound_folder.is_none() && host.ui.call_sounds.found.is_none());
}

#[test]
fn the_input_volume_sits_beside_the_output_and_is_kept() {
    let mut host = Host::new();
    host.us_open();
    host.us_write(SEARCH, "volumen de entrada");
    host.click("us:page=voice");
    assert!(painted_has(&mut host, "Volumen de entrada"));
    assert!(painted_has(&mut host, "Volumen de salida"));
    host.us_slide("us:input-volume", 0.751, true);
    assert_eq!(host.ui.prefs.input_volume, 1.5);
    assert!(painted_has(&mut host, "150 %"));
    assert_eq!(host.us_next_run().prefs.input_volume, 1.5);
}

#[test]
fn microphone_test_distinguishes_opening_raw_and_processed_levels_and_stops_on_leave() {
    let mut host = Host::new();
    host.us_open();
    host.us_press("us:page=voice");
    assert!(!host.ui.audio.testing && !host.ui.audio.starting);
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "us:meter".into(),
        },
        host.now,
    );
    assert_eq!(intents, vec![Intent::StartMeter(None)]);
    assert!(host.ui.audio.testing && host.ui.audio.starting);
    assert!(painted_has(&mut host, "Abriendo micrófono…"));
    // A controlled worker result, without opening any physical device.
    host.ui.audio.starting = false;
    host.ui.audio.raw_levels = vec![0.8];
    host.ui.audio.levels = vec![0.2];
    // Under the devices and their volumes.
    host.us_press("us:jump=mic-test");
    let frame = host.frame();
    let bar_height = |name| {
        let bounds = frame
            .access
            .iter()
            .find(|node| node.name == name)
            .unwrap()
            .rect;
        frame
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::FillRect { rect, .. }
                    if rect.width == 4.0
                        && rect.x >= bounds.x
                        && rect.y >= bounds.y
                        && rect.x + rect.width <= bounds.x + bounds.width + 0.1
                        && rect.y + rect.height <= bounds.y + bounds.height + 0.1 =>
                {
                    Some(rect.height)
                }
                _ => None,
            })
            .fold(0.0, f32::max)
    };
    assert!(bar_height("Nivel de Entrada") > bar_height("Nivel de Procesada") + 10.0);
    assert!(!painted_has(&mut host, "Abriendo micrófono…"));
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "us:page=appearance".into(),
        },
        host.now,
    );
    assert!(intents.contains(&Intent::StopMeter));
    assert!(!host.ui.audio.testing && !host.ui.audio.starting);
    assert!(host.ui.audio.levels.is_empty() && host.ui.audio.raw_levels.is_empty());
}

#[test]
fn changing_microphone_clears_old_levels_and_sensitivity_follows_the_room_by_default() {
    let mut host = Host::new();
    host.us_open();
    host.us_press("us:page=voice");
    host.ui.audio.testing = true;
    host.ui.audio.levels = vec![0.9];
    host.ui.audio.raw_levels = vec![0.8];
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "us:mic=synthetic-device".into(),
        },
        host.now,
    );
    assert!(intents.contains(&Intent::StartMeter(Some("synthetic-device".into()))));
    assert!(host.ui.audio.starting);
    assert!(host.ui.audio.levels.is_empty() && host.ui.audio.raw_levels.is_empty());
    host.us_press("us:meter");
    host.us_write(SEARCH, "sensibilidad");
    assert!(painted_has(
        &mut host,
        "Determinar automáticamente la sensibilidad de entrada"
    ));
    // With RNNoise, the default, its gate decides, and says so.
    assert!(painted_has(&mut host, "Umbral: lo que suena como voz"));
    // Without it, where the room put it, as the host last heard it.
    host.ui.prefs.input_profile = crate::InputProfile::Studio;
    host.ui.audio.threshold = Some(0.5);
    assert!(painted_has(&mut host, "Umbral: -30 dBFS · automático"));
    host.us_press("us:auto-sensitivity");
    assert!(!host.us_next_run().prefs.auto_sensitivity);
}

#[test]
fn the_zoom_follows_its_slider_once_let_go() {
    let mut host = Host::new();
    host.us_open();
    host.click("us:page=accessibility");
    host.us_slide("us:zoom", 0.9, false);
    assert_eq!(host.ui.prefs.zoom, 100, "not while dragging");
    host.us_slide("us:zoom", 0.9, true);
    assert_eq!(host.ui.prefs.zoom, 175);
    assert!((host.ui.prefs.zoom_factor() - 1.75).abs() < 0.001);
    assert_eq!(host.us_next_run().prefs.zoom, 175);
}

#[test]
fn the_rail_marks_follow_the_notification_settings() {
    let mut host = Host::new();
    // Short pills at the rail's edge mark servers with something unread;
    // a badge counts the mentions.
    let marks = |host: &mut Host| {
        let frame = host.frame();
        let pills = frame
            .commands
            .iter()
            .filter(|command| {
                matches!(command, limn::DrawCommand::FillRect { rect, .. }
                    if rect.height == rune_components::rail::PILL_UNREAD
                        && rect.x < rune_components::rail::WIDTH)
            })
            .count();
        let mentions = frame.access.iter().any(|node| {
            node.description
                .as_deref()
                .is_some_and(|said| said.ends_with(" menciones"))
        });
        (pills, mentions)
    };
    let (pills, mentions) = marks(&mut host);
    assert!(
        pills > 0 && mentions,
        "the simulation has news: {pills} {mentions}"
    );
    host.us_open();
    host.click("us:page=notifications");
    host.click("us:unread-badges");
    host.click("us:mention-badges");
    assert!(!host.ui.prefs.unread_badges && !host.ui.prefs.mention_badges);
    host.key(Key::Escape, false, false);
    assert_eq!(marks(&mut host), (0, false));
    let next = host.us_next_run();
    assert!(!next.prefs.unread_badges && !next.prefs.mention_badges);
    // Desktop notifications are asked for, and kept with the rest.
    host.us_open();
    host.click("us:page=notifications");
    assert!(!host.ui.prefs.desktop_notifications && host.ui.prefs.message_sounds);
    host.click("us:desktop");
    host.click("us:notification-content");
    host.click("us:message-sound");
    let next = host.us_next_run();
    assert!(next.prefs.desktop_notifications);
    assert!(!next.prefs.notification_content && !next.prefs.message_sounds);
}

// Windows has no GPU engine yet: there it cannot be turned on (windows-medios).
#[cfg(not(windows))]
#[test]
fn the_gpu_does_the_video_until_it_is_turned_off_and_that_is_kept() {
    let mut host = Host::new();
    assert!(host.ui.prefs.video_hardware);
    host.us_open();
    // Found by what it is about, on the voice page.
    host.us_write(SEARCH, "aceleración por hardware");
    host.click("us:page=voice");
    host.click("us:video-hardware");
    assert!(!host.ui.prefs.video_hardware);
    assert!(!host.us_next_run().prefs.video_hardware);
}

#[cfg(windows)]
#[test]
fn on_windows_the_gpu_switch_says_the_processor_does_the_video() {
    let mut host = Host::new();
    host.us_open();
    host.us_write(SEARCH, "aceleración por hardware");
    host.click("us:page=voice");
    assert!(!host.us_buttons().iter().any(|id| id == "us:video-hardware"));
    let said = host.frame().commands.iter().any(|command| {
        matches!(command, limn::DrawCommand::Text { text, .. } if text == "En Windows, el video todavía se procesa en el procesador.")
    });
    assert!(said);
}

#[test]
fn the_older_settings_actions_still_work() {
    let mut host = Host::new();
    host.us_press("settings:page=audio");
    assert_eq!(host.ui.settings, Some(SettingsPage::Voice));
    host.us_press("settings:theme=light");
    assert_eq!(host.ui.theme, Theme::Light);
    host.us_press("settings:close");
    assert!(host.ui.settings.is_none());
}

#[test]
fn push_to_talk_records_one_key_and_recording_ends_on_cancel_page_or_timeout() {
    let mut host = Host::new();
    host.us_open();
    host.click("us:page=voice");
    assert!(!painted_has(&mut host, "Atajo de Pulsar para hablar"));
    host.us_press("us:ptt-record");
    assert!(
        !crate::recording_key(&host.ui),
        "voice activity has no key to record"
    );
    host.us_press("us:input=ptt");
    // The output rows above push the key's row below the test window.
    host.us_write(SEARCH, "atajo");
    assert!(painted_has(&mut host, "Todavía no tiene atajo"));
    // Pressing the button again, Esc or a lost focus (the host's `None`) cancel.
    host.us_press("us:ptt-record");
    assert!(crate::recording_key(&host.ui));
    host.us_press("us:ptt-record");
    assert!(!crate::recording_key(&host.ui));
    host.us_press("us:ptt-record");
    assert!(crate::record_key(&mut host.ui, None).is_empty());
    assert!(!crate::recording_key(&host.ui));
    assert_eq!(
        host.ui.prefs.push_to_talk_key, None,
        "a cancel changes nothing"
    );
    // A key ends it and is kept for the next run.
    host.us_press("us:ptt-record");
    assert!(painted_has(&mut host, "Esc cancela"));
    let saved = crate::record_key(&mut host.ui, Some("Ctrl+Shift+T".into()));
    assert_eq!(
        saved,
        vec![Intent::SaveSetting {
            key: "push_to_talk_key".into(),
            value: Some("Ctrl+Shift+T".into())
        }]
    );
    assert!(!crate::recording_key(&host.ui));
    assert!(
        crate::record_key(&mut host.ui, Some("F9".into())).is_empty(),
        "a key after recording ended is not taken"
    );
    let next = host.us_next_run();
    assert!(next.prefs.push_to_talk);
    assert_eq!(next.prefs.push_to_talk_key.as_deref(), Some("Ctrl+Shift+T"));
    // Waiting too long gives up, and the host is woken for it.
    host.us_press("us:ptt-record");
    let started = host.now;
    assert_eq!(
        host.ui.next_expiry(started),
        Some(crate::settings::RECORD_TIMEOUT)
    );
    assert!(!host.ui.expire(started + Duration::from_secs(9)));
    assert!(host.ui.expire(started + crate::settings::RECORD_TIMEOUT));
    assert!(!crate::recording_key(&host.ui));
    // Leaving the page or the settings ends it too.
    host.us_press("us:ptt-record");
    host.us_write(SEARCH, "");
    host.click("us:page=appearance");
    assert!(!crate::recording_key(&host.ui));
    host.click("us:page=voice");
    host.us_write(SEARCH, "atajo");
    host.us_press("us:ptt-record");
    host.us_press("us:close");
    assert!(!crate::recording_key(&host.ui));
}

#[test]
fn the_release_delay_moves_in_steps_of_ten_milliseconds_up_to_two_seconds() {
    let mut host = Host::new();
    host.us_open();
    host.us_press("us:input=ptt");
    host.us_write(SEARCH, "retraso");
    host.click("us:page=voice");
    assert!(painted_has(&mut host, "20 ms"), "Discord's default");
    host.us_slide("us:ptt-release", 0.1234, true);
    assert_eq!(host.ui.prefs.push_to_talk_release, 250);
    host.us_slide("us:ptt-release", 7.0, true);
    assert_eq!(host.ui.prefs.push_to_talk_release, 2000);
    assert_eq!(
        host.us_next_run().prefs.release_delay(),
        Duration::from_secs(2)
    );
}

#[test]
fn the_speaker_its_volume_and_the_sound_test_are_settings_that_stop_on_leave() {
    let mut host = Host::new();
    host.us_open();
    host.us_press("us:page=voice");
    let device = |id: &str, name: &str| rune_media::DeviceInfo {
        id: id.into(),
        name: name.into(),
        default: false,
    };
    host.ui.audio.devices = Some(Ok(rune_media::Devices {
        backend: "prueba".into(),
        inputs: vec![device("mic-a", "Micrófono A")],
        outputs: vec![device("out-a", "Parlantes"), device("out-b", "Auriculares")],
    }));
    host.us_press("us:select:speaker");
    assert_eq!(host.ui.settings_state.select, Some("speaker"));
    host.us_press("us:speaker=out-b");
    assert_eq!(host.ui.audio.output.as_deref(), Some("out-b"));
    assert_eq!(host.ui.settings_state.select, None);
    assert!(painted_has(&mut host, "Auriculares"));
    assert_eq!(host.us_next_run().audio.output.as_deref(), Some("out-b"));
    host.us_press("us:speaker-default");
    assert_eq!(host.ui.audio.output, None);

    assert!(painted_has(&mut host, "100 %"));
    host.us_slide("us:output-volume", 0.251, true);
    assert_eq!(host.ui.prefs.output_volume, 0.5);
    assert!(painted_has(&mut host, "50 %"));
    assert_eq!(host.us_next_run().prefs.output_volume, 0.5);

    // The output's test is a call sound, which goes into a call too.
    host.us_press("us:speaker-test");
    assert_eq!(host.ui.call_sounds.preview, Some(crate::CallSound::Join));
    host.ui.call_sounds.preview = None;
    host.us_slide("us:output-volume", 0.0, true);
    assert!(painted_has(
        &mut host,
        "Con el volumen de salida en 0 % no se oye."
    ));
    // Changed volumes can go back as they start.
    assert!(painted_has(&mut host, "Restablecer niveles"));
    host.us_press("us:reset-levels");
    assert_eq!(host.ui.prefs.output_volume, 1.0);
    assert_eq!(host.ui.prefs.input_volume, 1.0);
    assert!(!painted_has(&mut host, "Restablecer niveles"));
    host.us_press("us:loopback");
    assert!(
        host.ui.audio.testing && host.ui.audio.loopback,
        "it starts the microphone test"
    );
    host.us_press("us:page=appearance");
    assert!(!host.ui.audio.loopback && !host.ui.audio.testing);
}

#[test]
fn echo_cancellation_is_explicit_searchable_and_kept() {
    let mut host = Host::new();
    let echo = |ui: &Ui| ui.prefs.effective_processing().echo_cancellation;
    // One's own processing starts with it off.
    host.ui.prefs.input_profile = crate::InputProfile::Custom;
    assert!(!echo(&host.ui));
    host.us_open();
    host.us_write(SEARCH, "cancelación eco");
    host.click("us:page=voice");
    assert!(painted_has(&mut host, "Cancelación de eco"));
    host.us_press("us:echo-cancellation");
    assert!(echo(&host.ui));
    assert!(echo(&host.us_next_run()));
}

#[test]
fn cancelling_the_pcs_sound_is_on_searchable_honest_and_kept() {
    let mut host = Host::new();
    assert!(host.ui.prefs.system_echo, "on by default");
    host.us_open();
    host.us_write(SEARCH, "música juegos");
    host.click("us:page=voice");
    assert!(painted_has(&mut host, "Cancelar sonido del equipo"));
    // Whether the canceller hears the output is said as it is.
    host.ui.audio.system_echo = Some(false);
    assert!(painted_has(
        &mut host,
        "No se pudo escuchar tu salida: por ahora se cancela sólo lo que reproduce RUNE."
    ));
    host.us_press("us:system-echo");
    assert!(!host.ui.prefs.system_echo);
    assert!(!host.us_next_run().prefs.system_echo, "kept");
    // It needs echo cancellation, and says so.
    host.ui.prefs.input_profile = crate::InputProfile::Studio;
    assert!(painted_has(&mut host, "Necesita la cancelación de eco."));
}

#[test]
fn hearing_oneself_starts_the_test_and_a_blank_click_leaves_the_search_alone() {
    let mut host = Host::new();
    host.us_open();
    host.click("us:page=voice");
    host.click("us:meter");
    assert_eq!(host.tree.focused_id(), Some("us:meter"));
    // Blank space beside the test's button.
    let frame = host.frame();
    let meter = frame
        .regions
        .iter()
        .find(|r| r.id == "us:meter")
        .unwrap()
        .rect;
    let blank = Point::new(meter.x + meter.width + 300.0, meter.y + 20.0);
    host.send(UiEvent::PointerDown(blank));
    host.send(UiEvent::PointerUp(blank));
    host.frame();
    assert_ne!(host.tree.focused_id(), Some(SEARCH));
    // Stopped, «Escucharme» starts the test with it, as Discord's check does.
    host.us_press("us:meter");
    assert!(!host.ui.audio.testing);
    host.us_press("us:loopback");
    assert!(host.ui.audio.testing && host.ui.audio.loopback);
}

#[test]
fn the_menu_has_the_new_pages_and_old_keys_still_open_theirs() {
    let mut host = Host::new();
    host.us_open();
    let buttons = host.us_buttons();
    for key in ["keybinds", "desktop", "connection", "developer"] {
        assert!(buttons.contains(&format!("us:page={key}")), "{key}");
    }
    assert!(!buttons.contains(&"us:page=system".to_owned()));
    host.click("us:page=keybinds");
    assert_eq!(host.ui.settings, Some(SettingsPage::Keybinds));
    host.click("us:page=activity");
    assert_eq!(host.ui.settings, Some(SettingsPage::Activity));
    host.click("us:page=desktop");
    assert_eq!(host.ui.settings, Some(SettingsPage::Desktop));
    // The page that was «Sistema» and the one of the first shortcuts.
    host.us_press("us:page=system");
    assert_eq!(host.ui.settings, Some(SettingsPage::Desktop));
    host.us_press("us:page=shortcuts");
    assert_eq!(host.ui.settings, Some(SettingsPage::Keybinds));
}

#[test]
fn searching_finds_the_shortcuts_page_by_its_title() {
    let mut host = Host::new();
    host.us_open();
    host.us_write(SEARCH, "atajos");
    assert!(host.us_buttons().contains(&"us:page=keybinds".to_owned()));
    host.click("us:page=keybinds");
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "Atajos de teclado"));
}
