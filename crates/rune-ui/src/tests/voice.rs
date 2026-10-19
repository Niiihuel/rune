//! Voice, driven as a person would: joining and the panel through every
//! phase of the call, the connection's details, the stage with its tiles
//! and its bar, the menus and cards of the people in a call, and leaving.
//! The simulation's answers to the voice commands are given as it gives
//! them: its session runs on a runtime the host here does not have.

use rune_components::voice::ids;
use rune_components::Look;
use rune_model::{CallPhase, CallQuality, Permissions, UserId, VoiceState};
use rune_provider::{ConnectionState, Failure};

use super::*;

impl Host {
    /// Answer the voice commands sent from `from` on as the simulation
    /// does, its connection measured at `rtt` milliseconds.
    fn vz_answer(&mut self, from: usize, rtt: u32) {
        let me = self.store.me().unwrap();
        let commands = self.sent[from..].to_vec();
        for command in commands {
            match command {
                Command::JoinVoice {
                    channel,
                    muted,
                    deafened,
                } => {
                    if let Some(old) = self.store.my_voice().map(|state| state.channel) {
                        self.event(Event::VoiceLeft {
                            user: me,
                            channel: old,
                        });
                        self.event(Event::CallEnded {
                            channel: old,
                            reason: None,
                        });
                    }
                    self.event(Event::VoiceChanged(VoiceState {
                        user: me,
                        channel,
                        muted: muted || deafened,
                        deafened,
                        speaking: false,
                        streaming: false,
                        server_muted: false,
                        server_deafened: false,
                        camera: false,
                        suppressed: false,
                        speak_request: None,
                    }));
                    self.event(Event::CallPhase {
                        channel,
                        phase: CallPhase::Connected,
                        quality: Some(CallQuality {
                            rtt_ms: rtt,
                            loss_permille: 0,
                        }),
                    });
                }
                Command::SetVoiceSelf { muted, deafened } => {
                    if let Some(mut state) = self.store.my_voice().cloned() {
                        state.muted = muted || deafened;
                        state.deafened = deafened;
                        self.event(Event::VoiceChanged(state));
                    }
                }
                Command::LeaveVoice => {
                    if let Some(state) = self.store.my_voice().cloned() {
                        self.event(Event::VoiceLeft {
                            user: me,
                            channel: state.channel,
                        });
                        self.event(Event::CallEnded {
                            channel: state.channel,
                            reason: None,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    /// Join the voice channel `name` with a click, answered: its ID.
    fn vz_join(&mut self, name: &str) -> ChannelId {
        let channel = self.channel_named(name);
        let from = self.sent.len();
        self.click(&format!("voice:{channel}"));
        self.vz_answer(from, 47);
        channel
    }

    /// Every text on screen.
    fn vz_texts(&mut self) -> Vec<String> {
        self.frame()
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn vz_shows(&mut self, id: &str) -> bool {
        self.frame().regions.iter().any(|region| region.id == id)
    }

    /// Whether the control `id` is laid out and can be used.
    fn vz_usable(&mut self, id: &str) -> bool {
        self.frame()
            .regions
            .iter()
            .any(|region| region.id == id && region.kind == limn::HitKind::Button)
    }

    fn vz_rect(&mut self, id: &str) -> limn::Rect {
        self.frame()
            .regions
            .iter()
            .rev()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id} is not on screen"))
            .rect
    }

    /// A secondary click on the left part of `id`.
    fn vz_right_click(&mut self, id: &str) {
        let rect = self.vz_rect(id);
        let at = Point::new(rect.x + 12.0, rect.y + rect.height / 2.0);
        self.send(UiEvent::ContextMenu { position: at });
    }

    /// What the hint of `id` says, once the pointer has rested on it.
    fn vz_hint(&mut self, id: &str) -> Vec<String> {
        let rect = self.vz_rect(id);
        let center = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        self.send(UiEvent::PointerMove(center));
        self.layout_only();
        let later = Instant::now() + Duration::from_secs(3);
        self.tree
            .layout_at(WINDOW, later)
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn vz_me(&self) -> UserId {
        self.store.me().unwrap()
    }

    /// Someone else in `channel`.
    fn vz_other(&self, channel: ChannelId) -> UserId {
        let me = self.vz_me();
        self.store
            .voice_in(channel)
            .iter()
            .map(|state| state.user)
            .find(|user| *user != me)
            .expect("someone else is in the channel")
    }
}

#[test]
fn joining_shows_the_panel_through_every_phase_and_the_details_its_measures() {
    let mut host = Host::new();
    let text = host.ui.channel().unwrap();
    let room = host.channel_named("Sala común");
    host.click(&format!("voice:{room}"));
    assert_eq!(host.store.call().unwrap().phase, CallPhase::Joining);
    assert!(host.painted().contains(&"Conectando a la voz…".to_owned()));
    // The first click joins; the conversation stays where it was.
    assert_eq!(host.ui.voice.stage, None);
    assert_eq!(host.ui.channel(), Some(text));

    host.vz_answer(0, 47);
    let painted = host.painted();
    assert!(painted.contains(&"Voz conectada (simulada)".to_owned()));
    // Where: the channel and its server (cut short in a narrow column).
    assert!(painted
        .iter()
        .any(|text| text.starts_with("Sala común / Ta")));
    assert!(painted.contains(&"00:00".to_owned()), "the call's clock");
    // The latency is the signal's hint.
    assert!(host.vz_hint(ids::DETAILS).contains(&"47 ms".to_owned()));

    // A second measure; the details average them and say they are made up.
    host.event(Event::CallPhase {
        channel: room,
        phase: CallPhase::Connected,
        quality: Some(CallQuality {
            rtt_ms: 41,
            loss_permille: 3,
        }),
    });
    host.click(ids::DETAILS);
    let painted = host.painted();
    for expected in [
        "Conexión",
        "Latencia promedio:",
        "44 ms",
        "Última latencia:",
        "41 ms",
        "Pérdida de paquetes:",
        "0,3 %",
        "Valores simulados:",
    ] {
        assert!(
            painted.iter().any(|text| text.starts_with(expected)),
            "{expected} in {painted:?}"
        );
    }
    host.key(Key::Escape, false, false);
    assert!(!host.ui.voice.details, "Escape closes the details");
    assert!(!host.painted().contains(&"Conexión".to_owned()));

    // The connection drops: the call says it reconnects.
    host.event(Event::Connection(ConnectionState::Reconnecting {
        attempt: 2,
        retry_in_ms: 1_000,
    }));
    let painted = host.painted();
    assert!(painted.contains(&"Reconectando la voz…".to_owned()));
    assert!(painted
        .iter()
        .any(|text| text.starts_with("Intento 2 · Sala común")));

    // It fails: why, and a way to try again.
    host.event(Event::Failed {
        failure: Failure::Voice {
            channel: Some(room),
        },
        reason: "El servidor de voz no responde".into(),
        ambiguous: false,
    });
    let painted = host.painted();
    assert!(painted.contains(&"No se pudo conectar la voz".to_owned()));
    assert!(painted.contains(&"El servidor de voz no responde".to_owned()));
    let from = host.sent.len();
    host.click(ids::RETRY);
    assert!(matches!(
        host.sent[from..],
        [Command::JoinVoice { channel, .. }] if channel == room
    ));
    assert_eq!(host.store.call().unwrap().phase, CallPhase::Joining);
}

#[test]
fn a_stage_is_joined_and_without_speaking_the_call_only_listens() {
    let mut host = Host::new();
    let noted = |host: &Host, detail: &str| {
        host.ui
            .notices
            .iter()
            .any(|notice| notice.detail.as_deref() == Some(detail))
    };
    // The simulation's stage (listed under the rooms, past the window's
    // bottom here) is joined like a room: see `tests::stage`.
    let stage = host.channel_named("Escenario (simulado)");
    let from = host.sent.len();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("voice:{stage}"),
        },
        host.now,
    );
    host.run(intents);
    assert!(
        matches!(host.sent[from..], [Command::JoinVoice { channel, .. }] if channel == stage),
        "{:?}",
        &host.sent[from..]
    );
    assert!(!noted(&host, rune_state::NO_STAGES));

    // A room where the account may connect but not speak.
    let room = host.channel_named("Sala común");
    let mut silent = host.store.channel(room).unwrap().clone();
    silent.permissions = silent.permissions.without(Permissions::SPEAK);
    host.event(Event::ChannelChanged(silent));
    let from = host.sent.len();
    host.click(&format!("voice:{room}"));
    assert!(matches!(
        host.sent[from..],
        [Command::JoinVoice { muted: true, .. }]
    ));
    assert!(noted(&host, "No tenés permiso para hablar en #Sala común"));
    // Unmuting is refused with the reason; nothing goes to the provider.
    host.vz_answer(from, 47);
    let from = host.sent.len();
    host.click("me:mute");
    assert!(host.sent[from..].is_empty());
    assert!(host.ui.muted);
}

#[test]
fn the_stage_lists_who_is_in_the_call_and_who_speaks_or_is_muted() {
    let mut host = Host::new();
    let text = host.ui.channel().unwrap();
    let room = host.vz_join("Sala común");
    // Clicking the channel of the call opens its stage over the conversation.
    host.click(&format!("voice:{room}"));
    assert_eq!(host.ui.voice.stage, Some(room));
    assert_eq!(host.ui.channel(), Some(text), "the conversation is kept");
    assert!(!host.vz_shows(&editor_id(text)), "the stage covers it");
    let me = host.vz_me();
    let people: Vec<UserId> = host
        .store
        .voice_in(room)
        .iter()
        .map(|state| state.user)
        .collect();
    assert_eq!(people.len(), 4, "three were there before the account");
    for user in &people {
        assert!(host.vz_shows(&ids::tile(user.0)), "a tile for {user:?}");
        assert!(host.vz_shows(&ids::row(user.0)), "a row for {user:?}");
    }
    // The account's tile comes first.
    let mine = host.vz_rect(&ids::tile(me.0));
    let other = host.vz_other(room);
    let theirs = host.vz_rect(&ids::tile(other.0));
    assert!((mine.y, mine.x) < (theirs.y, theirs.x));
    assert!(host.painted().contains(&"Sala común".to_owned()));

    // Who speaks is outlined in green, on the stage and in the column.
    host.event(Event::Speaking {
        channel: room,
        user: other,
        speaking: true,
    });
    let look = Look::DARK;
    let frame = host.frame();
    let outlined = |rect: limn::Rect| {
        frame.commands.iter().any(|command| {
            matches!(command, limn::DrawCommand::StrokeRect { rect: drawn, color, .. }
                if *color == look.online && drawn.x == rect.x && drawn.y == rect.y)
        })
    };
    assert!(outlined(theirs), "their tile is outlined while they speak");
    assert!(!outlined(mine));

    // Muted and deafened people are marked, each mark with its hint.
    host.event(Event::VoiceChanged(VoiceState {
        user: other,
        channel: room,
        muted: true,
        deafened: true,
        speaking: false,
        streaming: false,
        server_muted: false,
        server_deafened: false,
        camera: false,
        suppressed: false,
        speak_request: None,
    }));
    for place in [ids::tile(other.0), ids::row(other.0)] {
        assert!(host.vz_shows(&format!("{place}:muted")));
        assert!(host.vz_shows(&format!("{place}:deafened")));
    }
    assert!(host
        .vz_hint(&format!("{}:muted", ids::row(other.0)))
        .contains(&"Silenciado".to_owned()));
    assert!(!host.vz_shows(&format!("{}:muted", ids::tile(me.0))));
}

// Screen and camera are Linux's and Windows' (windows-medios).
#[cfg(any(target_os = "linux", windows))]
#[test]
fn the_bar_mutes_deafens_and_hangs_up_and_the_video_says_it_stays_here() {
    let mut host = Host::new();
    let text = host.ui.channel().unwrap();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let me = host.vz_me();

    let from = host.sent.len();
    host.click(ids::MUTE);
    host.click(ids::DEAFEN);
    host.click(ids::DEAFEN);
    host.click(ids::MUTE);
    let set: Vec<(bool, bool)> = host.sent[from..]
        .iter()
        .map(|command| match command {
            Command::SetVoiceSelf { muted, deafened } => (*muted, *deafened),
            other => panic!("{other:?}"),
        })
        .collect();
    // Deafening mutes too; hearing again gives back the microphone as it was.
    assert_eq!(
        set,
        [(true, false), (true, true), (true, false), (false, false)]
    );
    host.click(ids::DEAFEN);
    host.vz_answer(from, 47);
    assert!(host.vz_shows(&format!("{}:deafened", ids::tile(me.0))));
    assert!(host
        .vz_hint(ids::DEAFEN)
        .contains(&"Desactivar ensordecimiento".to_owned()));

    // The camera goes to the (simulated) call; the screen too, after
    // choosing its quality. Their hints say so.
    for (id, note) in [
        (ids::CAMERA, "Se transmite a la llamada."),
        (ids::SCREEN, "Elegí la calidad; se transmite a la llamada."),
    ] {
        assert!(host.vz_shows(id) && host.vz_usable(id), "{id} is usable");
        assert!(host.vz_hint(id).contains(&note.to_owned()), "{id}");
    }
    // The simulated rooms have a chat of their own.
    assert!(host.vz_usable(ids::CHAT));
    assert!(host.vz_hint(ids::CHAT).contains(&"Mostrar chat".to_owned()));

    // «Más opciones» leads to the voice settings.
    host.click(ids::MORE);
    host.click(ids::VOICE_SETTINGS);
    assert_eq!(host.ui.settings, Some(SettingsPage::Voice));
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.settings, None);

    // Hanging up shows the conversation the stage covered again.
    let from = host.sent.len();
    host.click(ids::LEAVE);
    assert_eq!(host.sent[from..], [Command::LeaveVoice]);
    assert!(host.store.call().is_none());
    assert_eq!(host.ui.voice.stage, None);
    assert_eq!(host.ui.channel(), Some(text));
    assert!(host.vz_shows(&editor_id(text)));
    host.vz_answer(from, 47);
    assert!(
        !host.vz_shows(&ids::row(me.0)),
        "the account left the column"
    );
}

#[test]
fn a_secondary_click_on_yourself_offers_your_microphone_profile_and_id() {
    let mut host = Host::new();
    let text = host.ui.channel().unwrap();
    let room = host.vz_join("Sala común");
    let me = host.vz_me();

    // Copying the ID is for developer mode; Escape closes the menu.
    host.vz_right_click(&ids::row(me.0));
    assert!(host.vz_shows(ids::MENU_MUTE));
    assert!(!host.vz_shows(&format!("{}{}", ids::COPY_ID, me.0)));
    host.key(Key::Escape, false, false);
    assert!(host.ui.voice.menu.is_none());
    assert!(!host.vz_shows(ids::MENU_MUTE));

    host.ui.prefs.developer = true;
    host.vz_right_click(&ids::row(me.0));
    for id in [
        format!("{}{}", ids::PROFILE, me.0),
        format!("{}{}", ids::MENTION, me.0),
        ids::MENU_MUTE.to_owned(),
        ids::MENU_DEAFEN.to_owned(),
        rune_components::profile::ids::EDIT.to_owned(),
        format!("{}{}", ids::COPY_ID, me.0),
    ] {
        assert!(host.vz_shows(&id), "{id} in the menu");
    }
    let from = host.sent.len();
    host.click(ids::MENU_MUTE);
    assert_eq!(
        host.sent[from..],
        [Command::SetVoiceSelf {
            muted: true,
            deafened: false
        }]
    );
    assert!(host.ui.voice.menu.is_none(), "choosing closes it");

    // «Perfil» opens the card beside the row.
    host.vz_right_click(&ids::row(me.0));
    host.click(&format!("{}{}", ids::PROFILE, me.0));
    assert_eq!(host.ui.voice.card, Some((me, room, false)));
    assert!(
        host.painted().contains(&"invitado".to_owned()),
        "the handle"
    );
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.voice.card, None);

    host.vz_right_click(&ids::row(me.0));
    host.click(&format!("{}{}", ids::COPY_ID, me.0));
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title == "ID copiado"));

    // Someone else's menu has none of the account's own items.
    let other = host.vz_other(room);
    host.vz_right_click(&ids::row(other.0));
    assert!(host.vz_shows(&format!("{}{}", ids::PROFILE, other.0)));
    assert!(!host.vz_shows(ids::MENU_MUTE));
    // «Mención» writes them into the conversation's composer.
    host.click(&format!("{}{}", ids::MENTION, other.0));
    let handle = host.store.user(other).unwrap().name.clone();
    assert_eq!(host.ui.drafts.get(&text), Some(&format!("@{handle} ")));

    // On the stage, the «⋯» of a tile opens the same menu, without «Mención».
    host.click(&format!("voice:{room}"));
    host.click(&ids::tile_more(me.0));
    assert!(host.vz_shows(ids::MENU_DEAFEN));
    assert!(!host.vz_shows(&format!("{}{}", ids::MENTION, me.0)));
    host.click(ids::MENU_DEAFEN);
    assert!(host.ui.deafened);
    // A secondary click on a tile too.
    host.vz_right_click(&ids::tile(other.0));
    assert_eq!(host.ui.voice.menu.map(|menu| menu.user), Some(other));
    host.click(&format!("{}{}", ids::PROFILE, other.0));
    assert_eq!(host.ui.voice.card, Some((other, room, true)));
}

#[test]
fn the_room_s_chat_shows_beside_the_stage_and_hides_back_to_the_conversation() {
    let mut host = Host::new();
    let text = host.ui.channel().unwrap();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    host.click(ids::CHAT);
    assert_eq!(
        host.ui.channel(),
        Some(room),
        "the room's chat is the conversation"
    );
    assert_eq!(host.ui.voice.stage, Some(room), "beside the stage");
    assert!(host.vz_shows(ids::LEAVE));
    assert!(host.vz_shows(&crate::timeline_id(room)) && host.vz_shows(&editor_id(room)));
    assert!(
        host.store
            .timeline(room)
            .is_some_and(|timeline| !timeline.is_empty()),
        "its history loaded"
    );
    assert!(host.vz_hint(ids::CHAT).contains(&"Ocultar chat".to_owned()));
    // Written in like any conversation.
    let from = host.sent.len();
    host.type_text("hola desde la voz");
    host.key(Key::Enter, false, false);
    assert!(host.sent[from..].iter().any(|command| matches!(
        command,
        Command::SendMessage { channel, .. } if *channel == room
    )));
    host.click(ids::CHAT);
    assert_eq!(host.ui.channel(), Some(text), "back to what was open");
    assert_eq!(host.ui.voice.stage, Some(room));
    assert!(!host.vz_shows(&crate::timeline_id(room)));

    // Hanging up with the chat open goes back to the conversation too.
    host.click(ids::CHAT);
    host.click(ids::LEAVE);
    assert_eq!(host.ui.channel(), Some(text));
    assert_eq!(host.ui.voice.stage, None);
}

#[test]
fn a_narrow_window_keeps_the_stage_alone_and_says_why() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    host.click(ids::CHAT);
    let frame = host.vz_frame_at(Size::new(900.0, 700.0));
    assert!(!frame
        .regions
        .iter()
        .any(|region| region.id == crate::timeline_id(room)));
    let chat = frame.regions.iter().find(|region| region.id == ids::CHAT);
    assert!(chat.is_some_and(|region| region.kind != limn::HitKind::Button));
}

#[test]
fn a_person_s_volume_and_silence_stay_on_this_device() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let other = host.vz_other(room);
    host.vz_right_click(&ids::tile(other.0));
    assert!(host.vz_shows(&ids::volume(other.0)));
    assert!(host
        .painted()
        .contains(&"Simulado: la llamada no lleva audio".to_owned()));
    let from = host.sent.len();
    let slide = |fraction, committed| UiAction::SliderChanged {
        id: ids::volume(other.0),
        fraction,
        committed,
    };
    let intents = update(&mut host.ui, &mut host.store, &slide(0.3, false), host.now);
    assert!(intents.is_empty(), "kept once let go");
    assert_eq!(host.ui.prefs.voice_volume(other.0), (0.6, false));
    let intents = update(&mut host.ui, &mut host.store, &slide(0.75, true), host.now);
    assert_eq!(
        intents,
        [Intent::SaveSetting {
            key: "voice_volumes".into(),
            value: Some(format!("{}:1.5:0", other.0)),
        }]
    );
    assert!(host.ui.voice.menu.is_some(), "sliding keeps the menu open");
    host.click(&ids::local_mute(other.0));
    assert_eq!(host.ui.prefs.voice_volume(other.0), (1.5, true));
    assert!(host.vz_shows(&format!("{}:silenced", ids::tile(other.0))));
    assert!(host.vz_shows(&format!("{}:silenced", ids::row(other.0))));
    assert!(host.sent[from..].is_empty(), "nothing goes to the provider");
    // Back to the usual, it is forgotten.
    host.vz_right_click(&ids::tile(other.0));
    host.click(&ids::local_mute(other.0));
    let intents = update(&mut host.ui, &mut host.store, &slide(0.5, true), host.now);
    assert_eq!(
        intents,
        [Intent::SaveSetting {
            key: "voice_volumes".into(),
            value: Some(String::new()),
        }]
    );
    assert!(host.ui.prefs.voice_volumes.is_empty());
    // The account has no volume of its own.
    let me = host.vz_me();
    host.vz_right_click(&ids::tile(me.0));
    assert!(!host.vz_shows(&ids::volume(me.0)));
}

#[test]
fn the_stage_gives_way_to_a_conversation_and_invites_back_once_the_call_ends() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    // The column marks the stage's channel as the one open.
    let help = host.channel_named("ayuda");
    host.click(&format!("channel:{help}"));
    assert_eq!(host.ui.voice.stage, None);
    assert_eq!(host.ui.channel(), Some(help));
    assert!(host.store.call().is_some(), "the call goes on");

    // The call is ended from elsewhere: the stage stays, inviting back.
    host.click(&format!("voice:{room}"));
    host.event(Event::CallEnded {
        channel: room,
        reason: Some("Te desconectaron del canal".into()),
    });
    assert_eq!(host.ui.voice.stage, Some(room));
    let painted = host.painted();
    assert!(painted.contains(&"Unirse al chat de voz".to_owned()));
    assert!(!host.vz_shows(ids::LEAVE), "no bar out of the call");
    let from = host.sent.len();
    host.click(ids::JOIN);
    assert!(matches!(
        host.sent[from..],
        [Command::JoinVoice { channel, .. }] if channel == room
    ));
    host.vz_answer(from, 47);
    assert!(host.vz_shows(ids::LEAVE));

    // Another voice channel moves the call, and the stage follows it.
    let pair = host.channel_named("Pair programming");
    let from = host.sent.len();
    // The column's rows reach under the voice panel since the events'
    // entry (eventos-programados): bring it into view.
    host.tree.reveal_at_start(&format!("voice:{pair}"), 0.0);
    host.click(&format!("voice:{pair}"));
    host.vz_answer(from, 47);
    assert_eq!(host.ui.voice.stage, Some(pair));
    let me = host.vz_me();
    assert!(host
        .store
        .voice_in(pair)
        .iter()
        .any(|state| state.user == me));
    assert!(host
        .store
        .voice_in(room)
        .iter()
        .all(|state| state.user != me));
    assert!(host.vz_shows(&ids::tile(me.0)), "alone on the new stage");
}

impl Host {
    /// The window laid out at `size`.
    fn vz_frame_at(&mut self, size: Size) -> Frame {
        let context = ViewContext {
            window: size,
            ..self.context()
        };
        self.tree.set_root(view(&self.ui, &self.store, &context));
        self.tree.layout(size)
    }

    /// Bring members of `room`'s server into it until `count` are there.
    fn vz_fill(&mut self, room: ChannelId, count: usize) {
        let server = self.store.channel(room).unwrap().server.unwrap();
        let members: Vec<UserId> = self
            .store
            .members(server)
            .iter()
            .map(|member| member.user)
            .collect();
        for user in members {
            if self.store.voice_in(room).len() >= count {
                break;
            }
            self.event(Event::VoiceChanged(VoiceState {
                user,
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
        }
        assert_eq!(self.store.voice_in(room).len(), count, "enough members");
    }
}

fn vz_rect_in(frame: &Frame, id: &str) -> limn::Rect {
    frame
        .regions
        .iter()
        .rev()
        .find(|region| region.id == id)
        .unwrap_or_else(|| panic!("{id} is not laid out"))
        .rect
}

#[test]
fn the_stage_keeps_its_controls_inside_the_narrowest_window() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    // A name as long as the simulation takes, beside the mark of a muted
    // microphone.
    let me = host.vz_me();
    let mut account = host.store.user(me).unwrap().clone();
    account.display_name = Some("Maximiliano Fernández González".into());
    host.event(Event::UserChanged(account));
    host.click(ids::MUTE);
    let check = |frame: &Frame, size: Size, people: &[UserId]| {
        // The header keeps its padding: the chat's button is inside it.
        let chat = vz_rect_in(frame, ids::CHAT);
        assert!(
            chat.x + chat.width <= size.width - 11.5,
            "{chat:?} at {size:?}"
        );
        for user in people {
            let tile = vz_rect_in(frame, &ids::tile(user.0));
            let more = vz_rect_in(frame, &ids::tile_more(user.0));
            assert!(
                more.x >= tile.x && more.x + more.width <= tile.x + tile.width + 0.5,
                "the «⋯» of {user:?} out of its tile at {size:?}: {more:?} in {tile:?}"
            );
            assert!(tile.x + tile.width <= size.width, "{tile:?} at {size:?}");
        }
    };
    let people: Vec<UserId> = host.store.voice_in(room).iter().map(|s| s.user).collect();
    for height in [480.0, 560.0, 800.0] {
        let size = Size::new(720.0, height);
        let frame = host.vz_frame_at(size);
        check(&frame, size, &people);
        // The note is cut short, never dropped: the call is said to be simulated.
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            limn::DrawCommand::Text { text, .. } if text.starts_with("Llamada simulada")
        )));
    }
    // Thirteen people, three across.
    host.vz_fill(room, 13);
    let people: Vec<UserId> = host.store.voice_in(room).iter().map(|s| s.user).collect();
    let size = Size::new(720.0, 560.0);
    let frame = host.vz_frame_at(size);
    check(&frame, size, &people);
}

#[test]
fn the_stage_stays_in_front_when_the_window_reopens_a_conversation_itself() {
    let mut host = Host::new();
    let text = host.ui.channel().unwrap();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    // Ready again after a reconnection: the call goes on, and so does its stage.
    host.event(Event::Connection(ConnectionState::Reconnecting {
        attempt: 1,
        retry_in_ms: 1_000,
    }));
    let snapshot = host.provider.snapshot();
    host.event(Event::Ready(Box::new(snapshot)));
    assert!(host.store.call().is_some());
    assert_eq!(host.ui.voice.stage, Some(room));
    assert_eq!(host.ui.channel(), Some(text));
    // The conversation under it is removed: another takes its place, under it.
    host.event(Event::ChannelRemoved { channel: text });
    assert_ne!(host.ui.channel(), Some(text));
    assert_eq!(host.ui.voice.stage, Some(room));
    // Opening one is what takes it away.
    let help = host.channel_named("ayuda");
    host.click(&format!("channel:{help}"));
    assert_eq!(host.ui.voice.stage, None);
}

#[test]
fn out_of_the_call_more_than_three_people_are_counted_not_named() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    host.vz_fill(room, 9);
    let me = host.vz_me();
    host.event(Event::VoiceLeft {
        user: me,
        channel: room,
    });
    host.event(Event::CallEnded {
        channel: room,
        reason: None,
    });
    assert_eq!(host.ui.voice.stage, Some(room));
    let size = Size::new(720.0, 600.0);
    let frame = host.vz_frame_at(size);
    let (position, text) = frame
        .commands
        .iter()
        .find_map(|command| match command {
            limn::DrawCommand::Text { position, text, .. } if text.contains("chat de voz") => {
                Some((*position, text.clone()))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(text, "8 personas están en el chat de voz");
    // The test measures a character as half its size.
    assert!(position.x + text.chars().count() as f32 * 7.0 <= size.width);
}

// RUNE's own picker (windows-medios): what the host lists is offered,
// the main screen chosen until another is, a choice kept while listed.
#[test]
fn rune_s_own_picker_offers_what_the_host_listed() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    host.click(ids::SCREEN);
    assert!(host.ui.video.picker);
    let listed = |windows: &[(&str, &str)]| {
        let mut listed = vec![(
            r"screen:\\.\DISPLAY1".to_owned(),
            "Pantalla 1".to_owned(),
            false,
        )];
        listed.extend(
            windows
                .iter()
                .map(|(id, name)| ((*id).to_owned(), (*name).to_owned(), true)),
        );
        listed
    };
    crate::video_sources_listed(&mut host.ui, listed(&[("window:42", "Un juego")]));
    assert_eq!(
        host.ui.video.source.as_deref(),
        Some(r"screen:\\.\DISPLAY1")
    );
    assert!(host.vz_usable("vz:source:window:42"));
    host.click("vz:source:window:42");
    assert_eq!(host.ui.video.source.as_deref(), Some("window:42"));
    assert!(host.ui.video.picker, "choosing keeps it open");
    // An id the host never listed changes nothing.
    crate::video::choose_source(&mut host.ui, "window:7");
    assert_eq!(host.ui.video.source.as_deref(), Some("window:42"));
    // Listed again with it still there, it stays; gone, the main screen.
    crate::video_sources_listed(&mut host.ui, listed(&[("window:42", "Un juego")]));
    assert_eq!(host.ui.video.source.as_deref(), Some("window:42"));
    crate::video_sources_listed(&mut host.ui, listed(&[]));
    assert_eq!(
        host.ui.video.source.as_deref(),
        Some(r"screen:\\.\DISPLAY1")
    );
    // Where the system has its own dialog nothing is listed or chosen.
    crate::video_sources_listed(&mut host.ui, Vec::new());
    assert_eq!(host.ui.video.source, None);
}

// Screen and camera are Linux's and Windows' (windows-medios).
#[cfg(any(target_os = "linux", windows))]
#[test]
fn the_screen_and_camera_show_while_wanted_and_stop_with_the_call() {
    use rune_model::{StreamPhase, StreamQuality};
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    assert_eq!(host.ui.video.wanted(&host.store), (false, false));

    // The screen asks for its quality first: 720p30 unless chosen.
    host.click(ids::SCREEN);
    assert!(host.ui.video.picker && host.vz_shows(ids::SHARE));
    assert_eq!(host.ui.video.quality, StreamQuality::default());
    host.click("vz:quality:1080");
    host.click("vz:fps:60");
    assert!(host.ui.video.picker, "choosing keeps it open");
    // The applications' sound goes along unless turned off there.
    assert!(host.vz_usable(ids::SOUND) && !host.ui.video.silent);
    host.click(ids::SOUND);
    assert!(host.ui.video.silent && host.ui.video.picker);
    host.click(ids::SOUND);
    assert!(!host.ui.video.silent);
    // The screen the system remembered is kept, and can be chosen anew.
    assert!(!host.vz_shows(ids::PICK_SCREEN));
    let saved = crate::video_screen_chosen(&mut host.ui, "portal-choice".into());
    assert!(matches!(
        &saved[..],
        [Intent::SaveSetting { key, value: Some(token) }]
            if key == "screen_restore" && token == "portal-choice"
    ));
    assert!(
        crate::video_screen_chosen(&mut host.ui, "portal-choice".into()).is_empty(),
        "kept already"
    );
    assert!(host.vz_usable(ids::PICK_SCREEN));
    host.click(ids::PICK_SCREEN);
    assert!(host.ui.prefs.screen_restore.is_none() && host.ui.video.picker);
    assert_eq!(host.ui.video.wanted(&host.store), (false, false));
    let from = host.sent.len();
    host.click(ids::SHARE);
    let quality = StreamQuality {
        height: 1080,
        fps: 60,
    };
    // The system's dialog first, captured at the quality chosen; the stream
    // only once the person chose there and frames arrive, and only once.
    assert!(host.sent.len() == from && host.store.sharing().is_none());
    assert!(!host.ui.video.picker);
    assert_eq!(
        crate::video_capture_quality(&host.ui, &host.store),
        Some(quality)
    );
    assert!(crate::video_screen_ready(&host.ui, &mut host.store, false).is_empty());
    let started = crate::video_screen_ready(&host.ui, &mut host.store, true);
    host.run(started);
    assert_eq!(
        host.sent[from..],
        [Command::StartStream {
            channel: room,
            quality
        }]
    );
    assert!(crate::video_screen_ready(&host.ui, &mut host.store, true).is_empty());
    host.click(ids::CAMERA);
    assert_eq!(host.ui.video.wanted(&host.store), (true, true));
    assert_eq!(host.ui.video.screen.live, rune_ui_live_waiting());
    // Their previews come up on the stage, each with its way to stop; the
    // screen's says how far its stream got, and that it is simulated.
    for id in [ids::STOP_SCREEN, ids::STOP_CAMERA] {
        assert!(host.vz_shows(id) && host.vz_usable(id), "{id}");
    }
    let me = host.store.me().unwrap();
    host.event(Event::StreamPhase {
        channel: room,
        owner: me,
        phase: StreamPhase::Live,
    });
    host.ui.video.screen.sent = Some((58, 2_400));
    assert!(host
        .vz_texts()
        .contains(&"En vivo · 58 fps · 2400 kbit/s · simulada".to_owned()));
    // And who encodes it, as the host says (the preview cuts what is past it).
    host.ui.video.screen.encoder = Some("VA-API");
    let texts = host.vz_texts();
    assert!(
        texts
            .iter()
            .any(|text| text.starts_with("En vivo · 58 fps · 2400 kbit/s · VA-API")),
        "{texts:?}"
    );
    assert!(host
        .vz_hint(ids::SCREEN)
        .contains(&"Dejar de compartir".to_owned()));
    assert!(crate::video_sound(&host.ui, &host.store), "read while live");
    // «Pausar» while live, «Reanudar» once the provider says it is paused.
    assert!(host.vz_usable(ids::PAUSE) && !host.vz_shows(ids::RESUME));
    let from = host.sent.len();
    host.click(ids::PAUSE);
    assert_eq!(
        host.sent[from..],
        [Command::PauseStream {
            channel: room,
            paused: true
        }]
    );
    host.event(Event::StreamPhase {
        channel: room,
        owner: me,
        phase: StreamPhase::Paused,
    });
    assert!(host.vz_texts().contains(&"En pausa · simulada".to_owned()));
    assert!(
        !crate::video_sound(&host.ui, &host.store),
        "not while paused"
    );
    assert!(host.vz_usable(ids::RESUME) && !host.vz_shows(ids::PAUSE));
    let from = host.sent.len();
    host.click(ids::RESUME);
    assert_eq!(
        host.sent[from..],
        [Command::PauseStream {
            channel: room,
            paused: false
        }]
    );
    host.event(Event::StreamPhase {
        channel: room,
        owner: me,
        phase: StreamPhase::Live,
    });

    let from = host.sent.len();
    host.click(ids::STOP_SCREEN);
    assert_eq!(host.sent[from..], [Command::StopStream { channel: room }]);
    assert_eq!(host.ui.video.wanted(&host.store), (false, true));
    assert!(!host.vz_shows(ids::STOP_SCREEN) && host.store.sharing().is_none());

    // Leaving the system's dialog before choosing asks the provider nothing.
    let from = host.sent.len();
    host.click(ids::SCREEN);
    host.click(ids::SHARE);
    let intents = crate::video_ended(&mut host.ui, &mut host.store, true, None, Instant::now());
    assert!(intents.is_empty() && host.sent.len() == from);
    assert!(!host.ui.video.screen.on && host.store.sharing().is_none());

    // The system ending a source says why, turns it off and stops the stream.
    host.click(ids::SCREEN);
    host.click(ids::SHARE);
    let started = crate::video_screen_ready(&host.ui, &mut host.store, true);
    host.run(started);
    let intents = crate::video_ended(
        &mut host.ui,
        &mut host.store,
        true,
        Some("El sistema terminó la sesión.".into()),
        Instant::now(),
    );
    assert!(matches!(
        intents[..],
        [Intent::Command(Command::StopStream { .. })]
    ));
    assert!(!host.ui.video.screen.on);
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.detail.as_deref() == Some("El sistema terminó la sesión.")));

    // The service ending it says why, and turns the screen off.
    host.click(ids::SCREEN);
    host.click(ids::SHARE);
    let started = crate::video_screen_ready(&host.ui, &mut host.store, true);
    host.run(started);
    host.event(Event::StreamPhase {
        channel: room,
        owner: me,
        phase: StreamPhase::Ended(Some("Discord no te dejó transmitir en este canal.".into())),
    });
    assert!(!host.ui.video.screen.on);
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.detail.as_deref()
            == Some("Discord no te dejó transmitir en este canal.")));
    let from = host.sent.len();
    host.click(ids::SCREEN);
    host.click(ids::SHARE_DISMISS);
    assert!(
        !host.ui.video.picker && host.sent.len() == from,
        "dismissed"
    );

    // Hanging up lets go of everything shared in the call.
    host.click(ids::SCREEN);
    host.click(ids::SHARE);
    let from = host.sent.len();
    host.click(ids::LEAVE);
    host.vz_answer(from, 47);
    assert_eq!(host.ui.video.wanted(&host.store), (false, false));
    assert!(!host.ui.video.screen.on && !host.ui.video.camera.on);
    // Outside a call there is nothing to share in.
    assert!(!host.vz_shows(ids::SCREEN));
}

// Screen and camera are Linux's and Windows' (windows-medios).
#[cfg(any(target_os = "linux", windows))]
#[test]
fn the_camera_goes_to_the_call_says_what_it_sends_and_is_taken_back_on_every_stop() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let on = |on| Command::SetCamera { channel: room, on };

    // On: the provider is told, and the host encodes it for the call.
    let from = host.sent.len();
    host.click(ids::CAMERA);
    assert_eq!(host.sent[from..], [on(true)]);
    assert!(crate::video_camera_sending(&host.ui, &host.store));
    assert!(host
        .vz_texts()
        .contains(&"En la llamada · simulada".to_owned()));
    host.ui.video.camera.sent = Some((30, 800));
    assert!(host
        .vz_texts()
        .contains(&"En la llamada · 30 fps · 800 kbit/s · simulada".to_owned()));
    assert!(!host
        .vz_texts()
        .iter()
        .any(|text| text.contains(rune_state::NO_VIDEO_SENDING)));
    // «Apagar la cámara» takes it back.
    let from = host.sent.len();
    host.click(ids::STOP_CAMERA);
    assert_eq!(host.sent[from..], [on(false)]);
    assert!(!crate::video_camera_sending(&host.ui, &host.store));

    // The system ending the camera takes it back too.
    host.click(ids::CAMERA);
    let intents = crate::video_ended(
        &mut host.ui,
        &mut host.store,
        false,
        Some("Se desconectó la cámara.".into()),
        Instant::now(),
    );
    assert_eq!(intents, [Intent::Command(on(false))]);
    assert!(!host.ui.video.camera.on);

    // The provider refusing it turns it off, with why.
    host.click(ids::CAMERA);
    host.event(Event::Failed {
        failure: Failure::Camera { channel: room },
        reason: "Esta ventana no puede enviar la cámara.".into(),
        ambiguous: false,
    });
    assert!(!host.ui.video.camera.on);
    assert!(host
        .ui
        .notices
        .iter()
        .any(|notice| notice.title == "No se pudo usar la cámara"));

    // Without permission to stream there, it is only previewed, and says why.
    let mut quiet = host.store.channel(room).unwrap().clone();
    quiet.permissions = quiet.permissions.without(Permissions::STREAM);
    host.event(Event::ChannelChanged(quiet));
    let from = host.sent.len();
    host.click(ids::CAMERA);
    assert!(host.sent[from..].is_empty(), "{:?}", &host.sent[from..]);
    assert!(host.ui.video.camera.on && !crate::video_camera_sending(&host.ui, &host.store));
    assert!(host
        .vz_texts()
        .iter()
        .any(|text| text.starts_with("Solo vos lo ves. No tenés permiso")));
    // Hanging up lets go of it; the provider ends it with the call.
    let from = host.sent.len();
    host.click(ids::LEAVE);
    assert_eq!(host.sent[from..], [Command::LeaveVoice]);
    assert!(!host.ui.video.camera.on);
}

#[test]
fn the_camera_test_lives_only_while_the_voice_settings_are_open() {
    let mut host = Host::new();
    host.click("me:settings");
    host.click("us:page=voice");
    // Below the microphone's sections: pressed as its action, not scrolled to.
    let press = |host: &mut Host| {
        let action = UiAction::Clicked {
            id: "us:video-test".into(),
        };
        let intents = update(&mut host.ui, &mut host.store, &action, host.now);
        host.run(intents);
    };
    press(&mut host);
    assert!(host.ui.video.testing);
    assert_eq!(host.ui.video.wanted(&host.store), (false, true));
    press(&mut host);
    assert!(!host.ui.video.testing);
    press(&mut host);
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.settings, None);
    assert!(
        !host.ui.video.testing,
        "closing the settings stops the test"
    );
}

#[test]
fn the_rail_says_who_is_in_voice_in_each_server_and_marks_the_own_call() {
    let mut host = Host::new();
    let look = host.ui.look();
    let server = host.ui.server.unwrap();
    let id = format!("server:{server}");
    let streamer = host
        .store
        .voice_by_server()
        .get(&server)
        .and_then(|voice| voice.people.iter().find(|state| state.streaming).copied())
        .map(|state| host.store.display_name(state.user, Some(server)))
        .expect("the simulation shows a screen in the first server");
    // Its stage has a row of its own: the rest are in voice.
    let on_stage = |host: &Host, state: &VoiceState| {
        host.store
            .channel(state.channel)
            .is_some_and(|found| found.kind == rune_model::ChannelKind::Stage)
    };
    let count = host.store.voice_by_server()[&server]
        .people
        .iter()
        .filter(|state| !on_stage(&host, state))
        .count();
    let frame = host.frame();
    let name = &frame.access.iter().find(|node| node.id == id).unwrap().name;
    assert!(
        name.ends_with(&format!(", {count} personas en voz")),
        "{name}"
    );
    let speakers = |frame: &limn::Frame, fill: limn::Color| {
        frame
            .commands
            .iter()
            .filter(|command| {
                matches!(command, limn::DrawCommand::FillRect { color, rect, .. }
                    if *color == fill && rect.width == 18.0 && rect.x < rune_components::rail::WIDTH)
            })
            .count()
    };
    // Three servers have people in voice, but the owned one's are in its
    // idle (AFK) channel: that lights nothing, as in the official client.
    assert_eq!(
        speakers(&frame, look.raised),
        2,
        "two servers have people in voice"
    );
    assert_eq!(speakers(&frame, look.online), 0);
    let hint = host.vz_hint(&id);
    // Faces, not names: the streamer's initial on the screen's row, and
    // the others' after the speaker, five and «+N» past six.
    let initial: String = streamer.chars().next().unwrap().to_uppercase().collect();
    assert!(hint.contains(&initial), "{hint:?}");
    let streaming = host.store.voice_by_server()[&server]
        .people
        .iter()
        .filter(|state| state.streaming && !on_stage(&host, state))
        .count();
    // Its stage: two speakers, and three listen.
    assert!(hint.contains(&"Escuchando".to_owned()), "{hint:?}");
    assert!(hint.contains(&"3".to_owned()), "{hint:?}");
    let others = count - streaming;
    let shown = rune_components::rail::VOICE_SHOWN;
    if others > shown {
        assert!(
            hint.contains(&format!("+{}", others - (shown - 1))),
            "{hint:?}"
        );
    }
    host.vz_join("Sala común");
    let frame = host.frame();
    assert_eq!(speakers(&frame, look.online), 1, "the own call is green");
}

#[cfg(any(target_os = "linux", windows))]
fn rune_ui_live_waiting() -> crate::Live {
    crate::Live::Waiting
}

#[test]
fn someone_streaming_is_watched_on_the_stage_ends_with_why_and_is_let_go() {
    use rune_model::StreamPhase;
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let other = host.vz_other(room);
    let mut state = host
        .store
        .voice_in(room)
        .into_iter()
        .find(|state| state.user == other)
        .cloned()
        .unwrap();
    state.streaming = true;
    host.event(Event::VoiceChanged(state));
    let watch = ids::watch(other.0);
    assert!(host.vz_usable(&watch), "«Ver transmisión» on their tile");
    assert!(!host.vz_shows(&ids::watch(host.vz_me().0)));

    let from = host.sent.len();
    host.click(&watch);
    assert!(matches!(
        host.sent[from..],
        [Command::WatchStream { channel, owner }] if channel == room && owner == other
    ));
    host.event(Event::StreamPhase {
        channel: room,
        owner: other,
        phase: StreamPhase::Live,
    });
    // It takes the stage, says it is simulated, and can be let go of.
    let stop = ids::stop_watching(other.0);
    assert!(host.vz_usable(&stop));
    assert!(!host.vz_shows(&ids::tile(other.0)));
    let shown: Vec<String> = host
        .frame()
        .commands
        .iter()
        .filter_map(|command| match command {
            limn::DrawCommand::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        shown.iter().any(|text| text == "En vivo · simulada"),
        "{shown:?}"
    );
    // Alone, the window full screen; Escape, or its button, goes back.
    let room_row = format!("voice:{room}");
    assert!(host.vz_usable(&ids::fullscreen(other.0)) && host.vz_shows(&room_row));
    host.click(&ids::fullscreen(other.0));
    assert!(crate::video_fullscreen(&host.ui, &host.store));
    assert!(host.vz_usable(ids::FULLSCREEN_EXIT) && !host.vz_shows(&room_row));
    host.key(limn::Key::Escape, false, false);
    assert!(!crate::video_fullscreen(&host.ui, &host.store) && host.vz_shows(&room_row));
    host.click(&ids::fullscreen(other.0));
    host.click(ids::FULLSCREEN_EXIT);
    assert!(!crate::video_fullscreen(&host.ui, &host.store));
    // In a window of its own as well, until that window closes.
    host.click(&ids::window(other.0));
    let windows = crate::video_windows(&host.ui, &host.store);
    assert!(
        matches!(&windows[..], [(owner, title)] if *owner == other && title.starts_with("Pantalla de "))
    );
    assert!(
        !host.vz_shows(&ids::window(other.0)),
        "it is in one already"
    );
    let size = limn::Size::new(960.0, 540.0);
    assert!(crate::video_window(&host.ui, &host.store, other, size).is_some());
    crate::video_window_closed(&mut host.ui, other);
    assert!(crate::video_windows(&host.ui, &host.store).is_empty());
    host.click(&ids::window(other.0));
    let from = host.sent.len();
    host.click(&stop);
    assert!(matches!(host.sent[from..], [Command::StopWatching { .. }]));
    assert!(host.store.watching().is_empty() && host.vz_shows(&watch));
    // Let go of, it leaves its window too.
    assert!(crate::video_windows(&host.ui, &host.store).is_empty());

    // Ended by the service: it stays, with why, until put away.
    host.click(&watch);
    host.event(Event::StreamPhase {
        channel: room,
        owner: other,
        phase: StreamPhase::Ended(Some("La transmisión está llena.".into())),
    });
    assert!(host
        .frame()
        .commands
        .iter()
        .any(|command| matches!(command, limn::DrawCommand::Text { text, .. } if text == "La transmisión está llena.")));
    let from = host.sent.len();
    host.click(&stop);
    assert!(
        host.sent[from..].is_empty(),
        "nothing to tell about an ended stream"
    );

    // Hanging up lets go of a stream watched.
    host.click(&watch);
    let from = host.sent.len();
    host.click(ids::LEAVE);
    host.vz_answer(from, 47);
    assert!(host.store.watching().is_empty());
    assert!(host.ui.video.streams.is_empty());
}

#[test]
fn a_stream_s_picture_shows_under_its_owner_in_the_column_asked_for_now_and_then() {
    use std::time::Duration;
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    let other = host.vz_other(room);
    let mut state = host
        .store
        .voice_in(room)
        .into_iter()
        .find(|state| state.user == other)
        .cloned()
        .unwrap();
    state.streaming = true;
    host.event(Event::VoiceChanged(state));
    let now = Instant::now();
    let server = host.store.channel(room).unwrap().server.unwrap();
    // Theirs among those streaming in the server's voice channels.
    let asked = crate::video_previews(&mut host.ui, &host.store, now);
    assert!(asked.iter().any(|intent| matches!(
        intent,
        Intent::Command(Command::LoadStreamPreview { server: on, channel, owner })
            if (*on, *channel, *owner) == (server, room, other)
    )));
    // Not again until a minute went by.
    let later = |seconds| now + Duration::from_secs(seconds);
    assert!(crate::video_previews(&mut host.ui, &host.store, later(30)).is_empty());
    assert_eq!(
        crate::video_previews(&mut host.ui, &host.store, later(60)).len(),
        asked.len()
    );
    // The simulation has none to show: a made-up one, said so; a click
    // watches the stream, which then leaves the column.
    let preview = ids::preview(other.0);
    assert!(!host.vz_shows(&preview));
    host.event(Event::StreamPreview {
        channel: room,
        owner: other,
        image: None,
    });
    assert!(host.vz_usable(&preview));
    assert!(host
        .vz_texts()
        .contains(&"Vista previa simulada".to_owned()));
    let from = host.sent.len();
    host.click(&preview);
    assert!(matches!(
        host.sent[from..],
        [Command::WatchStream { owner, .. }] if owner == other
    ));
    assert!(!host.vz_shows(&preview));
}

#[test]
fn several_streams_share_the_stage_up_to_their_bound_and_the_oldest_goes() {
    use rune_model::StreamPhase;
    use rune_provider::video::MAX_STREAMS;
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    host.vz_fill(room, MAX_STREAMS + 2);
    let me = host.vz_me();
    let states: Vec<_> = host
        .store
        .voice_in(room)
        .into_iter()
        .filter(|state| state.user != me)
        .take(MAX_STREAMS + 1)
        .cloned()
        .collect();
    let streamers: Vec<UserId> = states.iter().map(|state| state.user).collect();
    for mut state in states {
        state.streaming = true;
        host.event(Event::VoiceChanged(state));
    }
    // Each one watched joins the stage; those not watched stay there to
    // be watched too.
    for (at, owner) in streamers.iter().enumerate() {
        let from = host.sent.len();
        host.click(&ids::watch(owner.0));
        host.event(Event::StreamPhase {
            channel: room,
            owner: *owner,
            phase: StreamPhase::Live,
        });
        if at < MAX_STREAMS {
            assert!(matches!(
                host.sent[from..],
                [Command::WatchStream { owner: asked, .. }] if asked == *owner
            ));
        } else {
            // One more lets go of the one watched longest.
            assert_eq!(
                host.sent[from..],
                [
                    Command::StopWatching {
                        channel: room,
                        owner: streamers[0]
                    },
                    Command::WatchStream {
                        channel: room,
                        owner: *owner
                    }
                ]
            );
        }
    }
    let watched: Vec<UserId> = host.store.watching().iter().map(|w| w.owner).collect();
    assert_eq!(watched, streamers[1..]);
    for owner in &streamers[1..] {
        assert!(host.vz_usable(&ids::stop_watching(owner.0)));
        assert!(!host.vz_shows(&ids::watch(owner.0)));
    }
    assert!(
        host.vz_usable(&ids::watch(streamers[0].0)),
        "to watch again"
    );
    // The sound of a stream watched has a volume of its own, in its
    // owner's menu, apart from their voice's.
    let owner = streamers[2];
    host.ui.voice.menu = Some(crate::voice::Menu {
        user: owner,
        channel: room,
        at: Some((300.0, 200.0)),
        row: true,
    });
    let sound = owner.0 | rune_media::mixer::STREAM_SOUND;
    assert!(host.vz_shows(&ids::volume(owner.0)) && host.vz_shows(&ids::volume(sound)));
    let intents = crate::update(
        &mut host.ui,
        &mut host.store,
        &limn::UiAction::SliderChanged {
            id: ids::volume(sound),
            fraction: 0.25,
            committed: true,
        },
        Instant::now(),
    );
    assert_eq!(intents.len(), 1, "kept");
    assert_eq!(host.ui.prefs.voice_volume(sound), (0.5, false));
    assert_eq!(host.ui.prefs.voice_volume(owner.0), (1.0, false));
    host.ui.voice.menu = None;
    // Letting go of one leaves the others.
    host.click(&ids::stop_watching(streamers[1].0));
    assert_eq!(host.store.watching().len(), MAX_STREAMS - 1);
}

#[test]
fn the_cameras_on_the_stage_are_asked_for_at_most_four_and_show_on_their_tiles() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    host.vz_fill(room, 7);
    let me = host.vz_me();
    for mut state in host
        .store
        .voice_in(room)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>()
    {
        state.camera = true;
        host.event(Event::VoiceChanged(state));
    }
    let wanted = crate::video::cameras(&host.ui, &host.store);
    assert_eq!(wanted.len(), rune_provider::video::MAX_CAMERAS);
    assert!(!wanted.contains(&me), "not the account's own");
    // What the host shows takes the place of their mark.
    let surface = ids::camera_surface(wanted[0].0);
    let drawn = |host: &mut Host| {
        host.frame().commands.iter().any(
            |command| matches!(command, limn::DrawCommand::Surface { id, .. } if *id == surface),
        )
    };
    assert!(!drawn(&mut host));
    host.ui.video.watched = vec![(wanted[0], crate::Live::Showing(640, 360))];
    assert!(drawn(&mut host));
    // A stream on the stage, or no stage, asks for none.
    host.store.begin_watch(room, wanted[1]);
    assert!(crate::video::cameras(&host.ui, &host.store).is_empty());
    host.store.stop_watch(wanted[1]);
    host.ui.voice.stage = None;
    assert!(crate::video::cameras(&host.ui, &host.store).is_empty());
    // Hanging up lets go of them.
    host.click(&format!("voice:{room}"));
    let from = host.sent.len();
    host.click(ids::LEAVE);
    host.vz_answer(from, 47);
    assert!(host.ui.video.watched.is_empty());
}

#[test]
fn the_bars_arrows_open_the_quick_panel_with_the_calls_level_live() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    // The stage, where the bar is.
    host.click(&format!("voice:{room}"));
    assert_eq!(host.ui.voice.stage, Some(room));
    let opened = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: ids::INPUT_OPTIONS.into(),
        },
        host.now,
    );
    // The call holds the microphone: its level shows at once.
    assert!(opened.contains(&Intent::StartMeter(None)), "{opened:?}");
    assert!(host
        .ui
        .profiles
        .devices
        .is_some_and(|it| it.bar && !it.output));
    assert!(host.vz_texts().iter().any(|it| it == "Ajustes de entrada"));
    // A click outside closes it, and the meter it started.
    host.click(ids::OUTPUT_OPTIONS);
    assert!(host.ui.profiles.devices.is_none() && !host.ui.audio.testing);
    host.click(ids::OUTPUT_OPTIONS);
    assert!(host
        .ui
        .profiles
        .devices
        .is_some_and(|it| it.bar && it.output));
    assert!(host.vz_texts().iter().any(|it| it == "Ajustes de salida"));
    host.click(ids::OUTPUT_OPTIONS);
    assert!(host.ui.profiles.devices.is_none());
    // Muted, the microphone stays closed until asked.
    host.ui.muted = true;
    host.click(ids::INPUT_OPTIONS);
    assert!(host.ui.profiles.devices.is_some() && !host.ui.audio.testing);
}

// Voice protocol (voz-protocolo).
#[test]
fn the_column_tells_the_server_it_shows_and_how_long_each_voice_room_is_on() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    host.frame();
    assert_eq!(host.servers_shown.last(), Some(&Some(server)));
    let told = host.servers_shown.len();
    host.frame();
    assert_eq!(host.servers_shown.len(), told, "once a server");
    // A new session watches nothing yet: told again.
    let snapshot = host.provider.snapshot();
    host.event(Event::Ready(Box::new(snapshot)));
    host.frame();
    assert_eq!(host.servers_shown[told..], [Some(server)]);
    // A room someone is in, on since 65 and a half minutes ago.
    let room = host.channel_named("Sala común");
    let wall = host.store.wall();
    host.event(Event::VoiceChanged(VoiceState {
        user: UserId(77),
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
    host.event(Event::VoiceStartTimes {
        server,
        times: vec![(room, Some(Timestamp(wall.0 - 65 * 60_000 - 30_000)))],
        whole: false,
    });
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text == "1 h 05 min"),
        "{painted:?}"
    );
    // Its next minute comes in half of one: the host paints again then.
    assert_eq!(
        crate::voice_since_tick(&host.store, wall),
        Some(Duration::from_secs(30))
    );
}

// Screen and camera are Linux's and Windows' (windows-medios).
#[cfg(any(target_os = "linux", windows))]
// Calls and the voice panel (llamadas-y-panel-de-voz).
#[test]
fn the_bars_camera_arrow_tries_the_camera_without_sending_it() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    host.click(ids::CAMERA_OPTIONS);
    assert!(host
        .ui
        .profiles
        .devices
        .is_some_and(|it| it.bar && it.camera && !it.output));
    let shown = host.vz_texts();
    assert!(
        shown.iter().any(|it| it == "Ajustes de cámara"),
        "{shown:?}"
    );
    assert!(!shown.iter().any(|it| it == "Ajustes de entrada"));
    // The camera chosen before shows as itself until the system lists them.
    host.ui.video.device = Some("/dev/video2".into());
    let shown = host.vz_texts();
    assert!(shown.iter().any(|it| it == "/dev/video2"), "{shown:?}");
    assert!(!shown.iter().any(|it| it == "Dispositivo no disponible"));
    // Trying it holds the camera here; nothing goes to the call.
    let from = host.sent.len();
    host.click("us:video-test");
    assert!(host.ui.video.testing);
    assert_eq!(host.ui.video.wanted(&host.store), (false, true));
    assert!(!crate::video::camera_sending(&host.ui, &host.store));
    assert!(!host.sent[from..]
        .iter()
        .any(|command| matches!(command, Command::SetCamera { .. })));
    // Its picture shows once the host has a frame.
    let drawn = |host: &mut Host| {
        host.frame().commands.iter().any(|command| {
            // Mirrored, as a webcam shows the person (limn-teclado-y-superficies).
            matches!(command, limn::DrawCommand::Surface { id, mirrored: true, .. } if id == ids::CAMERA_SURFACE)
        })
    };
    assert!(!drawn(&mut host));
    host.ui.video.camera.live = crate::Live::Showing(640, 480);
    assert!(drawn(&mut host));
    assert!(host.vz_texts().iter().any(|it| it == "Detener video"));
    // Its arrow closes it, and the test with it.
    host.click(ids::CAMERA_OPTIONS);
    assert!(host.ui.profiles.devices.is_none() && !host.ui.video.testing);
    // The call ending while it tests closes it too.
    host.click(ids::CAMERA_OPTIONS);
    host.click("us:video-test");
    assert!(host.ui.video.testing);
    host.event(Event::CallEnded {
        channel: room,
        reason: None,
    });
    assert!(host.ui.profiles.devices.is_none() && !host.ui.video.testing);
}

#[test]
fn the_bars_audio_panel_goes_with_the_call_and_stops_its_level() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    host.click(ids::INPUT_OPTIONS);
    assert!(host.ui.audio.testing, "the call's level shows");
    host.event(Event::CallEnded {
        channel: room,
        reason: None,
    });
    assert!(host.ui.profiles.devices.is_none());
    assert!(!host.ui.audio.testing, "the microphone is not left open");
}

#[test]
fn a_tile_under_the_pointer_plays_the_frame_over_their_face() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let other = host.vz_other(room);
    // Someone in the call wears a frame over their face (simulated).
    let decoration = rune_model::RemoteImage {
        url: "https://simulacion.invalid/decoraciones/sala.png".into(),
        kind: rune_model::ImageKind::Artwork,
    };
    let user = host.store.user(other).unwrap().clone();
    host.event(Event::UserChanged(rune_model::User {
        identity: rune_model::Identity {
            decoration: Some(decoration.clone()),
            ..user.identity.clone()
        },
        ..user
    }));
    assert!(!playable_images(&host.ui, &host.store, &host.tree, false).contains(&decoration));
    let tile = host.vz_rect(&ids::tile(other.0));
    host.send(UiEvent::PointerMove(limn::Point::new(
        tile.x + tile.width / 2.0,
        tile.y + tile.height / 2.0,
    )));
    host.frame();
    assert_eq!(host.ui.profiles.hovered, Some(other));
    assert!(playable_images(&host.ui, &host.store, &host.tree, false).contains(&decoration));
    assert!(
        playable_images(&host.ui, &host.store, &host.tree, true).is_empty(),
        "nothing moves with reduced motion"
    );
}
