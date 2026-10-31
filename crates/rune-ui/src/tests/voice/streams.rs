//! Streams (transmisiones): focusing one watched and coming back to the
//! grid, full screen with a double click, a stream's own menu and its
//! volume apart from the person's, what its tile and the bar offer as the
//! official client's, the pictures of streams asked for as the official
//! client asks them, and the account's own picture going up.

use std::time::Duration;

use rune_model::StreamPhase;

use super::*;

impl Host {
    /// `count` people in `room` (the account among them), the first
    /// `streaming` of the others streaming: who streams.
    fn tr_streamers(&mut self, room: ChannelId, count: usize, streaming: usize) -> Vec<UserId> {
        let already = self.store.voice_in(room).len();
        self.vz_fill(room, count.max(already));
        let me = self.vz_me();
        let states: Vec<_> = self
            .store
            .voice_in(room)
            .into_iter()
            .filter(|state| state.user != me)
            .take(streaming)
            .cloned()
            .collect();
        let owners = states.iter().map(|state| state.user).collect();
        for mut state in states {
            state.streaming = true;
            self.event(Event::VoiceChanged(state));
        }
        owners
    }

    /// Watch `owner`'s stream in `room`, live.
    fn tr_watch(&mut self, room: ChannelId, owner: UserId) {
        self.click(&ids::watch(owner.0));
        self.event(Event::StreamPhase {
            channel: room,
            owner,
            phase: StreamPhase::Live,
        });
        // The host shows its frames.
        self.ui.video.streams.retain(|(shown, _)| *shown != owner);
        self.ui
            .video
            .streams
            .push((owner, crate::Live::Showing(1280, 720, None)));
    }

    /// Where `owner`'s stream is drawn.
    fn tr_surface(&mut self, owner: UserId) -> limn::Rect {
        let surface = ids::stream_surface(owner.0);
        self.frame()
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::Surface { id, rect, .. } if *id == surface => Some(*rect),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{surface} is not drawn"))
    }

    /// A double click at `at`, as the window reports one: a click, then a
    /// press that counts two.
    fn tr_double_click(&mut self, at: Point) {
        self.send(UiEvent::PointerDown(at));
        self.send(UiEvent::PointerUp(at));
        self.send(UiEvent::PointerDownEditor {
            position: at,
            shift: false,
            alt: false,
            ctrl: false,
            click_count: 2,
        });
        self.send(UiEvent::PointerUp(at));
    }
}

fn middle(rect: limn::Rect) -> Point {
    Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

#[test]
fn a_stream_watched_is_focused_with_a_click_and_the_grid_comes_back() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let owners = host.tr_streamers(room, 4, 2);
    let (first, second) = (owners[0], owners[1]);
    host.tr_watch(room, first);
    host.tr_watch(room, second);
    let side = host.tr_surface(first);
    assert_eq!(side.width, host.tr_surface(second).width, "side by side");

    // A click on one: it takes the stage, the other goes small under it.
    host.click(&ids::stream_tile(first.0));
    assert_eq!(host.ui.video.spotlight, Some(first));
    let (big, small) = (host.tr_surface(first), host.tr_surface(second));
    assert!(big.width > side.width && small.width < side.width);
    assert!(small.y > big.y + big.height);
    // The bar lets it go in place of hanging up, as the official client's.
    assert!(host.vz_usable(&ids::stop_watching(first.0)), "the way out");
    assert!(!host.vz_shows(&ids::stop_watching(second.0)));
    // A click on the one in the strip focuses it instead.
    host.click(&ids::stream_tile(second.0));
    assert_eq!(host.ui.video.spotlight, Some(second));
    // A click on it again, Escape or «Volver a la grilla» in its menu: the grid.
    host.click(&ids::stream_tile(second.0));
    assert_eq!(host.ui.video.spotlight, None);
    assert!(
        !host.vz_shows(&ids::stop_watching(second.0)),
        "hanging up is back"
    );
    host.click(&ids::stream_tile(first.0));
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.video.spotlight, None);
    assert_eq!(host.tr_surface(first).width, side.width);
    host.click(&ids::stream_tile(first.0));
    host.vz_right_click(&ids::stream_tile(first.0));
    host.click(&ids::spotlight(first.0));
    assert_eq!(host.ui.video.spotlight, None);
    // Letting go of the one focused brings the grid back too.
    host.click(&ids::stream_tile(first.0));
    host.click(&ids::stop_watching(first.0));
    assert_eq!(host.ui.video.spotlight, None);
    assert!(host
        .store
        .watching()
        .iter()
        .all(|shown| shown.owner != first));
}

#[test]
fn a_double_click_shows_a_stream_full_screen_and_another_brings_the_grid_back() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let owners = host.tr_streamers(room, 4, 2);
    let (first, second) = (owners[0], owners[1]);
    host.tr_watch(room, first);
    host.tr_watch(room, second);
    // From the grid: it is focused and shown alone, the window full screen.
    let at = middle(host.vz_rect(&ids::stream_tile(second.0)));
    host.tr_double_click(at);
    assert_eq!(host.ui.video.spotlight, Some(second));
    assert_eq!(host.ui.video.focus, Some(second));
    assert!(crate::video_fullscreen(&host.ui, &host.store));
    // Another there, as the official client: the stage and its grid.
    let alone = host.tr_surface(second);
    host.tr_double_click(middle(alone));
    assert_eq!((host.ui.video.focus, host.ui.video.spotlight), (None, None));
    assert!(!crate::video_fullscreen(&host.ui, &host.store));
    // From the stream focused: its first click lets it go and moves the
    // tiles, yet the press is still the focused one's.
    host.click(&ids::stream_tile(first.0));
    let at = middle(host.tr_surface(first));
    host.tr_double_click(at);
    assert_eq!(host.ui.video.focus, Some(first));
    assert_eq!(host.ui.video.spotlight, Some(first));
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.video.focus, None);
    assert_eq!(
        host.ui.video.spotlight,
        Some(first),
        "Escape leaves it focused"
    );
    // A double click elsewhere is the tree's: two clicks of the bar's.
    host.key(Key::Escape, false, false);
    let at = middle(host.vz_rect(ids::MUTE));
    host.tr_double_click(at);
    assert_eq!(host.ui.video.focus, None);
}

#[test]
fn a_streams_tile_offers_its_sound_and_menu_and_the_bar_its_window_and_full_screen() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let owner = host.tr_streamers(room, 3, 1)[0];
    host.tr_watch(room, owner);
    let sound = owner.0 | rune_media::mixer::STREAM_SOUND;
    // Its sound, from its tile: the stream's, apart from the voice's.
    host.click(&ids::local_mute(sound));
    assert_eq!(host.ui.prefs.voice_volume(sound), (1.0, true));
    assert_eq!(host.ui.prefs.voice_volume(owner.0), (1.0, false));
    // Its «⋯»: its menu, there.
    host.click(&ids::stream_more(owner.0));
    assert_eq!(host.ui.video.menu, Some((owner, None)));
    for id in [
        ids::volume(sound),
        ids::fullscreen(owner.0),
        ids::stop_watching(owner.0),
    ] {
        assert!(host.vz_shows(&id), "{id}");
    }
    host.click(&ids::window(owner.0));
    assert_eq!(host.ui.video.windows, [owner]);
    assert!(host.ui.video.menu.is_none(), "an item closes it");
    // Focused, as the only one there: the bar's end has full screen, and
    // no second window.
    assert_eq!(host.ui.video.spotlight, Some(owner));
    assert!(!host.vz_shows(&ids::window(owner.0)), "already in one");
    host.ui.video.windows.clear();
    let (window, full) = (
        host.vz_rect(&ids::window(owner.0)),
        host.vz_rect(&ids::fullscreen(owner.0)),
    );
    let out = host.vz_rect(&ids::stop_watching(owner.0));
    assert!(out.x + out.width < window.x && window.x < full.x);
    host.click(&ids::fullscreen(owner.0));
    assert_eq!(host.ui.video.focus, Some(owner));
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.video.focus, None);
    // «Dejar de ver» in place of hanging up.
    let from = host.sent.len();
    host.click(&ids::stop_watching(owner.0));
    assert!(matches!(
        host.sent[from..],
        [Command::StopWatching { owner: stopped, .. }] if stopped == owner
    ));
}

#[test]
fn a_streams_menu_has_its_own_volume_and_its_ways_to_show_it() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let owner = host.tr_streamers(room, 3, 1)[0];
    host.tr_watch(room, owner);
    let sound = owner.0 | rune_media::mixer::STREAM_SOUND;

    // A secondary click on the stream opens its own menu, not the person's.
    host.vz_right_click(&ids::stream_tile(owner.0));
    assert_eq!(host.ui.video.menu.map(|(shown, _)| shown), Some(owner));
    assert!(host.ui.voice.menu.is_none());
    for id in [
        ids::volume(sound),
        ids::local_mute(sound),
        ids::spotlight(owner.0),
        ids::fullscreen(owner.0),
        ids::window(owner.0),
        ids::stop_watching(owner.0),
    ] {
        assert!(host.vz_shows(&id), "{id}");
    }
    assert!(
        !host.vz_shows(&ids::volume(owner.0)),
        "the person's is theirs"
    );
    // The note wraps, so its first words are what is on screen.
    assert!(host
        .vz_texts()
        .iter()
        .any(|text| text.starts_with("Simulado")));
    // Its volume and silence are the stream's sound, apart from the voice's.
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
    assert_eq!(host.ui.prefs.voice_volume(sound), (0.14, false));
    host.click(&ids::local_mute(sound));
    assert_eq!(host.ui.prefs.voice_volume(sound), (0.14, true));
    assert_eq!(host.ui.prefs.voice_volume(owner.0), (1.0, false));
    assert!(host.ui.video.menu.is_none(), "an item closes it");
    // The only one there, it was focused as it was watched, as in the
    // official client. Back to the grid, focusing it, full screen and a
    // window of its own, from there too.
    assert_eq!(host.ui.video.spotlight, Some(owner));
    host.vz_right_click(&ids::stream_tile(owner.0));
    assert!(host.vz_texts().contains(&"Volver a la grilla".to_owned()));
    host.click(&ids::spotlight(owner.0));
    assert_eq!(host.ui.video.spotlight, None);
    host.vz_right_click(&ids::stream_tile(owner.0));
    host.click(&ids::spotlight(owner.0));
    assert_eq!(host.ui.video.spotlight, Some(owner));
    host.vz_right_click(&ids::stream_tile(owner.0));
    host.click(&ids::window(owner.0));
    assert_eq!(host.ui.video.windows, [owner]);
    host.vz_right_click(&ids::stream_tile(owner.0));
    assert!(!host.vz_shows(&ids::window(owner.0)), "already in one");
    host.click(&ids::fullscreen(owner.0));
    assert_eq!(host.ui.video.focus, Some(owner));
    host.key(Key::Escape, false, false);
    assert_eq!(host.ui.video.focus, None);
    // «Dejar de ver», and the menu goes with the stream.
    host.vz_right_click(&ids::stream_tile(owner.0));
    let from = host.sent.len();
    host.click(&ids::stop_watching(owner.0));
    assert!(matches!(
        host.sent[from..],
        [Command::StopWatching { owner: stopped, .. }] if stopped == owner
    ));
    assert!(host.ui.video.menu.is_none() && host.store.watching().is_empty());
}

#[test]
fn a_streams_picture_is_asked_for_as_the_official_client_does_and_shows_on_its_tile() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let owner = host.tr_streamers(room, 2, 1)[0];
    let server = host.store.channel(room).unwrap().server.unwrap();
    let start = Instant::now();
    let at = |seconds: u64| start + Duration::from_secs(seconds);
    let asked = |host: &mut Host, when| {
        crate::video_previews(&mut host.ui, &host.store, when)
            .iter()
            .any(|intent| {
                matches!(
                    intent,
                    Intent::Command(Command::LoadStreamPreview { server: on, channel, owner: of })
                        if (*on, *channel, *of) == (Some(server), room, owner)
                )
            })
    };
    let answer = |host: &mut Host, image, retry_after| {
        host.event(Event::StreamPreview {
            channel: room,
            owner,
            image,
            retry_after,
        })
    };
    assert!(asked(&mut host, at(0)));
    // Unanswered: again after ten seconds, as a failure.
    assert!(!asked(&mut host, at(9)));
    assert!(asked(&mut host, at(10)));
    // A picture: good for two minutes.
    let image = rune_model::RemoteImage {
        url: "https://cdn.discordapp.com/streams/1.png".into(),
        kind: rune_model::ImageKind::Banner,
    };
    answer(&mut host, Some(image), None);
    assert!(!asked(&mut host, at(129)));
    assert!(asked(&mut host, at(130)));
    // A 429: as long as the service said.
    answer(&mut host, None, Some(Duration::from_secs(45)));
    assert!(!asked(&mut host, at(174)));
    assert!(asked(&mut host, at(175)));
    // None, again and again: ten seconds more each time.
    answer(&mut host, None, None);
    assert!(!asked(&mut host, at(194)), "the second in a row: 20 s");
    assert!(asked(&mut host, at(195)));
    answer(&mut host, None, None);
    assert!(!asked(&mut host, at(224)), "the third: 30 s");
    assert!(asked(&mut host, at(225)));

    // The simulation's has none: its tile on the stage says so, under
    // «EN VIVO» and over the way to watch it.
    let texts = host.vz_texts();
    for expected in ["EN VIVO", "Vista previa simulada", "Ver transmisión"] {
        assert!(
            texts.contains(&expected.to_owned()),
            "{expected} in {texts:?}"
        );
    }
    assert!(host.vz_usable(&ids::watch(owner.0)));
}

#[test]
fn the_accounts_own_streams_picture_goes_up_as_the_official_client_sends_it() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    let me = host.vz_me();
    let server = host.store.channel(room).unwrap().server;
    let start = Instant::now();
    let at = |millis: u64| start + Duration::from_millis(millis);
    // Nothing while it does not share.
    assert_eq!(
        crate::video_own_preview(&mut host.ui, &host.store, at(0)),
        None
    );
    host.store.begin_share(room);
    host.event(Event::StreamPhase {
        channel: room,
        owner: me,
        phase: StreamPhase::Live,
    });
    // Nor before the screen shows.
    assert_eq!(
        crate::video_own_preview(&mut host.ui, &host.store, at(0)),
        None
    );
    host.ui.video.screen.on = true;
    host.ui.video.screen.live = crate::Live::Showing(1920, 1080, None);
    let due =
        |host: &mut Host, millis| crate::video_own_preview(&mut host.ui, &host.store, at(millis));
    // Half a second after it shows, then every five minutes.
    assert_eq!(due(&mut host, 0), None);
    assert_eq!(due(&mut host, 499), None);
    assert_eq!(due(&mut host, 500), Some((server, room, me)));
    assert_eq!(due(&mut host, 1_000), None);
    assert_eq!(due(&mut host, 300_499), None);
    assert_eq!(due(&mut host, 300_500), Some((server, room, me)));
    // A picture that could not be taken: a minute later.
    crate::video_own_preview_failed(&mut host.ui, at(301_000));
    assert_eq!(due(&mut host, 360_999), None);
    assert_eq!(due(&mut host, 361_000), Some((server, room, me)));
    // Paused: none, and it looks again each minute.
    host.event(Event::StreamPhase {
        channel: room,
        owner: me,
        phase: StreamPhase::Paused,
    });
    assert_eq!(due(&mut host, 661_000), None);
    host.event(Event::StreamPhase {
        channel: room,
        owner: me,
        phase: StreamPhase::Live,
    });
    assert_eq!(due(&mut host, 720_999), None);
    assert_eq!(due(&mut host, 721_000), Some((server, room, me)));
    // Done sharing: it starts over with the next stream.
    host.store.stop_share();
    assert_eq!(due(&mut host, 721_500), None);
    assert_eq!(host.ui.video.own_preview, None);
}

#[test]
fn a_secondary_click_on_a_stream_or_its_streamer_offers_to_stop_watching() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let me = host.vz_me();
    let owners = host.tr_streamers(room, 4, 2);
    let (first, second) = (owners[0], owners[1]);
    let count = |host: &mut Host, id: &str| {
        host.frame()
            .regions
            .iter()
            .filter(|region| region.id == id)
            .count()
    };
    host.tr_watch(room, first);
    host.click(&ids::stream_tile(first.0));
    assert_eq!(host.ui.video.spotlight, Some(first));
    let stop = ids::stop_watching(first.0);
    assert_eq!(count(&mut host, &stop), 1, "the bar's");

    // Anywhere on the stream focused: its menu, with «Dejar de ver».
    let at = middle(host.tr_surface(first));
    host.send(UiEvent::ContextMenu { position: at });
    assert_eq!(host.ui.video.menu.map(|(shown, _)| shown), Some(first));
    assert_eq!(count(&mut host, &stop), 2, "the bar's and the menu's");
    host.click(ids::MENU_DISMISS);

    // The streamer stays on the stage, in the strip, as in the official
    // client: their own menu lets go of their stream too.
    host.vz_right_click(&ids::tile(first.0));
    assert_eq!(host.ui.voice.menu.map(|menu| menu.user), Some(first));
    assert_eq!(count(&mut host, &stop), 2, "from the streamer's tile");
    // Away from the menu, which may cover the screen's middle.
    let away = Point::new(1.0, 799.0);
    host.send(UiEvent::PointerDown(away));
    host.send(UiEvent::PointerUp(away));
    // Someone streaming and not watched: «Ver transmisión» in theirs.
    host.vz_right_click(&ids::tile(second.0));
    assert_eq!(host.ui.voice.menu.map(|menu| menu.user), Some(second));
    assert_eq!(count(&mut host, &ids::watch(second.0)), 1);
    host.click(&ids::watch(second.0));
    assert!(host
        .store
        .watching()
        .iter()
        .any(|shown| shown.owner == second));
    host.event(Event::StreamPhase {
        channel: room,
        owner: second,
        phase: StreamPhase::Live,
    });
    host.ui.video.streams.retain(|(shown, _)| *shown != second);
    host.ui
        .video
        .streams
        .push((second, crate::Live::Showing(1280, 720, None)));
    assert_eq!(host.ui.video.spotlight, Some(first), "still focused");

    // The thumbnail in the strip, under the one focused.
    let at = middle(host.tr_surface(second));
    host.send(UiEvent::ContextMenu { position: at });
    assert_eq!(host.ui.video.menu.map(|(shown, _)| shown), Some(second));
    assert_eq!(count(&mut host, &ids::stop_watching(second.0)), 1);
    host.click(ids::MENU_DISMISS);

    // Let go of from the streamer's tile: the grid comes back.
    host.vz_right_click(&ids::tile(first.0));
    let from = host.sent.len();
    let item = host
        .frame()
        .regions
        .iter()
        .rev()
        .find(|region| region.id == stop)
        .map(|region| middle(region.rect))
        .expect("in the menu");
    host.send(UiEvent::PointerDown(item));
    host.send(UiEvent::PointerUp(item));
    assert!(matches!(
        host.sent[from..],
        [Command::StopWatching { owner: stopped, .. }] if stopped == first
    ));
    assert_eq!(host.ui.video.spotlight, None);

    // The account's own stream: «Dejar de compartir».
    host.store.begin_share(room);
    host.ui.video.screen.on = true;
    host.event(Event::StreamPhase {
        channel: room,
        owner: me,
        phase: StreamPhase::Live,
    });
    host.vz_right_click(&ids::tile(me.0));
    assert_eq!(host.ui.voice.menu.map(|menu| menu.user), Some(me));
    let from = host.sent.len();
    let item = host
        .frame()
        .regions
        .iter()
        .rev()
        .find(|region| region.id == ids::STOP_SCREEN)
        .map(|region| middle(region.rect))
        .expect("in the menu");
    host.send(UiEvent::PointerDown(item));
    host.send(UiEvent::PointerUp(item));
    assert_eq!(host.sent[from..], [Command::StopStream { channel: room }]);
}

impl Host {
    /// Watch the only stream in a call of three, focused on the stage, and
    /// rest the pointer on its picture: whose it is.
    fn tr_focused(&mut self) -> UserId {
        let room = self.vz_join("Sala común");
        self.click(&format!("voice:{room}"));
        let owner = self.tr_streamers(room, 3, 1)[0];
        self.tr_watch(room, owner);
        if self.ui.video.spotlight != Some(owner) {
            self.click(&ids::stream_tile(owner.0));
        }
        assert_eq!(self.ui.video.spotlight, Some(owner));
        let at = middle(self.tr_surface(owner));
        self.send(UiEvent::PointerMove(at));
        owner
    }

    /// The pointer stays where it is for `millis`, after the frame that
    /// follows what was last done, as the window paints one.
    fn tr_rest(&mut self, millis: u64) {
        self.frame();
        self.now += Duration::from_millis(millis);
        self.frame();
    }

    /// What is over the stream focused, and the bar, show.
    fn tr_controls_show(&mut self, owner: UserId) -> bool {
        self.vz_usable(&ids::fullscreen(owner.0)) && self.vz_usable(ids::MUTE)
    }
}

// Resting on a stream (transmisiones): as the official client's stage, what
// is over the stream focused and the bar fade after 3 s without the pointer
// moving, and the pointer hides; any move brings them back at once.
#[test]
fn a_stream_focused_hides_its_controls_and_the_pointer_once_the_pointer_rests() {
    let mut host = Host::new();
    let owner = host.tr_focused();
    let at = middle(host.tr_surface(owner));
    assert!(host.tr_controls_show(owner));
    assert_eq!(
        crate::stream_idle_tick(&host.ui, &host.store, host.now),
        Some(crate::video::IDLE_AFTER),
        "the host is asked to paint again when it rests"
    );
    host.tr_rest(2_900);
    assert!(host.tr_controls_show(owner), "not yet");
    host.tr_rest(100);
    assert!(!host.tr_controls_show(owner));
    assert!(!host.vz_shows(&ids::stop_watching(owner.0)));
    assert!(
        host.vz_usable(&ids::stream_tile(owner.0)),
        "the picture is still there to click"
    );
    assert_eq!(host.tree.cursor(), Some(limn::Cursor::Hidden));
    assert_eq!(
        crate::stream_idle_tick(&host.ui, &host.store, host.now),
        None
    );
    // A move, however small: all of it back, and the pointer.
    host.send(UiEvent::PointerMove(Point::new(at.x + 1.0, at.y)));
    assert!(host.tr_controls_show(owner));
    assert_eq!(host.tree.cursor(), None);
    // A key too, unlike the official client.
    host.tr_rest(3_000);
    assert!(!host.tr_controls_show(owner));
    host.key(Key::Tab, false, false);
    assert!(host.tr_controls_show(owner));
}

#[test]
fn a_stream_never_hides_its_controls_on_the_bar_with_a_menu_open_or_in_the_grid() {
    let mut host = Host::new();
    let owner = host.tr_focused();
    // The pointer resting on the bar keeps it.
    let mute = host.vz_rect(ids::MUTE);
    host.send(UiEvent::PointerMove(middle(mute)));
    host.tr_rest(10_000);
    assert!(host.tr_controls_show(owner));
    assert_eq!(
        crate::stream_idle_tick(&host.ui, &host.store, host.now),
        None
    );
    // On the stream's own controls too.
    let full = host.vz_rect(&ids::fullscreen(owner.0));
    host.send(UiEvent::PointerMove(middle(full)));
    host.tr_rest(10_000);
    assert!(host.tr_controls_show(owner));
    // Its menu open, the pointer back on the picture.
    host.click(&ids::stream_more(owner.0));
    assert!(host.ui.video.menu.is_some());
    let at = middle(host.tr_surface(owner));
    host.send(UiEvent::PointerMove(at));
    host.tr_rest(10_000);
    assert!(host.tr_controls_show(owner) && host.ui.video.menu.is_some());
    host.click(&ids::stream_more(owner.0));
    assert!(host.ui.video.menu.is_none());
    host.send(UiEvent::PointerMove(at));
    host.tr_rest(3_000);
    assert!(!host.tr_controls_show(owner), "closed, it rests again");
    host.send(UiEvent::PointerMove(at));
    // A control focused from the keyboard keeps them.
    host.key(Key::Tab, false, false);
    host.tr_rest(10_000);
    assert!(host.tr_controls_show(owner), "the keyboard is at work");
    // Back to the grid: nothing hides there, however long it rests.
    host.click(&ids::stream_tile(owner.0));
    assert_eq!(host.ui.video.spotlight, None);
    let at = middle(host.tr_surface(owner));
    host.send(UiEvent::PointerMove(at));
    host.tr_rest(10_000);
    assert!(host.vz_usable(ids::MUTE));
    assert_eq!(host.tree.cursor(), None);
    assert_eq!(
        crate::stream_idle_tick(&host.ui, &host.store, host.now),
        None
    );
}

#[test]
fn full_screen_and_a_window_of_its_own_rest_as_the_stage_does() {
    let mut host = Host::new();
    let owner = host.tr_focused();
    host.click(&ids::fullscreen(owner.0));
    assert!(crate::video_fullscreen(&host.ui, &host.store));
    // A click on the picture only wakes it: it stays full screen.
    host.click(&ids::stream_tile(owner.0));
    assert!(crate::video_fullscreen(&host.ui, &host.store));
    assert!(host.vz_usable(ids::FULLSCREEN_EXIT));
    host.tr_rest(3_000);
    assert!(!host.vz_shows(ids::FULLSCREEN_EXIT));
    assert_eq!(host.tree.cursor(), Some(limn::Cursor::Hidden));
    host.send(UiEvent::PointerMove(Point::new(10.0, 10.0)));
    host.click(ids::FULLSCREEN_EXIT);
    assert!(!crate::video_fullscreen(&host.ui, &host.store));
    // Its own window: its title goes once the pointer rests over it.
    host.click(&ids::window(owner.0));
    let size = limn::Size::new(960.0, 540.0);
    let shows_title = |host: &Host| {
        let view = crate::video_window(&host.ui, &host.store, owner, size, host.now).unwrap();
        let mut tree = limn::UiTree::new(view);
        tree.set_reduced_motion(true);
        tree.layout(size).commands.iter().any(|command| {
            matches!(command, limn::DrawCommand::Text { text, .. } if text.starts_with("Pantalla de "))
        })
    };
    crate::video_window_stirred(&mut host.ui, owner, host.now);
    assert!(shows_title(&host));
    host.now += Duration::from_millis(3_000);
    assert!(!shows_title(&host) && crate::video_window_idle(&host.ui, owner, host.now));
    crate::video_window_stirred(&mut host.ui, owner, host.now);
    assert!(shows_title(&host));
}

#[test]
fn the_own_card_and_a_stream_watched_take_the_stage_one_at_a_time() {
    let mut host = Host::new();
    let room = host.vz_join("Sala común");
    host.click(&format!("voice:{room}"));
    let owners = host.tr_streamers(room, 4, 2);
    host.tr_watch(room, owners[0]);
    // Sharing: the account's stream is a card in the grid, with its way to
    // stop, and no picture of its own until it is clicked.
    host.click(ids::SCREEN);
    host.click("vz:quality:720");
    host.click(ids::SHARE);
    let started = crate::video_screen_ready(&host.ui, &mut host.store, true);
    host.run(started);
    assert!(host.vz_shows(ids::OWN_STREAM_STOP));
    assert!(!host.vz_shows(ids::STOP_SCREEN));
    assert!(host.vz_texts().contains(&"¡Estás en directo!".to_owned()));
    // A click on the card: its stream takes the stage, live, and the card goes.
    host.click(ids::OWN_STREAM);
    assert!(host.ui.video.own_focus);
    assert!(host.vz_shows(ids::STOP_SCREEN) && !host.vz_shows(ids::OWN_STREAM_STOP));
    assert!(host.vz_texts().contains(&"Tu pantalla".to_owned()));
    // Watching one focuses that one instead; the card comes back.
    host.click(&ids::stream_tile(owners[0].0));
    assert!(!host.ui.video.own_focus);
    assert_eq!(host.ui.video.spotlight, Some(owners[0]));
    // In the strip it is a small card: its title, no picture and no button.
    assert!(host.vz_texts().contains(&"¡Estás en directo!".to_owned()));
    assert!(!host.vz_shows(ids::OWN_STREAM_STOP) && !host.vz_shows(ids::STOP_SCREEN));
    // Its own grid again once the watched one lets go of the stage.
    host.click(&ids::stream_tile(owners[0].0));
    assert_eq!(host.ui.video.spotlight, None);
    assert!(host.vz_shows(ids::OWN_STREAM_STOP));
    // Stopping the share takes the card away with it.
    host.click(ids::OWN_STREAM_STOP);
    assert!(!host.vz_shows(ids::OWN_STREAM_STOP) && host.store.sharing().is_none());
}

#[test]
fn a_persons_tile_takes_the_color_of_their_avatar_and_their_accent_without_one() {
    let mut host = Host::new();
    let user = host.store.me().unwrap();
    let source = rune_model::RemoteImage {
        url: "https://cdn.example.test/tile.png".into(),
        kind: rune_model::ImageKind::Artwork,
    };
    let mut account = host.store.user(user).unwrap().clone();
    account.avatar = Some(source.clone());
    host.event(Event::UserChanged(account));
    let tile =
        |host: &Host| crate::voice::tile_accent(&host.ui, &host.store, None, user, &host.ui.look());
    let before = tile(&host);
    host.ui
        .image_colors
        .insert(source, rune_model::Rgb(0x12_34_56));
    assert_eq!(tile(&host), host.ui.look().ink(rune_model::Rgb(0x12_34_56)));
    assert_ne!(tile(&host), before);
}
